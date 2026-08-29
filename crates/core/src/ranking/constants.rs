//! Constants for the Dev-Elo ranking system.
//!
//! These values are load-bearing: they define every published rank. Changing one
//! silently re-ranks every user, so they are pinned by the golden fixtures in
//! `fixtures/` which were generated from the original TypeScript implementation.

use super::types::{Division, Tier};

/// Mean of log-transformed global developer activity.
pub const MEAN_LOG_SCORE: f64 = 6.5;

/// Standard deviation of log-transformed scores.
pub const STD_DEV: f64 = 1.5;

/// Base Elo rating — the median developer (Gold IV).
pub const BASE_ELO: f64 = 1200.0;

/// Elo points per standard deviation.
pub const ELO_PER_SIGMA: f64 = 400.0;

/// Weights for the Weighted Performance Index, normalized to 100%.
///
/// Collaboration (PRs + reviews) is 70% by design: it makes Diamond+ unreachable
/// without peer interaction. Commits are weighted low to resist farming.
pub mod weights {
    pub const MERGED_PRS: f64 = 35.0;
    pub const CODE_REVIEWS: f64 = 35.0;
    pub const ISSUES_CLOSED: f64 = 15.0;
    pub const COMMITS: f64 = 10.0;
    pub const STARS: f64 = 5.0;
}

/// Stars beyond this stop contributing, so star-heavy repos can't dominate.
/// The card still *displays* the true total.
pub const MAX_STARS_CAP: f64 = 1_000.0;

/// Git Points range within a division.
pub const MAX_GP: f64 = 99.0;
pub const MIN_GP: f64 = 0.0;

/// Elo range for a tier: `[min, max)`.
pub struct TierRange {
    pub min: f64,
    pub max: f64,
}

/// Elo thresholds per tier. Challenger is open-ended.
pub fn tier_range(tier: Tier) -> TierRange {
    let (min, max) = match tier {
        Tier::Iron => (0.0, 600.0),
        Tier::Bronze => (600.0, 900.0),
        Tier::Silver => (900.0, 1200.0),
        Tier::Gold => (1200.0, 1500.0),
        Tier::Platinum => (1500.0, 1700.0),
        Tier::Emerald => (1700.0, 2000.0),
        Tier::Diamond => (2000.0, 2400.0),
        Tier::Master => (2400.0, 2600.0),
        Tier::Grandmaster => (2600.0, 3000.0),
        Tier::Challenger => (3000.0, f64::INFINITY),
    };
    TierRange { min, max }
}

/// Gradient and accent colors for a tier: `([gradient start, gradient end], accent)`.
pub fn tier_colors(tier: Tier) -> ([&'static str; 2], &'static str) {
    match tier {
        Tier::Iron => (["#3a3a3a", "#1a1a1a"], "#5c5c5c"),
        Tier::Bronze => (["#8B4513", "#CD7F32"], "#D4A574"),
        Tier::Silver => (["#C0C0C0", "#A8A8A8"], "#E8E8E8"),
        Tier::Gold => (["#FFD700", "#FDB931"], "#FFF4B8"),
        Tier::Platinum => (["#00CED1", "#20B2AA"], "#7FFFD4"),
        Tier::Emerald => (["#50C878", "#2E8B57"], "#98FB98"),
        Tier::Diamond => (["#B9F2FF", "#00D4FF"], "#E0FFFF"),
        Tier::Master => (["#9932CC", "#8B008B"], "#DA70D6"),
        Tier::Grandmaster => (["#DC143C", "#8B0000"], "#FF6B6B"),
        Tier::Challenger => (["#FFD700", "#FF8C00"], "#FFF700"),
    }
}

/// Seasonal decay multipliers — a League-style soft reset where older
/// contributions progressively stop counting.
pub mod decay {
    pub const CURRENT_SEASON: f64 = 1.0;
    pub const PREVIOUS_SEASON: f64 = 0.6;
    pub const TWO_SEASONS_AGO: f64 = 0.35;
    pub const THREE_SEASONS_AGO: f64 = 0.2;
    pub const LEGACY: f64 = 0.1;
}

/// Decay multiplier applied to a contribution year, relative to `current_year`.
pub fn seasonal_decay_multiplier(year: i32, current_year: i32) -> f64 {
    match current_year - year {
        y if y <= 0 => decay::CURRENT_SEASON,
        1 => decay::PREVIOUS_SEASON,
        2 => decay::TWO_SEASONS_AGO,
        3 => decay::THREE_SEASONS_AGO,
        _ => decay::LEGACY,
    }
}

/// All four divisions, lowest first.
pub const DIVISIONS_ASC: [Division; 4] = [Division::IV, Division::III, Division::II, Division::I];
