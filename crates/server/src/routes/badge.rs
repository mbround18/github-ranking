//! The badge and rank endpoints.

use crate::cache::{TTL_DEFAULT, TTL_NOT_FOUND};
use crate::config::current_year;
use crate::error::{ApiError, ApiResult};
use crate::routes::request_id::RequestId;
use crate::service::{CacheOutcome, rank_user};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use github_ranked_core::render::card::{CardInput, render_card};
use github_ranked_core::validation::{parse_season, parse_theme, validate_username};
use serde::Deserialize;

/// Query parameters accepted by both endpoints.
///
/// Everything is taken as a string and validated by hand rather than through
/// serde's typed parsing: a malformed `?theme=` must fall back to the default
/// and still render, because the alternative is a broken image in someone's
/// README.
#[derive(Debug, Default, Deserialize)]
pub struct BadgeParams {
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub season: Option<String>,
    #[serde(default)]
    pub force: Option<String>,
}

impl BadgeParams {
    fn force_refresh(&self) -> bool {
        matches!(self.force.as_deref(), Some("true" | "1"))
    }
}

/// An SVG card with the caching headers a badge needs.
struct SvgCard {
    svg: String,
    max_age: u64,
    outcome: CacheOutcome,
}

impl IntoResponse for SvgCard {
    fn into_response(self) -> Response {
        // `stale-while-revalidate` lets caches serve the old badge while we
        // refresh, so an expiring entry never shows a user a loading failure.
        let cache_control = format!(
            "public, max-age={0}, s-maxage={0}, stale-while-revalidate={1}",
            self.max_age,
            self.max_age / 2
        );

        (
            StatusCode::OK,
            [
                (
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("image/svg+xml; charset=utf-8"),
                ),
                (
                    header::CACHE_CONTROL,
                    HeaderValue::from_str(&cache_control).expect("ascii"),
                ),
                // GitHub's camo proxy will sniff otherwise.
                (
                    header::X_CONTENT_TYPE_OPTIONS,
                    HeaderValue::from_static("nosniff"),
                ),
                (
                    header::HeaderName::from_static("x-cache"),
                    HeaderValue::from_static(self.outcome.as_header()),
                ),
            ],
            self.svg,
        )
            .into_response()
    }
}

/// `GET /api/rank/{username}` — the badge itself.
pub async fn badge(
    State(state): State<AppState>,
    Path(username): Path<String>,
    Query(params): Query<BadgeParams>,
    Extension(request_id): Extension<RequestId>,
) -> Result<Response, ApiError> {
    let year = current_year();

    validate_username(&username)?;
    let theme = parse_theme(params.theme.as_deref());
    let season = parse_season(params.season.as_deref(), year)?;
    let force = params.force_refresh();

    tracing::debug!(%request_id, username, ?season, %theme, force, "badge requested");

    let (payload, outcome) = rank_user(&state, &username, season, force).await?;

    let svg = render_card(&CardInput {
        username: &payload.username,
        rank: &payload.rank,
        stats: &payload.stats,
        theme,
        season,
        current_year: year,
    });

    state.metrics.record_card_rendered();

    Ok(SvgCard {
        svg,
        max_age: TTL_DEFAULT,
        outcome,
    }
    .into_response())
}

/// `GET /api/v1/rank/{username}` — the same data as JSON, for the dashboard.
pub async fn rank_json(
    State(state): State<AppState>,
    Path(username): Path<String>,
    Query(params): Query<BadgeParams>,
) -> ApiResult<Response> {
    let year = current_year();

    validate_username(&username)?;
    let season = parse_season(params.season.as_deref(), year)?;

    let (payload, _) = rank_user(&state, &username, season, params.force_refresh()).await?;

    let cache_control = format!("public, max-age={TTL_DEFAULT}");
    Ok((
        StatusCode::OK,
        [(
            header::CACHE_CONTROL,
            HeaderValue::from_str(&cache_control).expect("ascii"),
        )],
        Json(payload),
    )
        .into_response())
}

/// Anything under `/api/` that doesn't match a route.
///
/// Returns the same JSON envelope as every other error so a client never has to
/// parse two shapes, but with a 404 — the endpoint is missing, the request
/// wasn't malformed.
pub async fn not_found(Extension(request_id): Extension<RequestId>) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "error": "NotFound",
            "code": 404,
            "message": "No such endpoint",
            "details": {
                "hint": "Try /api/rank/{username} or /api/v1/rank/{username}"
            },
            "requestId": request_id.to_string(),
        })),
    )
        .into_response()
}

/// Cache lifetime for an error response, so failures aren't retried in a loop.
pub fn error_ttl(error: &ApiError) -> u64 {
    match error {
        ApiError::UserNotFound { .. } => TTL_NOT_FOUND,
        _ => crate::cache::TTL_ERROR,
    }
}
