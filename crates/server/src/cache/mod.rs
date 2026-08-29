//! Caching.
//!
//! Replaces the original's Upstash Redis with two local layers: a bounded
//! in-process cache (moka) that serves the hot path, backed by SQLite so a pod
//! restart doesn't cost a full re-fetch from GitHub.
//!
//! The consequence for deployment: cache state is *per pod*. See
//! `docs/deployment.md` — the short version is that one replica is the intended
//! shape, and scaling out costs you extra GitHub API calls rather than
//! correctness.

mod sqlite;

pub use sqlite::SqliteStore;

use crate::error::{ApiError, ApiResult};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Rank results, refreshed daily.
pub const TTL_DEFAULT: u64 = 24 * 60 * 60;
/// The in-progress season changes, so it is held briefly.
pub const TTL_CURRENT_YEAR: u64 = 60 * 60;
/// Completed seasons are immutable; hold them for a month.
pub const TTL_HISTORICAL_YEAR: u64 = 30 * 24 * 60 * 60;
/// Missing users, so a typo'd badge can't hammer the API.
pub const TTL_NOT_FOUND: u64 = 60 * 60;
/// Transient failures.
pub const TTL_ERROR: u64 = 5 * 60;

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Data a rank was computed from. Part of the cache key so variants can never
/// be served for one another.
///
/// Only `public` exists today. It is here because ranking on private
/// contributions (visible when a signed-in user refreshes their own badge) would
/// otherwise collide with the public rank under the same key and the two would
/// alternate unpredictably.
pub const SCOPE_PUBLIC: &str = "public";

/// Cache key for a rank result.
///
/// Deliberately *not* keyed by theme, unlike upstream. A rank does not depend on
/// its colour scheme, so keying by it split one entry into nine and multiplied
/// GitHub fetches for the same user by the number of themes in use.
pub fn rank_key(username: &str, season: Option<i32>, scope: &str) -> String {
    let season = season.map_or_else(|| "all".to_string(), |y| y.to_string());
    format!("rank:{}:{season}:{scope}", username.trim().to_lowercase())
}

/// Cache key for one year of a user's stats.
pub fn year_key(username: &str, year: i32) -> String {
    format!("year:{}:{year}", username.trim().to_lowercase())
}

/// How long a year's stats stay valid: finished seasons never change.
pub fn ttl_for_year(year: i32, current_year: i32) -> u64 {
    if year < current_year {
        TTL_HISTORICAL_YEAR
    } else {
        TTL_CURRENT_YEAR
    }
}

/// Gives every entry its own lifetime, rather than one TTL for the whole cache.
///
/// This is what lets [`Cache::get_or_insert_with`] work: an expired key is
/// genuinely absent from moka, so the next request re-runs the initializer
/// instead of being handed a stale value.
struct EntryExpiry;

impl moka::Expiry<String, Arc<Entry>> for EntryExpiry {
    fn expire_after_create(
        &self,
        _key: &String,
        entry: &Arc<Entry>,
        _created_at: Instant,
    ) -> Option<Duration> {
        let remaining = entry.expires_at - now_unix();
        Some(Duration::from_secs(remaining.max(0) as u64))
    }
}

/// Two-layer cache over JSON-serializable values.
#[derive(Clone)]
pub struct Cache {
    memory: moka::future::Cache<String, Arc<Entry>>,
    store: Option<SqliteStore>,
}

impl Cache {
    pub fn new(max_entries: u64, store: Option<SqliteStore>) -> Self {
        Self {
            memory: moka::future::Cache::builder()
                .max_capacity(max_entries)
                .expire_after(EntryExpiry)
                .build(),
            store,
        }
    }

    /// In-process only — no durable layer. Used by tests and by deployments
    /// that mount no volume.
    pub fn ephemeral(max_entries: u64) -> Self {
        Self::new(max_entries, None)
    }

