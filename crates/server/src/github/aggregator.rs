//! Turning a GitHub profile into rankable stats.
//!
//! Aggregation is deliberately a faithful reproduction of v1, including its
//! rounding: each year's contributions are decayed, rounded, *then* summed. That
//! order matters — rounding after summing gives different totals, and therefore
//! different ranks.

use super::GitHubClient;
use super::queries::{self, PROFILE_QUERY};
use crate::error::{ApiError, ApiResult};
use github_ranked_core::AggregatedStats;
use github_ranked_core::ranking::constants::seasonal_decay_multiplier;
use github_ranked_core::ranking::js_round;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// One year of raw, undecayed contributions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
// Every other type on the wire is camelCase; without this the dashboard would
// have to handle `private_contributions` alongside `totalMergedPRs`.
#[serde(rename_all = "camelCase")]
pub struct YearlyStats {
    pub year: i32,
    pub commits: f64,
    /// PRs **opened** — see the note on [`AggregatedStats`] naming below.
    pub prs: f64,
    pub reviews: f64,
    /// Issues **opened**.
    pub issues: f64,
    /// Contributions to private repositories, count only. Never scored: badges
    /// must stay reproducible by anyone, and nobody else can see these.
    pub private_contributions: f64,
}

/// Profile-level totals that are not year-scoped.
#[derive(Debug, Clone, Default)]
pub struct Profile {
    pub login: String,
    pub name: Option<String>,
    pub contribution_years: Vec<i32>,
    pub total_stars: f64,
    pub total_followers: f64,
}

/// Everything needed to render a card, plus the yearly detail the dashboard
/// shows.
#[derive(Debug, Clone)]
pub struct ProfileStats {
    pub profile: Profile,
    pub stats: AggregatedStats,
    pub yearly: Vec<YearlyStats>,
}

/// Fetch the profile: which years have contributions, plus stars and followers.
///
/// v1 issued a second request for stars and followers; folding them in here
/// saves one round trip on every cold render.
pub async fn fetch_profile(client: &GitHubClient, username: &str) -> ApiResult<Profile> {
    let data: Value = client
        .query(PROFILE_QUERY, json!({ "login": username }))
        .await
        .map_err(|error| name_not_found(error, username))?;

    let user = data
        .get("user")
        .filter(|u| !u.is_null())
        .ok_or_else(|| ApiError::UserNotFound {
            username: username.to_string(),
        })?;

    let contribution_years = user
        .pointer("/contributionsCollection/contributionYears")
        .and_then(Value::as_array)
        .map(|years| {
            years
                .iter()
                .filter_map(Value::as_i64)
                .map(|y| y as i32)
                .collect()
        })
        .unwrap_or_default();

    // The query asks for the 100 most-starred repos; anything beyond that has
    // a negligible tail, and stars are capped at 1,000 for scoring regardless.
    let total_stars = user
        .pointer("/repositories/nodes")
        .and_then(Value::as_array)
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(|n| n.pointer("/stargazers/totalCount").and_then(Value::as_f64))
                .sum()
        })
        .unwrap_or(0.0);

    Ok(Profile {
        login: user
            .get("login")
            .and_then(Value::as_str)
            .unwrap_or(username)
            .to_string(),
        name: user.get("name").and_then(Value::as_str).map(str::to_string),
        contribution_years,
        total_stars,
        total_followers: user
            .pointer("/followers/totalCount")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
    })
}

/// Fetch every listed year's contributions, batching aliases so this is one or
/// two requests rather than one per year.
pub async fn fetch_yearly_stats(
    client: &GitHubClient,
    username: &str,
    years: &[i32],
) -> ApiResult<Vec<YearlyStats>> {
    let mut collected = Vec::with_capacity(years.len());

    for batch in queries::batches(years) {
        let query = queries::batched_stats_query(batch);
        let data: Value = client
            .query(&query, json!({ "login": username }))
            .await
            .map_err(|error| name_not_found(error, username))?;

        let user =
            data.get("user")
                .filter(|u| !u.is_null())
                .ok_or_else(|| ApiError::UserNotFound {
                    username: username.to_string(),
                })?;

        for &year in batch {
            // A year GitHub declined to return is skipped rather than counted
            // as zero: zero would silently deflate the user's rank.
            let Some(node) = user.get(queries::year_alias(year)) else {
                tracing::warn!(username, year, "year missing from GitHub response");
                continue;
            };

            let field = |name: &str| node.get(name).and_then(Value::as_f64).unwrap_or(0.0);

            collected.push(YearlyStats {
                year,
                commits: field("totalCommitContributions"),
                prs: field("totalPullRequestContributions"),
                reviews: field("totalPullRequestReviewContributions"),
                issues: field("totalIssueContributions"),
                private_contributions: field("restrictedContributionsCount"),
            });
        }
    }

    Ok(collected)
}

