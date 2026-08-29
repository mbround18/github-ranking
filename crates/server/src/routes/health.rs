//! Liveness and readiness.
//!
//! Split deliberately, because Kubernetes treats them differently: a failing
//! liveness probe restarts the pod, a failing readiness probe only takes it out
//! of the load balancer. Running out of GitHub quota is a readiness problem —
//! restarting would not help and would throw away a warm cache.

use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde_json::json;

/// Liveness: the process is up and serving. Never depends on anything external.
///
/// A failure here restarts the pod, so it must not consult GitHub, the cache, or
/// anything else that can be broken without the process being at fault.
pub async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "status": "ok" })))
}

/// Startup: initialization finished.
///
/// Separate from liveness so the startup probe can be given a generous
/// `failureThreshold` for a slow first boot without also loosening the liveness
/// deadline for the rest of the pod's life.
pub async fn startupz(State(state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
            "status": "started",
            "uptimeSeconds": state.started_at.elapsed().as_secs(),
            "version": env!("CARGO_PKG_VERSION"),
            // Which build this is. Compile-time capabilities are otherwise
            // invisible once deployed, and "why did it refuse my token?" is
            // much easier to answer when the answer is on an endpoint.
            "build": {
                "patInProduction": crate::auth::pat_allowed_in_production(),
            },
        })),
    )
}

/// Prometheus scrape endpoint.
pub async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let body = state
        .metrics
        .encode(state.auth.available(), state.cache.memory_entries());

    (
        StatusCode::OK,
        // The version parameter is what Prometheus negotiates on.
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
}

/// Readiness: we can actually serve a badge right now.
pub async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    let available = state.auth.available();
    let ready = available > 0;

    let body = json!({
        "status": if ready { "ready" } else { "degraded" },
        "credentials": {
            "kind": state.auth.kind().as_str(),
            "available": available,
        },
        "cache": {
            "durable": state.cache.is_durable(),
            "entries": state.cache.memory_entries(),
        },
        "uptimeSeconds": state.started_at.elapsed().as_secs(),
    });

    // Serving from cache still works without quota, but we would rather traffic
    // went to a pod that can also handle a miss.
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status, Json(body))
}