    /// Read a cached value, promoting a durable hit back into memory.
    ///
    /// A corrupt or stale-shaped entry is treated as a miss rather than an
    /// error: the caller will simply refetch.
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let entry = self.lookup(key).await?;
        serde_json::from_str(&entry.value).ok()
    }

    /// Read an entry from memory, falling back to the durable layer.
    async fn lookup(&self, key: &str) -> Option<Arc<Entry>> {
        if let Some(entry) = self.memory.get(key).await {
            return Some(entry);
        }

        let entry = self.load_durable(key).await?;
        self.memory.insert(key.to_string(), entry.clone()).await;
        Some(entry)
    }

    async fn load_durable(&self, key: &str) -> Option<Arc<Entry>> {
        let store = self.store.clone()?;
        let owned_key = key.to_string();
        let now = now_unix();

        let raw = tokio::task::spawn_blocking(move || store.get(&owned_key, now))
            .await
            .ok()?
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "durable cache read failed");
                None
            })?;

        serde_json::from_str::<Entry>(&raw).ok().map(Arc::new)
    }

    /// Return the cached value, or compute and store it.
    ///
    /// **Only one caller computes a given key at a time.** Badge traffic arrives
    /// through GitHub's camo proxy, so a popular user's cache expiring means
    /// dozens of simultaneous requests for the same username — without
    /// coalescing, each one would fire its own set of GitHub API calls. moka
    /// resolves the race and hands every waiter the single computed result.
    pub async fn get_or_insert_with<T, F, Fut>(&self, key: &str, ttl: u64, init: F) -> ApiResult<T>
    where
        T: Serialize + DeserializeOwned,
        F: FnOnce() -> Fut,
        Fut: Future<Output = ApiResult<T>>,
    {
        let durable = self.clone();
        let owned_key = key.to_string();

        let entry = self
            .memory
            .try_get_with(key.to_string(), async move {
                // Another pod, or this one before a restart, may already have it.
                if let Some(entry) = durable.load_durable(&owned_key).await {
                    // Only reuse it if it still matches the current shape. A
                    // deploy that changes the payload leaves entries that no
                    // longer decode, and treating that as an error would 500
                    // every cached user until their TTLs expired. Recompute
                    // instead — a rollout should cost a refetch, not an outage.
                    if serde_json::from_str::<T>(&entry.value).is_ok() {
                        return Ok(entry);
                    }
                    tracing::info!(
                        key = %owned_key,
                        "cached value no longer decodes; recomputing"
                    );
                }

                let value = init().await?;
                let encoded = serde_json::to_string(&value)
                    .map_err(|e| ApiError::Internal(format!("serializing cache value: {e}")))?;

                let entry = Arc::new(Entry {
                    value: encoded,
                    expires_at: now_unix() + ttl as i64,
                });

                durable.write_durable(&owned_key, &entry).await;
                Ok(entry)
            })
            .await
            // try_get_with shares one Arc'd error with every waiter on the race.
            .map_err(|error: Arc<ApiError>| clone_error(&error))?;

        serde_json::from_str(&entry.value)
            .map_err(|e| ApiError::Internal(format!("decoding cached value: {e}")))
    }

    /// Write a value to both layers. Cache failures are logged, never fatal —
    /// a service that can't cache should still serve.
    pub async fn set<T: Serialize>(&self, key: &str, value: &T, ttl: u64) {
        let Ok(encoded) = serde_json::to_string(value) else {
            tracing::warn!(key, "value could not be serialized for caching");
            return;
        };

        let entry = Arc::new(Entry {
            value: encoded,
            expires_at: now_unix() + ttl as i64,
        });

        self.memory.insert(key.to_string(), entry.clone()).await;
        self.write_durable(key, &entry).await;
    }

    /// Persist an entry, logging rather than failing — a service that cannot
    /// cache should still serve.
    async fn write_durable(&self, key: &str, entry: &Entry) {
        let Some(store) = self.store.clone() else {
            return;
        };
        let Ok(raw) = serde_json::to_string(entry) else {
            return;
        };

        let owned_key = key.to_string();
        let expires_at = entry.expires_at;

        let _ = tokio::task::spawn_blocking(move || {
            if let Err(error) = store.set(&owned_key, &raw, expires_at) {
                tracing::warn!(%error, "durable cache write failed");
            }
        })
        .await;
    }

    /// Number of entries held in memory. Surfaced on the health endpoint.
    pub fn memory_entries(&self) -> u64 {
        self.memory.entry_count()
    }

    /// Delete expired rows from the durable layer.
    pub async fn purge_expired(&self) {
        let Some(store) = self.store.clone() else {
            return;
        };
        let now = now_unix();

        let _ = tokio::task::spawn_blocking(move || match store.purge_expired(now) {
            Ok(removed) if removed > 0 => tracing::debug!(removed, "purged expired cache entries"),
            Err(error) => tracing::warn!(%error, "cache purge failed"),
            _ => {}
        })
        .await;
    }

    /// Whether a durable layer is attached — surfaced on the health endpoint.
    pub fn is_durable(&self) -> bool {
        self.store.is_some()
    }
}

