//! Per-request correlation ids.
//!
//! Every response carries `x-request-id`, and every log line for that request is
//! tagged with it. Error bodies echo the same value, so a user reporting "my
//! badge is broken" can hand over an id that finds the exact trace.

use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;

/// The id assigned to the request currently being handled.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Header a proxy or load balancer may set upstream of us.
const HEADER: &str = "x-request-id";

/// Adopt an inbound request id when there is one, so traces join up across a
/// proxy; otherwise mint a fresh one.
pub async fn propagate(mut request: Request, next: Next) -> Response {
    let inbound = request
        .headers()
        .get(HEADER)
        .and_then(|value| value.to_str().ok())
        // Bound it: this ends up in logs and response headers, and it arrives
        // from outside.
        .filter(|value| !value.is_empty() && value.len() <= 128 && value.is_ascii())
        .map(str::to_string);

    let id = RequestId(inbound.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()));

    request.extensions_mut().insert(id.clone());

    let mut response = next.run(request).await;

    if let Ok(value) = HeaderValue::from_str(&id.0) {
        response.headers_mut().insert(HEADER, value);
    }

    response
}
