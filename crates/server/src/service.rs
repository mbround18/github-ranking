//! Rank computation, cached.
//!
//! Sits between the HTTP handlers and GitHub: handlers ask for a rank, this
//! decides whether that means a cache read or a fetch.

use crate::cache::{self, SCOPE_PUBLIC, TTL_DEFAULT};
use crate::config::current_year;
use crate::error::ApiResult;
use crate::github::aggregator::{aggregate_all_time, YearlyStats};
use crate::state::AppState;
use github_ranked_core::ranking::calculate_rank;
use github_ranked_core::{AggregatedStats, RankResult};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Whether a rank came from cache or had to be computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheOutcome {
    Hit,
    Miss,
}

impl CacheOutcome {
    pub fn as_header(self) -> &'static str {
        match self {
            Self::Hit => "HIT",
            Self::Miss => "MISS",
        }
    }
}

/// A computed rank, as cached and as served over the JSON API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RankPayload {
    pub username: String,
    /// GitHub's canonical casing, which may differ from what was requested.
    pub display_name: Option<String>,
    pub rank: RankResult,
    pub stats: AggregatedStats,
    /// Undecayed per-year contributions, for the dashboard's breakdown.
    pub yearly: Vec<YearlyStats>,
    /// The season this represents, or `None` for all-time.
    pub season: Option<i32>,
    /// Unix seconds when this was computed.
    pub computed_at: i64,
}

/// Fetch and rank a user, serving from cache when possible.
///
/// `force` bypasses the cache read but still writes the result back, so a
/// refresh benefits everyone rather than only the caller.
pub async fn rank_user(
    state: &AppState,
    username: &str,
    season: Option<i32>,
    force: bool,
) -> ApiResult<(RankPayload, CacheOutcome)> {
    let key = cache::rank_key(username, season, SCOPE_PUBLIC);

    if force {
        let payload = compute(state, username, season).await?;
        state.cache.set(&key, &payload, TTL_DEFAULT).await;
        state.metrics.record_cache_miss();
        return Ok((payload, CacheOutcome::Miss));
    }

    // The initializer only runs on a miss, so this reports the outcome exactly
    // rather than inferring it from timestamps.
    let computed = Arc::new(AtomicBool::new(false));

    let payload = state
        .cache
        .get_or_insert_with(&key, TTL_DEFAULT, || {
            let computed = computed.clone();
            async move {
                computed.store(true, Ordering::SeqCst);
                compute(state, username, season).await
            }
        })
        .await?;

    let outcome = if computed.load(Ordering::SeqCst) {
        state.metrics.record_cache_miss();
        CacheOutcome::Miss
    } else {
        state.metrics.record_cache_hit();
        CacheOutcome::Hit
    };

    Ok((payload, outcome))
}

async fn compute(
    state: &AppState,
    username: &str,
    season: Option<i32>,
) -> ApiResult<RankPayload> {
    // An all-time payload already carries every year, so a season can be
    // derived from it for free. Without this a `?season=` request refetched the
    // whole profile from GitHub even when the all-time rank was sitting in
    // cache — which is exactly what happened the first time this ran against
    // the real API.
    if let Some(season) = season {
        let all_time_key = cache::rank_key(username, None, SCOPE_PUBLIC);
        if let Some(all_time) = state.cache.get::<RankPayload>(&all_time_key).await {
            tracing::debug!(username, season, "derived season from cached all-time rank");
            return Ok(for_season(&all_time, season));
        }
    }

    let year = current_year();
    let profile = aggregate_all_time(&state.github, username, year).await?;

    let all_time = RankPayload {
        rank: calculate_rank(&profile.stats),
        username: profile.profile.login.clone(),
        display_name: profile.profile.name.clone(),
        stats: profile.stats.clone(),
        yearly: profile.yearly.clone(),
        season: None,
        computed_at: cache::now_unix(),
    };

    let Some(season) = season else {
        return Ok(all_time);
    };

    // We paid for the whole profile, so cache the all-time rank too rather than
    // making the next visitor fetch it again.
    let all_time_key = cache::rank_key(username, None, SCOPE_PUBLIC);
    state.cache.set(&all_time_key, &all_time, TTL_DEFAULT).await;

    Ok(for_season(&all_time, season))
}

/// Re-scope an all-time payload to a single season.
fn for_season(all_time: &RankPayload, season: i32) -> RankPayload {
    let yearly: Vec<YearlyStats> = all_time
        .yearly
        .iter()
        .copied()
        .filter(|year| year.year == season)
        .collect();

    let stats = AggregatedStats {
        total_merged_prs: yearly.iter().map(|y| y.prs).sum(),
        total_code_reviews: yearly.iter().map(|y| y.reviews).sum(),
        total_issues_closed: yearly.iter().map(|y| y.issues).sum(),
        total_commits: yearly.iter().map(|y| y.commits).sum(),
        // Stars are inherently all-time; a season cannot attribute them.
        total_stars: 0.0,
        total_followers: 0.0,
        first_contribution_year: season,
        last_contribution_year: season,
        years_active: 1,
    };

    RankPayload {
        rank: calculate_rank(&stats),
        username: all_time.username.clone(),
        display_name: all_time.display_name.clone(),
        stats,
        yearly,
        season: Some(season),
        computed_at: cache::now_unix(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use github_ranked_core::ranking::Tier;

    fn year(year: i32, prs: f64, commits: f64) -> YearlyStats {
        YearlyStats { year, commits, prs, reviews: 0.0, issues: 0.0, private_contributions: 0.0 }
    }

    fn all_time() -> RankPayload {
        let stats = AggregatedStats { total_stars: 500.0, ..Default::default() };
        RankPayload {
            rank: calculate_rank(&stats),
            username: "octocat".into(),
            display_name: Some("The Octocat".into()),
            stats,
            yearly: vec![year(2026, 60.0, 800.0), year(2025, 40.0, 400.0)],
            season: None,
            computed_at: 0,
        }
    }

    #[test]
    fn a_season_keeps_only_that_year() {
        let scoped = for_season(&all_time(), 2025);

        assert_eq!(scoped.season, Some(2025));
        assert_eq!(scoped.yearly.len(), 1);
        assert_eq!(scoped.yearly[0].year, 2025);
        assert_eq!(scoped.stats.total_merged_prs, 40.0);
        assert_eq!(scoped.stats.total_commits, 400.0);
    }

    #[test]
    fn stars_are_not_attributed_to_a_season() {
        // They are an all-time total with no per-year breakdown available.
        let scoped = for_season(&all_time(), 2026);
        assert_eq!(scoped.stats.total_stars, 0.0);
        assert_eq!(all_time().stats.total_stars, 500.0);
    }

    #[test]
    fn identity_is_carried_across() {
        let scoped = for_season(&all_time(), 2026);
        assert_eq!(scoped.username, "octocat");
        assert_eq!(scoped.display_name.as_deref(), Some("The Octocat"));
    }

    #[test]
    fn a_season_with_no_contributions_ranks_at_the_floor() {
        let scoped = for_season(&all_time(), 2019);

        assert!(scoped.yearly.is_empty());
        assert_eq!(scoped.stats.total_commits, 0.0);
        assert_eq!(scoped.rank.tier, Tier::Iron);
    }
}
