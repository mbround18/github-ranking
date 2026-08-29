//! WebAssembly bindings for the frontend.
//!
//! The browser runs the *same* ranking and rendering code the server does, so a
//! live theme preview or a "what would my rank be" calculator cannot drift from
//! the badge that actually gets served.
//!
//! Nothing here talks to GitHub. Fetching stats needs a credential, and
//! credentials stay server-side — the browser is handed stats and does maths on
//! them.

use github_ranked_core::ranking;
use github_ranked_core::render::card::{CardInput, render_card};
use github_ranked_core::validation;
use serde::Deserialize;
use wasm_bindgen::prelude::*;

/// What the browser passes in to draw a card.
///
/// Shaped like the `/api/v1/rank/{username}` response so the frontend can hand
/// an API payload straight back without reshaping it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CardRequest {
    username: String,
    rank: ranking::RankResult,
    stats: ranking::AggregatedStats,
    #[serde(default)]
    season: Option<i32>,
    current_year: i32,
    #[serde(default)]
    theme: String,
}

/// Render a rank card to SVG, in the browser.
///
/// This is the *same* renderer the badge endpoint runs, compiled to wasm, so a
/// theme previewed here is byte-identical to the badge that will be served —
/// and switching themes costs no round trip and no GitHub quota.
#[wasm_bindgen(js_name = renderCard)]
pub fn render_card_wasm(request: JsValue) -> Result<String, JsError> {
    let request: CardRequest = serde_wasm_bindgen::from_value(request)
        .map_err(|e| JsError::new(&format!("invalid card request: {e}")))?;

    // An unknown theme falls back rather than erroring, matching the server.
    let theme = validation::parse_theme(Some(&request.theme));

    Ok(render_card(&CardInput {
        username: &request.username,
        rank: &request.rank,
        stats: &request.stats,
        theme,
        season: request.season,
        current_year: request.current_year,
    }))
}

/// Rank a user from their contribution totals.
///
/// Takes and returns plain JS objects, shaped exactly like the JSON the API
/// serves, so the frontend can pass an API response straight back in.
#[wasm_bindgen(js_name = calculateRank)]
pub fn calculate_rank(stats: JsValue) -> Result<JsValue, JsError> {
    let stats: ranking::AggregatedStats = serde_wasm_bindgen::from_value(stats)
        .map_err(|e| JsError::new(&format!("invalid stats: {e}")))?;

    let rank = ranking::calculate_rank(&stats);

    serde_wasm_bindgen::to_value(&rank).map_err(|e| JsError::new(&e.to_string()))
}

/// The Elo threshold a user needs to reach the next tier, for progress UI.
#[wasm_bindgen(js_name = nextTierAt)]
pub fn next_tier_at(elo: f64) -> Option<f64> {
    let tier = ranking::get_tier(elo);
    let max = ranking::constants::tier_range(tier).max;
    max.is_finite().then_some(max)
}

/// The weight a contribution year still carries, given the current season.
///
/// Exposed rather than reimplemented in TypeScript: the dashboard shows these
/// weights beside the raw counts, and a copy would silently disagree with the
/// scores the server computes the moment the schedule changed.
#[wasm_bindgen(js_name = seasonalDecay)]
pub fn seasonal_decay(year: i32, current_year: i32) -> f64 {
    github_ranked_core::ranking::constants::seasonal_decay_multiplier(year, current_year)
}

/// Validate a username client-side, so the UI can reject a typo before it costs
/// a round trip. The server validates again regardless.
#[wasm_bindgen(js_name = isValidUsername)]
pub fn is_valid_username(username: &str) -> bool {
    validation::is_valid_username(username)
}

/// Every theme name the renderer accepts, for populating a picker.
#[wasm_bindgen(js_name = themeNames)]
pub fn theme_names() -> Vec<String> {
    validation::Theme::ALL
        .iter()
        .map(|t| t.as_str().to_string())
        .collect()
}