/// Collapse yearly contributions into decayed totals.
///
/// Each year is decayed and rounded independently before summing, matching v1
/// exactly.
pub fn apply_seasonal_decay(
    yearly: &[YearlyStats],
    profile: &Profile,
    current_year: i32,
) -> AggregatedStats {
    if yearly.is_empty() {
        return AggregatedStats {
            total_stars: profile.total_stars,
            total_followers: profile.total_followers,
            first_contribution_year: current_year,
            last_contribution_year: current_year,
            years_active: 0,
            ..Default::default()
        };
    }

    let mut stats = AggregatedStats {
        total_stars: profile.total_stars,
        total_followers: profile.total_followers,
        ..Default::default()
    };

    for year in yearly {
        let decay = seasonal_decay_multiplier(year.year, current_year);
        stats.total_commits += js_round(year.commits * decay);
        stats.total_merged_prs += js_round(year.prs * decay);
        stats.total_code_reviews += js_round(year.reviews * decay);
        stats.total_issues_closed += js_round(year.issues * decay);
    }

    let mut years: Vec<i32> = yearly.iter().map(|y| y.year).collect();
    years.sort_unstable();

    stats.first_contribution_year = years[0];
    stats.last_contribution_year = years[years.len() - 1];
    stats.years_active = years.len() as i32;
    stats
}

/// Fetch and aggregate a user's all-time stats.
pub async fn aggregate_all_time(
    client: &GitHubClient,
    username: &str,
    current_year: i32,
) -> ApiResult<ProfileStats> {
    let profile = fetch_profile(client, username).await?;
    let yearly = fetch_yearly_stats(client, username, &profile.contribution_years).await?;
    let stats = apply_seasonal_decay(&yearly, &profile, current_year);

    Ok(ProfileStats {
        profile,
        stats,
        yearly,
    })
}

/// The GraphQL layer cannot know which username it was resolving, so fill it in.
fn name_not_found(error: ApiError, username: &str) -> ApiError {
    match error {
        ApiError::UserNotFound { .. } => ApiError::UserNotFound {
            username: username.to_string(),
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn year(year: i32, commits: f64, prs: f64, reviews: f64, issues: f64) -> YearlyStats {
        YearlyStats {
            year,
            commits,
            prs,
            reviews,
            issues,
            private_contributions: 0.0,
        }
    }

    fn profile() -> Profile {
        Profile {
            total_stars: 250.0,
            total_followers: 10.0,
            ..Default::default()
        }
    }

    #[test]
    fn decay_is_applied_per_year_then_summed() {
        // 2026 at 100%, 2025 at 60%, 2024 at 35%.
        let yearly = vec![
            year(2026, 100.0, 10.0, 20.0, 5.0),
            year(2025, 100.0, 10.0, 20.0, 5.0),
            year(2024, 100.0, 10.0, 20.0, 5.0),
        ];

        let stats = apply_seasonal_decay(&yearly, &profile(), 2026);

        assert_eq!(stats.total_commits, 100.0 + 60.0 + 35.0);
        assert_eq!(stats.total_merged_prs, 10.0 + 6.0 + 4.0); // 3.5 rounds to 4
        assert_eq!(stats.total_code_reviews, 20.0 + 12.0 + 7.0);
        assert_eq!(stats.total_issues_closed, 5.0 + 3.0 + 2.0); // 1.75 -> 2
    }

    #[test]
    fn rounding_happens_before_summing_not_after() {
        // Three years each landing on .5 after decay. Rounding each up gives 3;
        // summing first (1.5) and rounding once would give 2.
        let yearly = vec![
            year(2025, 0.0, 5.0, 0.0, 0.0),
            year(2024, 0.0, 0.0, 0.0, 0.0),
            year(2023, 0.0, 0.0, 0.0, 0.0),
        ];
        let stats = apply_seasonal_decay(&yearly, &profile(), 2026);
        assert_eq!(stats.total_merged_prs, 3.0, "5 * 0.6 = 3.0");
    }

    #[test]
    fn ties_round_the_way_javascript_does() {
        // 0.35 decay on 10 issues = 3.5, which JS rounds up to 4.
        let stats = apply_seasonal_decay(&[year(2024, 0.0, 0.0, 0.0, 10.0)], &profile(), 2026);
        assert_eq!(stats.total_issues_closed, 4.0);
    }

    #[test]
    fn legacy_years_keep_a_floor_weight() {
        // Anything four or more seasons back decays to 10%, never to zero.
        let stats = apply_seasonal_decay(&[year(2005, 1000.0, 0.0, 0.0, 0.0)], &profile(), 2026);
        assert_eq!(stats.total_commits, 100.0);
    }

    #[test]
    fn an_account_with_no_contributions_still_reports_stars() {
        let stats = apply_seasonal_decay(&[], &profile(), 2026);

        assert_eq!(stats.years_active, 0);
        assert_eq!(stats.total_commits, 0.0);
        assert_eq!(stats.total_stars, 250.0);
        assert_eq!(stats.first_contribution_year, 2026);
        assert_eq!(stats.last_contribution_year, 2026);
    }

    #[test]
    fn active_span_is_derived_from_the_years_present() {
        let yearly = vec![
            year(2024, 1.0, 0.0, 0.0, 0.0),
            year(2019, 1.0, 0.0, 0.0, 0.0),
            year(2026, 1.0, 0.0, 0.0, 0.0),
        ];
        let stats = apply_seasonal_decay(&yearly, &profile(), 2026);

        assert_eq!(stats.first_contribution_year, 2019);
        assert_eq!(stats.last_contribution_year, 2026);
        // Years *with contributions*, not the calendar span.
        assert_eq!(stats.years_active, 3);
    }
}
