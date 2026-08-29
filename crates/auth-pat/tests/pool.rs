//! Tests for the token pool, standalone — no HTTP server, no cache, no GitHub.
//!
//! Splitting the provider into its own crate is what makes this possible: the
//! rate-limit logic is the part most likely to be subtly wrong, and it can now
//! be exercised directly.

use github_ranked_auth_core::{
    AuthError, AuthProvider, CredentialId, CredentialKind, RateLimitStatus, now_unix,
};
use github_ranked_auth_pat::PatProvider;
use proptest::prelude::*;

fn pool(count: usize) -> PatProvider {
    PatProvider::new((0..count).map(|i| format!("token-{i}")).collect()).unwrap()
}

fn id(index: usize) -> CredentialId {
    CredentialId {
        kind: CredentialKind::Pat,
        index,
    }
}

fn exhaust(pool: &PatProvider, index: usize, reset_in: i64) {
    pool.record(
        id(index),
        RateLimitStatus {
            remaining: 0,
            reset_at: now_unix() + reset_in,
        },
    );
}

#[test]
fn an_empty_pool_is_a_configuration_error() {
    let error = PatProvider::new(vec![]).unwrap_err();
    assert!(matches!(error, AuthError::Misconfigured(_)));
}

#[tokio::test]
async fn round_robins_across_tokens() {
    let pool = pool(3);

    let mut seen = Vec::new();
    for _ in 0..6 {
        seen.push(pool.credential().await.unwrap().id.index);
    }

    assert_eq!(seen, vec![0, 1, 2, 0, 1, 2]);
}

#[tokio::test]
async fn skips_exhausted_tokens() {
    let pool = pool(3);
    exhaust(&pool, 0, 3_600);
    exhaust(&pool, 2, 3_600);

    for _ in 0..5 {
        assert_eq!(pool.credential().await.unwrap().id.index, 1);
    }
    assert_eq!(pool.available(), 1);
}

#[tokio::test]
async fn reports_retry_after_when_everything_is_spent() {
    let pool = pool(2);
    exhaust(&pool, 0, 300);
    exhaust(&pool, 1, 120);

    let error = pool.credential().await.unwrap_err();
    match error {
        // The soonest window, not the latest — waiting longer than necessary
        // would idle a usable credential.
        AuthError::Exhausted { retry_after } => {
            let seconds = retry_after.expect("should know when to retry");
            assert!((100..=120).contains(&seconds), "got {seconds}");
        }
        other => panic!("expected exhaustion, got {other:?}"),
    }
}

#[tokio::test]
async fn quota_recovers_once_the_window_passes() {
    let pool = pool(1);
    pool.record(
        id(0),
        RateLimitStatus {
            remaining: 0,
            reset_at: now_unix() - 1,
        },
    );

    assert!(pool.credential().await.is_ok());
    assert_eq!(pool.available(), 1);
}

#[tokio::test]
async fn spending_estimates_quota_between_authoritative_answers() {
    let pool = pool(1);

    // GitHub reports the truth...
    pool.record(
        id(0),
        RateLimitStatus {
            remaining: 2,
            reset_at: now_unix() + 3_600,
        },
    );
    // ...and we estimate downward until it does so again.
    pool.record_spend(id(0), 1);
    pool.record_spend(id(0), 1);
    assert_eq!(pool.available(), 0);

    // Saturating, not wrapping: over-spending must not wrap to a huge quota.
    pool.record_spend(id(0), 999);
    assert_eq!(pool.available(), 0);
}

#[tokio::test]
async fn reports_for_an_unknown_credential_are_ignored() {
    let pool = pool(1);

    // A stale id from another provider must not panic or corrupt state.
    pool.record(
        CredentialId {
            kind: CredentialKind::Installation,
            index: 0,
        },
        RateLimitStatus {
            remaining: 0,
            reset_at: now_unix() + 3_600,
        },
    );
    pool.record(
        id(99),
        RateLimitStatus {
            remaining: 0,
            reset_at: now_unix() + 3_600,
        },
    );

    assert_eq!(pool.available(), 1);
    assert!(pool.credential().await.is_ok());
}

// --- properties -----------------------------------------------------------

proptest! {
    /// The invariant that actually matters: a credential handed out must have
    /// quota. Everything else is an optimisation.
    #[test]
    fn never_issues_an_exhausted_credential(
        size in 1usize..8,
        exhausted in prop::collection::vec(any::<bool>(), 1..8),
    ) {
        let pool = pool(size);
        let reset = now_unix() + 3_600;

        let dead: Vec<usize> = exhausted.iter().take(size).enumerate()
            .filter_map(|(i, &out)| out.then_some(i)).collect();

        for &index in &dead {
            pool.record(id(index), RateLimitStatus { remaining: 0, reset_at: reset });
        }

        let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
        runtime.block_on(async {
            for _ in 0..(size * 3) {
                match pool.credential().await {
                    Ok(credential) => prop_assert!(
                        !dead.contains(&credential.id.index),
                        "issued exhausted credential {}", credential.id.index
                    ),
                    // Only legitimate when every token is spent.
                    Err(_) => prop_assert_eq!(dead.len(), size),
                }
            }
            Ok(())
        }).unwrap();
    }

    /// Over many leases, no token should carry a disproportionate share — the
    /// point of the pool is spreading quota.
    #[test]
    fn distributes_evenly_across_healthy_tokens(size in 2usize..6, rounds in 4usize..20) {
        let pool = pool(size);
        let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();

        let mut counts = vec![0usize; size];
        runtime.block_on(async {
            for _ in 0..(size * rounds) {
                counts[pool.credential().await.unwrap().id.index] += 1;
            }
        });

        let (min, max) = (counts.iter().min().unwrap(), counts.iter().max().unwrap());
        prop_assert!(max - min <= 1, "uneven distribution: {counts:?}");
    }

    /// `available()` is a count of the pool, so it can never exceed its size or
    /// go negative — and it must agree with whether a lease can succeed.
    #[test]
    fn availability_is_consistent_with_leasing(size in 1usize..6, spend in 0u32..6_000) {
        let pool = pool(size);
        for index in 0..size {
            pool.record(id(index), RateLimitStatus { remaining: spend, reset_at: now_unix() + 3_600 });
        }

        let available = pool.available();
        prop_assert!(available <= size);

        let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let leased = runtime.block_on(pool.credential()).is_ok();
        prop_assert_eq!(leased, available > 0);
    }

    /// Arbitrary reports from anywhere must never panic or poison the pool.
    #[test]
    fn arbitrary_reports_keep_the_pool_usable(
        reports in prop::collection::vec((0usize..10, 0u32..10_000, -1000i64..1000), 0..30),
    ) {
        let pool = pool(3);
        let now = now_unix();

        for (index, remaining, offset) in reports {
            pool.record(id(index), RateLimitStatus { remaining, reset_at: now + offset });
            pool.record_spend(id(index), remaining);
        }

        prop_assert!(pool.available() <= 3);
    }
}
