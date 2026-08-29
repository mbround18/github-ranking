//! HTTP surface and middleware stack.

pub mod badge;
pub mod health;
pub mod request_id;

use crate::state::AppState;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderValue, Method, header};
use axum::routing::get;
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::LatencyUnit;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::sensitive_headers::SetSensitiveRequestHeadersLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::{DefaultOnResponse, TraceLayer};

/// Build the application router with its full middleware stack.
///
/// Layer order matters, outermost first: panics are caught *inside* tracing so
/// a panic is still logged against its request id, and the timeout sits inside
/// compression so a slow handler aborts before we begin encoding a body.
pub fn router(state: AppState) -> Router {
    let config = state.config.clone();

    // Nested rather than merged: a merged router's fallback is replaced by the
    // outer one, which sent unknown /api paths to the static file server and
    // produced an HTML-flavoured 404 instead of the JSON error shape.
    let api = Router::new()
        // Badges are embedded in READMEs; this path is a public contract and
        // must keep working exactly as it did.
        .route("/rank/{username}", get(badge::badge))
        .route("/v1/rank/{username}", get(badge::rank_json))
        .fallback(badge::not_found);

    // The Kubernetes probe surface. Deliberately outside /api so the API's
    // JSON-error fallback never shadows them.
    let ops = Router::new()
        .route("/healthz", get(health::healthz))
        .route("/readyz", get(health::readyz))
        .route("/startupz", get(health::startupz))
        .route("/metrics", get(health::metrics));

    let stack = ServiceBuilder::new()
        // Never let a credential reach a log line.
        .layer(SetSensitiveRequestHeadersLayer::new([
            header::AUTHORIZATION,
            header::COOKIE,
        ]))
        .layer(
            TraceLayer::new_for_http().on_response(
                DefaultOnResponse::new()
                    .level(tracing::Level::INFO)
                    .latency_unit(LatencyUnit::Millis),
            ),
        )
        // A panic in one handler must not take down the process.
        .layer(CatchPanicLayer::new())
        .layer(axum::middleware::from_fn(request_id::propagate))
        .layer(axum::middleware::from_fn_with_state(state.clone(), observe))
        // Badges are embedded cross-origin by design.
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([Method::GET, Method::HEAD, Method::OPTIONS])
                .allow_headers(Any)
                .max_age(Duration::from_secs(86_400)),
        )
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        // SVG and JSON both compress heavily; a card is mostly path data.
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::GATEWAY_TIMEOUT,
            config.request_timeout,
        ))
        // Shed load before GitHub's rate limiter or our memory does.
        .layer(tower::limit::GlobalConcurrencyLimitLayer::new(
            config.max_concurrent_requests,
        ))
        // Every endpoint is a GET; nothing should be sending us a body.
        .layer(DefaultBodyLimit::max(1024));

    Router::new()
        .nest("/api", api)
        // Upstream exposed this as a rewrite onto the same handler.
        .route("/badge/{username}", get(badge::badge))
        .merge(ops)
        .fallback_service(spa(&state))
        .layer(stack)
        .with_state(state)
}

/// Count every request and time it.
///
/// Sits inside the request-id layer so a slow request can be correlated with its
/// trace, and outside the handler so failures are counted too.
async fn observe(
    axum::extract::State(state): axum::extract::State<AppState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    // Scrapes would otherwise inflate their own counters.
    let is_scrape = request.uri().path() == "/metrics";
    if !is_scrape {
        state.metrics.record_request();
    }

    let started = std::time::Instant::now();
    let response = next.run(request).await;

    if !is_scrape {
        state
            .metrics
            .record_response(response.status().as_u16(), started.elapsed());
    }

    response
}

/// Serve the built frontend, falling back to `index.html` so client-side routes
/// like `/octocat` resolve on a hard refresh.
fn spa(state: &AppState) -> ServeDir<ServeFile> {
    let root = &state.config.web_root;
    ServeDir::new(root).fallback(ServeFile::new(root.join("index.html")))
}
