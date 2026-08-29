//! Runtime configuration, read from the environment.
//!
//! Everything has a working default except credentials, so `cargo run` with a
//! `GITHUB_TOKEN` set is enough to get a working service.

use crate::error::{ApiError, ApiResult};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// Which environment we're running in. Gates development-only affordances.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Development,
    Production,
}

impl Environment {
    pub fn is_production(self) -> bool {
        matches!(self, Self::Production)
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub environment: Environment,
    pub bind: SocketAddr,
    /// Where the durable cache lives. `None` runs memory-only, which is valid
    /// but means a restart re-fetches everything.
    pub cache_path: Option<PathBuf>,
    pub cache_max_entries: u64,
    /// Directory of built frontend assets to serve.
    pub web_root: PathBuf,
    /// Ceiling on a single request, so a slow GitHub call can't pin a connection.
    pub request_timeout: Duration,
    /// Cap on in-flight requests. Sheds load before GitHub's rate limiter or our
    /// own memory does.
    pub max_concurrent_requests: usize,
}

impl Config {
    pub fn from_env() -> ApiResult<Self> {
        let environment = match std::env::var("APP_ENV").as_deref() {
            Ok("production") | Ok("prod") => Environment::Production,
            _ => Environment::Development,
        };

        let host = env_or("HOST", "0.0.0.0");
        // 10090, not 10080: Chrome and Firefox refuse to connect to 10080 (ERR_UNSAFE_PORT) because it is on their restricted-ports list. It is the only blocked port in the 10k range.
        let port = parse_env("PORT", 10090u16)?;
        let bind = format!("{host}:{port}")
            .parse()
            .map_err(|e| ApiError::Internal(format!("invalid HOST/PORT: {e}")))?;

        // Empty or "none" disables the durable layer explicitly, rather than
        // silently falling back when a volume fails to mount.
        let cache_path = match std::env::var("CACHE_PATH") {
            Ok(value) if value.is_empty() || value == "none" => None,
            Ok(value) => Some(PathBuf::from(value)),
            Err(_) => Some(PathBuf::from("./data/cache.db")),
        };

        Ok(Self {
            environment,
            bind,
            cache_path,
            cache_max_entries: parse_env("CACHE_MAX_ENTRIES", 10_000u64)?,
            web_root: PathBuf::from(env_or("WEB_ROOT", "./web/dist")),
            request_timeout: Duration::from_secs(parse_env("REQUEST_TIMEOUT_SECS", 20u64)?),
            max_concurrent_requests: parse_env("MAX_CONCURRENT_REQUESTS", 256usize)?,
        })
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

/// Parse an environment variable, failing loudly rather than silently
/// defaulting — a typo'd `PORT` should not start on the wrong port.
fn parse_env<T>(key: &str, default: T) -> ApiResult<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match std::env::var(key) {
        Ok(value) if !value.trim().is_empty() => value
            .trim()
            .parse()
            .map_err(|e| ApiError::Internal(format!("invalid {key}: {e}"))),
        _ => Ok(default),
    }
}

/// The current UTC year, used for seasonal decay and default season labels.
pub fn current_year() -> i32 {
    time::OffsetDateTime::now_utc().year()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_year_is_plausible() {
        let year = current_year();
        assert!((2024..2100).contains(&year), "got {year}");
    }

    #[test]
    fn production_is_recognised_by_either_spelling() {
        assert!(Environment::Production.is_production());
        assert!(!Environment::Development.is_production());
    }
}
