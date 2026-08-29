//! Durable cache layer.
//!
//! SQLite stands in for the Upstash Redis the original used. It is not a
//! performance layer — the in-process cache in front of it absorbs the hot path
//! — its job is to survive a pod restart or rollout without re-fetching every
//! badge from the GitHub API.

use rusqlite::{Connection, OptionalExtension};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct SqliteStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStore {
    /// Open (or create) the cache database.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let conn = Connection::open(path)?;

        // WAL keeps reads from blocking behind the periodic purge. NORMAL sync
        // is the right trade for a cache: a lost write just means a refetch.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "busy_timeout", 5_000)?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cache_entries (
                 key        TEXT PRIMARY KEY,
                 value      TEXT NOT NULL,
                 expires_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_cache_expires
                 ON cache_entries (expires_at);",
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// An in-memory database, for tests.
    pub fn in_memory() -> rusqlite::Result<Self> {
        Self::open(Path::new(":memory:"))
    }

    /// Fetch a live entry. Expired rows are treated as absent and left for the
    /// purge to collect.
    pub fn get(&self, key: &str, now: i64) -> rusqlite::Result<Option<String>> {
        let conn = self.conn.lock().expect("cache mutex poisoned");
        conn.query_row(
            "SELECT value FROM cache_entries WHERE key = ?1 AND expires_at > ?2",
            (key, now),
            |row| row.get(0),
        )
        .optional()
    }

    pub fn set(&self, key: &str, value: &str, expires_at: i64) -> rusqlite::Result<()> {
        let conn = self.conn.lock().expect("cache mutex poisoned");
        conn.execute(
            "INSERT INTO cache_entries (key, value, expires_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value,
                                            expires_at = excluded.expires_at",
            (key, value, expires_at),
        )?;
        Ok(())
    }

    /// Drop expired rows. Returns how many were removed.
    pub fn purge_expired(&self, now: i64) -> rusqlite::Result<usize> {
        let conn = self.conn.lock().expect("cache mutex poisoned");
        conn.execute("DELETE FROM cache_entries WHERE expires_at <= ?1", (now,))
    }

    pub fn len(&self) -> rusqlite::Result<i64> {
        let conn = self.conn.lock().expect("cache mutex poisoned");
        conn.query_row("SELECT COUNT(*) FROM cache_entries", (), |row| row.get(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_honours_expiry() {
        let store = SqliteStore::in_memory().unwrap();

        store.set("rank:octocat:all:default", "{\"tier\":\"Gold\"}", 100).unwrap();

        assert_eq!(
            store.get("rank:octocat:all:default", 50).unwrap().as_deref(),
            Some("{\"tier\":\"Gold\"}")
        );
        // At and past the expiry the entry is invisible.
        assert_eq!(store.get("rank:octocat:all:default", 100).unwrap(), None);
        assert_eq!(store.get("rank:octocat:all:default", 150).unwrap(), None);
    }

    #[test]
    fn set_overwrites_rather_than_duplicating() {
        let store = SqliteStore::in_memory().unwrap();

        store.set("k", "first", 100).unwrap();
        store.set("k", "second", 200).unwrap();

        assert_eq!(store.get("k", 50).unwrap().as_deref(), Some("second"));
        assert_eq!(store.len().unwrap(), 1);
    }

    #[test]
    fn purge_removes_only_expired_rows() {
        let store = SqliteStore::in_memory().unwrap();

        store.set("stale", "x", 100).unwrap();
        store.set("fresh", "y", 900).unwrap();

        assert_eq!(store.purge_expired(500).unwrap(), 1);
        assert_eq!(store.get("fresh", 500).unwrap().as_deref(), Some("y"));
        assert_eq!(store.len().unwrap(), 1);
    }
}
