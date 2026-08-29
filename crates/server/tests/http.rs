//! HTTP-level tests.
//!
//! These drive the real router through `tower`'s `oneshot`, so the middleware
//! stack is exercised exactly as in production — no network needed, because
//! every case here resolves before GitHub would be called.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use github_ranked::auth::{
    AuthError, AuthProvider, Credential, CredentialId, CredentialKind, RateLimitStatus,
};
use github_ranked::config::{Config, Environment};

use github_ranked::routes::router;
use github_ranked::state::AppState;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

/// Stands in for a credential source without touching the network.
struct StubAuth {
    available: usize,
}

#[async_trait::async_trait]
impl AuthProvider for StubAuth {
    async fn credential(&self) -> Result<Credential, AuthError> {
        if self.available == 0 {
            return Err(AuthError::Exhausted { retry_after: Some(60) });
        }
        Ok(Credential::new(
            CredentialId { kind: CredentialKind::Pat, index: 0 },
            "stub-token",
        ))
    }

    fn record(&self, _id: CredentialId, _status: RateLimitStatus) {}
    fn record_spend(&self, _id: CredentialId, _points: u32) {}
    fn available(&self) -> usize {
        self.available
    }
    fn kind(&self) -> CredentialKind {
        CredentialKind::Pat
    }
}

fn app(available: usize) -> axum::Router {
    let config = Config {
        environment: Environment::Development,
        bind: "127.0.0.1:0".parse().unwrap(),
        // Memory-only: tests must not touch the filesystem.
        cache_path: None,
        cache_max_entries: 128,
        web_root: "./nonexistent-web-root".into(),
        request_timeout: Duration::from_secs(5),
        max_concurrent_requests: 32,
    };

    let state = AppState::new(config, Arc::new(StubAuth { available })).unwrap();
    router(state)
}