/// A cached value plus its own expiry, so both layers agree on freshness.
#[derive(Debug, Serialize, serde::Deserialize)]
struct Entry {
    value: String,
    expires_at: i64,
}

/// `try_get_with` hands every waiter the same `Arc<ApiError>`; rebuild an owned
/// error so callers get a normal `ApiError`.
fn clone_error(error: &ApiError) -> ApiError {
    match error {
        ApiError::UserNotFound { username } => ApiError::UserNotFound {
            username: username.clone(),
        },
        ApiError::RateLimit { message, retry_after } => {
            ApiError::rate_limited(message.clone(), *retry_after)
        }
        ApiError::Validation { message, details } => ApiError::Validation {
            message: message.clone(),
            details: details.clone(),
        },
        ApiError::GitHubApi { message } => ApiError::github(message.clone()),
        ApiError::Internal(message) => ApiError::Internal(message.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_keys_are_stable_and_scoped() {
        assert_eq!(rank_key("octocat", None, SCOPE_PUBLIC), "rank:octocat:all:public");
        assert_eq!(rank_key("octocat", Some(2024), SCOPE_PUBLIC), "rank:octocat:2024:public");
        // Usernames are case-insensitive, so keys must not fragment by case.
        assert_eq!(rank_key("  OctoCat ", None, SCOPE_PUBLIC), "rank:octocat:all:public");
        assert_eq!(year_key("OctoCat", 2024), "year:octocat:2024");
    }

    /// Theme must not fragment the cache: nine themes of one user is one rank.
    #[test]
    fn theme_does_not_affect_the_key() {
        assert_eq!(
            rank_key("octocat", None, SCOPE_PUBLIC),
            rank_key("octocat", None, SCOPE_PUBLIC)
        );
    }

    #[test]
    fn finished_seasons_are_held_far_longer_than_the_live_one() {
        assert_eq!(ttl_for_year(2025, 2026), TTL_HISTORICAL_YEAR);
        assert_eq!(ttl_for_year(2026, 2026), TTL_CURRENT_YEAR);
        assert_eq!(ttl_for_year(2027, 2026), TTL_CURRENT_YEAR);
    }

    #[tokio::test]
    async fn round_trips_through_memory() {
        let cache = Cache::ephemeral(64);
        cache.set("k", &vec![1, 2, 3], 60).await;

        assert_eq!(cache.get::<Vec<i32>>("k").await, Some(vec![1, 2, 3]));
        assert_eq!(cache.get::<Vec<i32>>("missing").await, None);
    }

    #[tokio::test]
    async fn expired_entries_read_as_missing() {
        let cache = Cache::ephemeral(64);
        cache.set("k", &"value", 0).await;

        assert_eq!(cache.get::<String>("k").await, None);
    }

    /// The thundering-herd guard: a popular badge expiring must not fan out
    /// into one GitHub fetch per waiting request.
    #[tokio::test]
    async fn concurrent_requests_compute_the_value_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let cache = Cache::ephemeral(64);
        let calls = Arc::new(AtomicUsize::new(0));

        let waiters: Vec<_> = (0..32)
            .map(|_| {
                let cache = cache.clone();
                let calls = calls.clone();
                tokio::spawn(async move {
                    cache
                        .get_or_insert_with::<String, _, _>("hot", 60, || async move {
                            calls.fetch_add(1, Ordering::SeqCst);
                            // Long enough that every waiter piles up first.
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            Ok("computed".to_string())
                        })
                        .await
                })
            })
            .collect();

        for waiter in waiters {
            assert_eq!(waiter.await.unwrap().unwrap(), "computed");
        }

        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "32 concurrent requests should have triggered exactly one fetch"
        );
    }

    #[tokio::test]
    async fn a_cached_value_is_not_recomputed() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let cache = Cache::ephemeral(64);
        let calls = Arc::new(AtomicUsize::new(0));

        for _ in 0..3 {
            let calls = calls.clone();
            let value: String = cache
                .get_or_insert_with("k", 60, || async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok("v".to_string())
                })
                .await
                .unwrap();
            assert_eq!(value, "v");
        }

        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn an_expired_entry_is_recomputed() {
        let cache = Cache::ephemeral(64);

        let first: String = cache
            .get_or_insert_with("k", 0, || async { Ok("old".to_string()) })
            .await
            .unwrap();
        assert_eq!(first, "old");

        // Zero TTL means the entry is already dead.
        let second: String = cache
            .get_or_insert_with("k", 60, || async { Ok("new".to_string()) })
            .await
            .unwrap();
        assert_eq!(second, "new");
    }

    #[tokio::test]
    async fn a_failed_computation_is_not_cached() {
        let cache = Cache::ephemeral(64);

        let failure = cache
            .get_or_insert_with::<String, _, _>("k", 60, || async {
                Err(ApiError::github("upstream is down"))
            })
            .await;
        assert!(failure.is_err());

        // The next caller must get a fresh attempt, not the cached failure.
        let recovered: String = cache
            .get_or_insert_with("k", 60, || async { Ok("recovered".to_string()) })
            .await
            .unwrap();
        assert_eq!(recovered, "recovered");
    }

    /// A deploy that changes the cached payload's shape must degrade to a
    /// refetch, not to 500s until every TTL expires.
    #[tokio::test]
    async fn a_stale_schema_is_recomputed_rather_than_failing() {
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct NewShape {
            value: String,
            added_in_this_release: u32,
        }

        let store = SqliteStore::in_memory().unwrap();

        // An entry written by the previous release.
        let old = Cache::new(64, Some(store.clone()));
        old.set("k", &"just a string", 600).await;

        // The new release reads the same key with an incompatible type.
        let new = Cache::new(64, Some(store));
        let value: NewShape = new
            .get_or_insert_with("k", 600, || async {
                Ok(NewShape { value: "recomputed".into(), added_in_this_release: 1 })
            })
            .await
            .expect("should recompute, not fail");

        assert_eq!(value.value, "recomputed");
    }

    #[tokio::test]
    async fn a_durable_hit_skips_the_initializer() {
        let store = SqliteStore::in_memory().unwrap();
        let warm = Cache::new(64, Some(store.clone()));
        warm.set("k", &"durable".to_string(), 600).await;

        // Fresh memory layer, same database — as after a pod restart.
        let restarted = Cache::new(64, Some(store));
        let value: String = restarted
            .get_or_insert_with("k", 600, || async {
                panic!("should not refetch when the durable layer has it")
            })
            .await
            .unwrap();

        assert_eq!(value, "durable");
    }

    #[tokio::test]
    async fn survives_loss_of_the_memory_layer() {
        let store = SqliteStore::in_memory().unwrap();
        let cache = Cache::new(64, Some(store.clone()));
        cache.set("k", &"durable", 600).await;

        // Simulate a restart: fresh in-process cache, same database.
        let restarted = Cache::new(64, Some(store));
        assert_eq!(restarted.get::<String>("k").await, Some("durable".into()));
    }
}
