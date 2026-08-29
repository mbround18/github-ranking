//! The ranking engine: contribution totals in, tier/division/Elo out.
//!
//! This is a faithful port of the original TypeScript engine, including its
//! float semantics, so that migrating does not move anybody's rank. Where JS
//! and Rust disagree on rounding, we follow JS — see [`js_round`].

use super::constants::*;
use super::types::{AggregatedStats, Division, RankResult, Tier};

/// `Math.round` semantics: ties go toward +infinity, not away from zero.
///
/// Rust's `f64::round` rounds -1.5 to -2.0 where JS rounds it to -1. Elo is
/// clamped at 0 so the difference is unobservable today, but the ranking
/// pipeline is float-sensitive enough that it's worth not introducing the skew.
pub fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// Weighted Performance Index — the raw, unnormalized contribution score.
///
/// Floored at 1 so the log transform in [`calculate_z_score`] stays finite for
/// users with no recorded activity.
pub fn calculate_wpi(stats: &AggregatedStats) -> f64 {
    let capped_stars = stats.total_stars.min(MAX_STARS_CAP);

    let wpi = stats.total_merged_prs * weights::MERGED_PRS
        + stats.total_code_reviews * weights::CODE_REVIEWS
        + stats.total_issues_closed * weights::ISSUES_CLOSED
        + stats.total_commits * weights::COMMITS
        + capped_stars * weights::STARS;

    wpi.max(1.0)
}

/// Standard deviations from the global mean, on a log-normal fit.
pub fn calculate_z_score(wpi: f64) -> f64 {
    (wpi.ln() - MEAN_LOG_SCORE) / STD_DEV
}

/// Elo rating from a Z-score, clamped at 0.
pub fn calculate_elo(z_score: f64) -> f64 {
    js_round(BASE_ELO + z_score * ELO_PER_SIGMA).max(0.0)
}

/// The tier an Elo rating falls in.
pub fn get_tier(elo: f64) -> Tier {
    for tier in Tier::ALL_DESC {
        let range = tier_range(tier);
        if elo >= range.min && elo < range.max {
            return tier;
        }
    }
    // Unreachable for clamped Elo, but Iron is the safe floor.
    Tier::Iron
}

/// The division within a tier, or `None` for the undivided tiers (Master+).
///
/// Each tier's Elo range is split into four equal quarters, IV (lowest) to I.
pub fn get_division(elo: f64, tier: Tier) -> Option<Division> {
    if !tier.has_divisions() {
        return None;
    }

    let range = tier_range(tier);
    let division_size = (range.max - range.min) / 4.0;
    let position = elo - range.min;

    Some(if position < division_size {
        Division::IV
    } else if position < division_size * 2.0 {
        Division::III
    } else if position < division_size * 3.0 {
        Division::II
    } else {
        Division::I
    })
}

/// Git Points: progress through the current division, 0–99.
///
/// Always 0 for undivided tiers.
pub fn calculate_gp(elo: f64, tier: Tier, division: Option<Division>) -> f64 {
    let Some(division) = division.filter(|_| tier.has_divisions()) else {
        return 0.0;
    };

    let range = tier_range(tier);
    let division_size = (range.max - range.min) / 4.0;

    let division_min_elo = range.min + division.index() as f64 * division_size;
    let division_max_elo = range.min + (division.index() + 1) as f64 * division_size;
    let position_in_division = elo - division_min_elo;

    // Snap the last Elo point of a division to 99 so promotion reads cleanly.
    if elo >= division_max_elo - 1.0 {
        return MAX_GP;
    }

    let gp = (position_in_division / division_size) * (MAX_GP + 1.0);
    gp.floor().clamp(MIN_GP, MAX_GP)
}

/// Abramowitz & Stegun 7.1.26 approximation of the error function.
///
/// Accurate to ~1.5e-7, far tighter than the one decimal place percentiles are
/// reported to.
fn erf(x: f64) -> f64 {
    const A1: f64 = 0.254829592;
    const A2: f64 = -0.284496736;
    const A3: f64 = 1.421413741;
    const A4: f64 = -1.453152027;
    const A5: f64 = 1.061405429;
    const P: f64 = 0.3275911;

    let sign = if x >= 0.0 { 1.0 } else { -1.0 };
    let abs_x = x.abs();

    let t = 1.0 / (1.0 + P * abs_x);
    let y = 1.0 - ((((A5 * t + A4) * t + A3) * t + A2) * t + A1) * t * (-abs_x * abs_x).exp();

    sign * y
}

/// Percentile (0–100) from a Z-score, via the standard normal CDF.
pub fn calculate_percentile(z_score: f64) -> f64 {
    let cdf = 0.5 * (1.0 + erf(z_score / std::f64::consts::SQRT_2));
    let percentile = (cdf * 100.0).clamp(0.0, 100.0);
    js_round(percentile * 10.0) / 10.0
}

/// Rank a user from their aggregated contribution totals.
pub fn calculate_rank(stats: &AggregatedStats) -> RankResult {
    let wpi = calculate_wpi(stats);
    let z_score = calculate_z_score(wpi);
    let elo = calculate_elo(z_score);
    let tier = get_tier(elo);
    let division = get_division(elo, tier);
    let gp = calculate_gp(elo, tier, division);

    RankResult {
        tier,
        division,
        elo: elo as i64,
        gp: gp as i64,
        percentile: calculate_percentile(z_score),
        wpi,
        z_score,
    }
}
