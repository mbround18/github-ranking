//! GraphQL query construction.
//!
//! The original issued one request per contribution year — a 15-year account
//! cost 16 round trips. GitHub's GraphQL API lets you alias the same field
//! multiple times in one document, so every year is fetched in a single request
//! instead:
//!
//! ```graphql
//! y2024: contributionsCollection(from: "…", to: "…") { ...Contributions }
//! y2023: contributionsCollection(from: "…", to: "…") { ...Contributions }
//! ```
//!
//! That takes a full profile to **two** requests regardless of account age: one
//! to learn which years have contributions, one to fetch them all. It matters
//! more than it looks — GraphQL quota is per credential, and once ranks are
//! refreshed with a signed-in user's own token, each user has their own budget
//! to stay inside.

/// Years per request. `contributionsCollection` is not a connection so each
/// alias is cheap, but keeping batches bounded avoids tripping GitHub's query
/// complexity limits on very old accounts.
pub const YEARS_PER_BATCH: usize = 12;

/// The contribution counters, requested identically for every year.
const CONTRIBUTION_FRAGMENT: &str = "
fragment Contributions on ContributionsCollection {
  totalCommitContributions
  totalPullRequestContributions
  totalPullRequestReviewContributions
  totalIssueContributions
  restrictedContributionsCount
}
";

/// Which years a user has contributions in, plus the profile-level totals that
/// are not year-scoped.
///
/// Stars are all-time by nature, so they are fetched once here rather than per
/// year.
pub const PROFILE_QUERY: &str = r#"
query Profile($login: String!) {
  user(login: $login) {
    login
    name
    createdAt
    contributionsCollection {
      contributionYears
    }
    followers {
      totalCount
    }
    repositories(
      first: 100
      ownerAffiliations: OWNER
      orderBy: { field: STARGAZERS, direction: DESC }
    ) {
      totalCount
      nodes {
        stargazers {
          totalCount
        }
      }
    }
  }
  rateLimit {
    limit
    cost
    remaining
    resetAt
  }
}
"#;

/// Accurate per-year counts of *merged* PRs and *closed* issues.
///
/// The v1 scoring metrics named `totalMergedPRs` and `totalIssuesClosed` are
/// misnomers — they come from `totalPullRequestContributions` and
/// `totalIssueContributions`, which count PRs and issues a user **opened**, with
/// no peer validation at all. See `docs/scoring.md`.
///
/// The search API gives the real figures, one aliased node per year, so
/// collecting them costs no extra requests. They are stored but deliberately
/// not scored: changing what the numbers mean would re-rank every existing
/// badge, which is a v2 decision rather than a silent migration.
pub fn verified_counts_fragment(login: &str, year: i32) -> String {
    format!(
        "    merged{year}: search(query: \"author:{login} is:pr is:merged merged:{year}-01-01..{year}-12-31\", type: ISSUE, first: 0) {{ issueCount }}\n\
             closed{year}: search(query: \"author:{login} is:issue is:closed closed:{year}-01-01..{year}-12-31\", type: ISSUE, first: 0) {{ issueCount }}\n"
    )
}

/// The alias used for a year's contributions, e.g. `y2024`.
pub fn year_alias(year: i32) -> String {
    format!("y{year}")
}

/// Build one query fetching every listed year's contributions at once.
///
/// Years are interpolated rather than passed as variables because GraphQL has
/// no way to parameterise a field alias. They are `i32`s formatted by us, never
/// user-supplied text, so there is nothing injectable here.
pub fn batched_stats_query(years: &[i32]) -> String {
    let mut query = String::from("query YearStats($login: String!) {\n  user(login: $login) {\n");

    for &year in years {
        query.push_str(&format!(
            "    {alias}: contributionsCollection(from: \"{year}-01-01T00:00:00Z\", to: \"{year}-12-31T23:59:59Z\") {{ ...Contributions }}\n",
            alias = year_alias(year),
        ));
    }

    query.push_str("  }\n  rateLimit {\n    limit\n    cost\n    remaining\n    resetAt\n  }\n}\n");
    query.push_str(CONTRIBUTION_FRAGMENT);
    query
}

/// Split years into request-sized batches.
pub fn batches(years: &[i32]) -> impl Iterator<Item = &[i32]> {
    years.chunks(YEARS_PER_BATCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batched_query_aliases_every_year() {
        let query = batched_stats_query(&[2023, 2024, 2025]);

        for year in [2023, 2024, 2025] {
            assert!(query.contains(&format!("y{year}: contributionsCollection")));
            assert!(query.contains(&format!("\"{year}-01-01T00:00:00Z\"")));
            assert!(query.contains(&format!("\"{year}-12-31T23:59:59Z\"")));
        }

        // One document, one fragment, one rateLimit block.
        assert_eq!(query.matches("fragment Contributions").count(), 1);
        assert_eq!(query.matches("rateLimit").count(), 1);
        assert_eq!(query.matches("contributionsCollection").count(), 3);
    }

    #[test]
    fn a_long_lived_account_still_fits_in_two_requests() {
        // GitHub launched in 2008; this is about as old as an account gets.
        let years: Vec<i32> = (2008..=2026).collect();
        assert_eq!(years.len(), 19);

        let batch_count = batches(&years).count();
        assert_eq!(batch_count, 2, "19 years should batch into 2 requests");
    }

    #[test]
    fn typical_account_is_a_single_batch() {
        let years: Vec<i32> = (2019..=2026).collect();
        assert_eq!(batches(&years).count(), 1);
    }

    #[test]
    fn empty_year_list_produces_no_batches() {
        assert_eq!(batches(&[]).count(), 0);
    }
}
