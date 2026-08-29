//! GitHub GraphQL transport.
//!
//! Owns credential leasing, retries and rate-limit accounting. Everything above
//! this layer just asks for a query and gets data back.

use crate::auth::{AuthProvider, RateLimitStatus};
use crate::error::{ApiError, ApiResult};
use crate::metrics::Metrics;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

const GRAPHQL_ENDPOINT: &str = "https://api.github.com/graphql";
const USER_AGENT: &str = concat!("github-ranked/", env!("CARGO_PKG_VERSION"));

/// Attempts per query, including the first.
const MAX_ATTEMPTS: u32 = 3;
/// Base delay for exponential backoff between attempts.
const BACKOFF_BASE: Duration = Duration::from_millis(250);
/// Never sleep longer than this waiting to retry, even if GitHub asks us to —
/// beyond it we fail fast and let the response be cached as an error.
const MAX_BACKOFF: Duration = Duration::from_secs(8);

/// What GitHub reports about our remaining GraphQL budget.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimit {
    pub limit: u32,
    pub cost: u32,
    pub remaining: u32,
    pub reset_at: String,
}

impl RateLimit {
    /// `reset_at` is RFC 3339; we only need the epoch seconds.
    fn reset_epoch(&self) -> i64 {
        parse_rfc3339_epoch(&self.reset_at).unwrap_or_else(|| crate::cache::now_unix() + 3_600)
    }

