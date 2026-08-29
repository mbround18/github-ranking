//! Shared application state.

use crate::auth::AuthProvider;
use crate::cache::{Cache, SqliteStore};
use crate::config::Config;
use crate::error::{ApiError, ApiResult};
use crate::github::GitHubClient;
use crate::metrics::Metrics;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

pub struct Inner {
    pub config: Config,
    pub cache: Cache,
    pub github: GitHubClient,
    pub auth: Arc<dyn AuthProvider>,
    pub metrics: Arc<Metrics>,
    /// When the process began serving, for uptime reporting.
    pub started_at: std::time::Instant,
}

impl std::ops::Deref for AppState {
    type Target = Inner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AppState {
    pub fn new(config: Config, auth: Arc<dyn AuthProvider>) -> ApiResult<Self> {
        let store = match &config.cache_path {
            Some(path) => match SqliteStore::open(path) {
                Ok(store) => {
                    tracing::info!(path = %path.display(), "durable cache open");
                    Some(store)
                }
                // A missing volume should degrade to memory-only, not refuse to
                // boot — the service is still correct without persistence.
                Err(error) => {
                    tracing::error!(%error, path = %path.display(), "durable cache unavailable; continuing in memory only");
                    None
                }
            },
            None => {
                tracing::warn!("no CACHE_PATH set; cache will not survive a restart");
                None
            }
        };

        let cache = Cache::new(config.cache_max_entries, store);
        let metrics = Arc::new(Metrics::new());
        let github = GitHubClient::new(auth.clone(), metrics.clone())?;

        Ok(Self(Arc::new(Inner {
            config,
            cache,
            github,
            auth,
            metrics,
            started_at: std::time::Instant::now(),
        })))
    }

    /// Sweep expired rows from the durable cache. Run periodically.
    pub async fn purge_cache(&self) {
        self.cache.purge_expired().await;
    }
}

/// Convenience for handlers that only need to report configuration problems.
pub fn misconfigured(what: &str) -> ApiError {
    ApiError::Internal(format!("misconfigured: {what}"))
}