async fn get(path: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    let response = app(1)
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();

    (status, headers, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn liveness_never_depends_on_anything_external() {
    let response = app(0)
        .oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn readiness_fails_when_no_credential_can_serve_a_miss() {
    let ready = app(1)
        .oneshot(Request::builder().uri("/readyz").body(Body::empty()).unwrap())
        .await.unwrap();
    assert_eq!(ready.status(), StatusCode::OK);

    // Out of quota is a readiness problem, not a liveness one — restarting
    // would only throw away a warm cache.
    let degraded = app(0)
        .oneshot(Request::builder().uri("/readyz").body(Body::empty()).unwrap())
        .await.unwrap();
    assert_eq!(degraded.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn malformed_usernames_are_rejected_before_github_is_called() {
    for username in ["-leading", "trailing-", "a--b"] {
        let (status, _, body) = get(&format!("/api/rank/{username}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{username} should be rejected");

        let json: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["error"], "ValidationError");
        assert_eq!(json["code"], 400);
        assert!(json["requestId"].as_str().is_some_and(|id| !id.is_empty()));
    }
}

#[tokio::test]
async fn out_of_range_seasons_are_rejected() {
    let (status, _, body) = get("/api/rank/octocat?season=1999").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let json: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["error"], "ValidationError");
    assert!(json["details"]["hint"].as_str().unwrap().contains("between"));
}

/// A bad theme must not break someone's README: it falls back and still
/// renders, so validation passes and we get as far as fetching.
#[tokio::test]
async fn an_unknown_theme_is_not_a_client_error() {
    let (status, _, _) = get("/api/rank/octocat?theme=not-a-real-theme").await;
    assert_ne!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn every_response_carries_a_request_id() {
    let (_, headers, _) = get("/healthz").await;
    assert!(headers.contains_key("x-request-id"));
}

#[tokio::test]
async fn an_upstream_request_id_is_adopted() {
    let response = app(1)
        .oneshot(Request::builder().uri("/healthz")
            .header("x-request-id", "trace-from-the-proxy")
            .body(Body::empty()).unwrap())
        .await.unwrap();
    assert_eq!(response.headers()["x-request-id"], "trace-from-the-proxy");
}

#[tokio::test]
async fn a_hostile_request_id_is_not_reflected() {
    let response = app(1)
        .oneshot(Request::builder().uri("/healthz")
            .header("x-request-id", "x".repeat(500))
            .body(Body::empty()).unwrap())
        .await.unwrap();

    let id = response.headers()["x-request-id"].to_str().unwrap();
    assert!(id.len() < 100, "oversized id was reflected: {} chars", id.len());
}

#[tokio::test]
async fn security_headers_are_always_present() {
    let (_, headers, _) = get("/healthz").await;
    assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert_eq!(headers[header::X_FRAME_OPTIONS], "DENY");
}

/// Badges are embedded cross-origin by definition.
#[tokio::test]
async fn badges_are_cors_enabled() {
    let (_, headers, _) = get("/api/rank/-invalid").await;
    assert_eq!(headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
}

#[tokio::test]
async fn errors_are_cached_so_failures_are_not_retried_in_a_loop() {
    let (_, headers, _) = get("/api/rank/-invalid").await;
    let cache_control = headers[header::CACHE_CONTROL].to_str().unwrap();
    assert!(cache_control.contains("max-age"), "got {cache_control}");
}

#[tokio::test]
async fn unknown_api_routes_return_the_json_error_shape() {
    let (status, _, body) = get("/api/does-not-exist").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Same JSON envelope as every other error, so clients parse one shape.
    let json: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["error"], "NotFound");
    assert!(json["requestId"].as_str().is_some_and(|id| !id.is_empty()));
    assert!(json["details"]["hint"].as_str().unwrap().contains("/api/rank/"));
}

#[tokio::test]
async fn the_legacy_badge_path_is_still_routed() {
    // Upstream rewrote /badge/{user} onto the rank handler; READMEs may use it.
    let (status, _, _) = get("/badge/-invalid").await;
    assert_eq!(status, StatusCode::BAD_REQUEST,
        "should reach the handler and fail validation, not 404");
}

#[tokio::test]
async fn startup_probe_reports_version_and_uptime() {
    let (status, _, body) = get("/startupz").await;

    assert_eq!(status, StatusCode::OK);
    let json: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["status"], "started");
    assert!(json["uptimeSeconds"].is_number());
    assert!(json["version"].as_str().is_some_and(|v| !v.is_empty()));
}

#[tokio::test]
async fn metrics_are_exposed_in_prometheus_format() {
    let app = app(1);

    // Generate some traffic first so the counters aren't all zero.
    for path in ["/healthz", "/api/rank/-invalid", "/api/nope"] {
        let _ = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
    }

    let response = app
        .oneshot(Request::builder().uri("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers()[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .starts_with("text/plain"));

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8_lossy(&bytes);

    assert!(body.contains("github_ranked_requests_total 3"));
    assert!(body.contains(r#"github_ranked_responses_total{class="4xx"} 2"#));
    assert!(body.contains("github_ranked_credentials_available 1"));
    assert!(body.contains("github_ranked_request_duration_seconds_bucket"));
}

/// Scraping must not count as traffic, or the metrics measure the monitoring.
#[tokio::test]
async fn scrapes_do_not_inflate_their_own_counters() {
    let app = app(1);

    for _ in 0..3 {
        let _ = app
            .clone()
            .oneshot(Request::builder().uri("/metrics").body(Body::empty()).unwrap())
            .await
            .unwrap();
    }

    let response = app
        .oneshot(Request::builder().uri("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8_lossy(&bytes);

    assert!(body.contains("github_ranked_requests_total 0"));
}

/// The probe paths must not be shadowed by the API fallback or the SPA.
#[tokio::test]
async fn every_kubernetes_probe_path_is_routed() {
    for path in ["/healthz", "/readyz", "/startupz", "/metrics"] {
        let (status, _, _) = get(path).await;
        assert_eq!(status, StatusCode::OK, "{path} should be routed");
    }
}