    fn status(&self) -> RateLimitStatus {
        RateLimitStatus {
            remaining: self.remaining,
            reset_at: self.reset_epoch(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct GraphQlError {
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    message: String,
}

#[derive(Debug, Deserialize)]
struct GraphQlResponse {
    #[serde(default)]
    data: Option<Value>,
    #[serde(default)]
    errors: Vec<GraphQlError>,
}

pub struct GitHubClient {
    http: reqwest::Client,
    auth: Arc<dyn AuthProvider>,
    metrics: Arc<Metrics>,
}

impl GitHubClient {
    pub fn new(auth: Arc<dyn AuthProvider>, metrics: Arc<Metrics>) -> ApiResult<Self> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            // Reuse connections: badge traffic is bursty and TLS handshakes
            // dominate otherwise.
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .map_err(|e| ApiError::Internal(format!("building HTTP client: {e}")))?;

        Ok(Self {
            http,
            auth,
            metrics,
        })
    }

    /// Run a GraphQL query and deserialize `data` into `T`.
    ///
    /// Retries transient failures with backoff. Rate-limit and not-found
    /// responses are returned immediately — they are answers, not failures.
    pub async fn query<T: DeserializeOwned>(&self, query: &str, variables: Value) -> ApiResult<T> {
        let body = json!({ "query": query, "variables": variables });
        let mut last_error = None;

        for attempt in 0..MAX_ATTEMPTS {
            if attempt > 0 {
                tokio::time::sleep(backoff(attempt)).await;
            }

            match self.attempt(&body).await {
                Ok(data) => {
                    return serde_json::from_value(data).map_err(|e| {
                        ApiError::github(format!("unexpected response shape from GitHub: {e}"))
                    });
                }
                // A definitive answer — retrying cannot change it.
                Err(error @ (ApiError::UserNotFound { .. } | ApiError::RateLimit { .. })) => {
                    return Err(error);
                }
                Err(error) => {
                    tracing::warn!(attempt = attempt + 1, %error, "GitHub query failed");
                    last_error = Some(error);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| ApiError::github("GitHub query failed after retries")))
    }

    /// One request/response cycle.
    async fn attempt(&self, body: &Value) -> ApiResult<Value> {
        let credential = self.auth.credential().await?;
        self.metrics.record_github_request();

        let response = self
            .http
            .post(GRAPHQL_ENDPOINT)
            .bearer_auth(credential.expose())
            .json(body)
            .send()
            .await
            .map_err(|e| ApiError::github(format!("GitHub request failed: {e}")))?;

        let status = response.status();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());

        // GitHub signals both primary and secondary rate limits with 403/429.
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.as_u16() == 403 {
            self.auth.record(
                credential.id,
                RateLimitStatus {
                    remaining: 0,
                    reset_at: crate::cache::now_unix() + retry_after.unwrap_or(60) as i64,
                },
            );
            self.metrics.record_github_rate_limited();
            return Err(ApiError::rate_limited(
                "GitHub rate limit exceeded",
                retry_after.or(Some(60)),
            ));
        }

        if status == reqwest::StatusCode::UNAUTHORIZED {
            // Don't let a revoked credential be retried forever.
            self.auth.record(
                credential.id,
                RateLimitStatus {
                    remaining: 0,
                    reset_at: crate::cache::now_unix() + 3_600,
                },
            );
            return Err(ApiError::github("GitHub rejected the credential"));
        }

        if !status.is_success() {
            self.metrics.record_github_error();
            return Err(ApiError::github(format!("GitHub returned HTTP {status}")));
        }

        let payload: GraphQlResponse = response
            .json()
            .await
            .map_err(|e| ApiError::github(format!("malformed GitHub response: {e}")))?;

        // Record quota before handling errors — a failed query still costs.
        if let Some(rate_limit) = payload
            .data
            .as_ref()
            .and_then(|d| d.get("rateLimit"))
            .and_then(|v| serde_json::from_value::<RateLimit>(v.clone()).ok())
        {
            tracing::debug!(
                cost = rate_limit.cost,
                remaining = rate_limit.remaining,
                credential = %credential.id,
                "GitHub quota"
            );
            self.auth.record(credential.id, rate_limit.status());
        } else {
            // No authoritative figure came back; assume we spent a point.
            self.auth.record_spend(credential.id, 1);
        }

        if let Some(error) = payload.errors.first() {
            return Err(classify(error));
        }

        payload
            .data
            .ok_or_else(|| ApiError::github("GitHub returned no data"))
    }
}

/// Map a GraphQL error onto our own error type.
fn classify(error: &GraphQlError) -> ApiError {
    let kind = error.r#type.as_deref().unwrap_or_default();

    match kind {
        "NOT_FOUND" => ApiError::UserNotFound {
            // The caller substitutes the real username; the GraphQL error does
            // not reliably carry it.
            username: String::new(),
        },
        "RATE_LIMITED" => ApiError::rate_limited("GitHub rate limit exceeded", Some(60)),
        _ => ApiError::github(format!("GitHub GraphQL error: {}", error.message)),
    }
}

fn backoff(attempt: u32) -> Duration {
    (BACKOFF_BASE * 2u32.saturating_pow(attempt - 1)).min(MAX_BACKOFF)
}

/// Extract epoch seconds from an RFC 3339 timestamp like
/// `2026-08-28T15:04:05Z`.
///
/// GitHub always returns UTC here, so a full date-time parser would be more
/// machinery than the one format we actually receive.
fn parse_rfc3339_epoch(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() < 20 || bytes[4] != b'-' || bytes[10] != b'T' {
        return None;
    }

    let num = |range: std::ops::Range<usize>| value.get(range)?.parse::<i64>().ok();

    let (year, month, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hour, minute, second) = (num(11..13)?, num(14..16)?, num(17..19)?);

    Some(days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// Days since the Unix epoch, via Howard Hinnant's civil-from-days algorithm.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_github_reset_timestamps() {
        assert_eq!(parse_rfc3339_epoch("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            parse_rfc3339_epoch("2026-08-28T15:04:05Z"),
            Some(1787929445)
        );
        // Leap day, and a leap-year boundary.
        assert_eq!(
            parse_rfc3339_epoch("2024-02-29T00:00:00Z"),
            Some(1709164800)
        );
        assert_eq!(parse_rfc3339_epoch("2000-03-01T00:00:00Z"), Some(951868800));
        // Past the 32-bit cliff, since these are reset times in the future.
        assert_eq!(
            parse_rfc3339_epoch("2038-01-19T03:14:07Z"),
            Some(2147483647)
        );
    }

    #[test]
    fn rejects_timestamps_it_cannot_trust() {
        assert_eq!(parse_rfc3339_epoch(""), None);
        assert_eq!(parse_rfc3339_epoch("not-a-timestamp-at-all"), None);
        assert_eq!(parse_rfc3339_epoch("2026/08/28 15:04:05"), None);
    }

    #[test]
    fn backoff_grows_then_caps() {
        assert_eq!(backoff(1), Duration::from_millis(250));
        assert_eq!(backoff(2), Duration::from_millis(500));
        assert_eq!(backoff(3), Duration::from_millis(1_000));
        assert!(backoff(20) <= MAX_BACKOFF);
    }

    #[test]
    fn missing_users_are_distinguished_from_real_failures() {
        let not_found = classify(&GraphQlError {
            r#type: Some("NOT_FOUND".into()),
            message: "Could not resolve to a User".into(),
        });
        assert!(matches!(not_found, ApiError::UserNotFound { .. }));

        let other = classify(&GraphQlError {
            r#type: Some("INTERNAL".into()),
            message: "boom".into(),
        });
        assert!(matches!(other, ApiError::GitHubApi { .. }));
    }
}
