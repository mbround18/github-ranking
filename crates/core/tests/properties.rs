//! Property tests.
//!
//! The golden fixtures pin *known* inputs against the original implementation.
//! These cover the rest of the space: invariants that must hold for any input at
//! all, including the ones nobody thought to write a fixture for.

use github_ranked_core::ranking::constants::tier_range;
use github_ranked_core::ranking::{
    AggregatedStats, Division, RankResult, Tier, calculate_gp, calculate_percentile,
    calculate_rank, calculate_wpi, calculate_z_score, get_division, get_tier,
};
use github_ranked_core::render::card::{CardInput, render_card, thousands};
use github_ranked_core::render::text::{Anchor, TextStyle, Weight, measure, text_svg};
use github_ranked_core::validation::{Theme, is_valid_username};
use proptest::prelude::*;

/// Plausible contribution counts, spanning empty accounts to extreme outliers.
fn stats_strategy() -> impl Strategy<Value = AggregatedStats> {
    (
        0.0..500_000.0f64,
        0.0..500_000.0f64,
        0.0..500_000.0f64,
        0.0..2_000_000.0f64,
        0.0..5_000_000.0f64,
    )
        .prop_map(|(prs, reviews, issues, commits, stars)| AggregatedStats {
            total_merged_prs: prs,
            total_code_reviews: reviews,
            total_issues_closed: issues,
            total_commits: commits,
            total_stars: stars,
            ..Default::default()
        })
}

fn tier_strategy() -> impl Strategy<Value = Tier> {
    prop::sample::select(Tier::ALL_DESC.to_vec())
}

fn theme_strategy() -> impl Strategy<Value = Theme> {
    prop::sample::select(Theme::ALL.to_vec())
}

proptest! {
    // --- the ladder ------------------------------------------------------

    /// A tier's range must actually contain the Elo that selected it. This is
    /// the invariant that a mis-typed threshold would break.
    #[test]
    fn every_elo_lands_inside_its_own_tier(elo in 0.0..10_000.0f64) {
        let tier = get_tier(elo);
        let range = tier_range(tier);

        prop_assert!(elo >= range.min, "{elo} below {tier} minimum {}", range.min);
        prop_assert!(elo < range.max, "{elo} at or above {tier} maximum {}", range.max);
    }

    /// Divisions exist for exactly the tiers below Master, never otherwise.
    #[test]
    fn divisions_exist_for_exactly_the_divided_tiers(elo in 0.0..10_000.0f64) {
        let tier = get_tier(elo);
        let division = get_division(elo, tier);

        prop_assert_eq!(division.is_some(), tier.has_divisions(), "{}", tier);
    }

    /// GP is a percentage within a division, and undivided tiers have none.
    #[test]
    fn gp_stays_in_range(elo in 0.0..10_000.0f64) {
        let tier = get_tier(elo);
        let division = get_division(elo, tier);
        let gp = calculate_gp(elo, tier, division);

        prop_assert!((0.0..=99.0).contains(&gp), "gp {gp} out of range at elo {elo}");
        prop_assert_eq!(gp == 0.0 || tier.has_divisions(), true);
    }

    /// Climbing a tier means climbing through its divisions in order.
    #[test]
    fn divisions_ascend_with_elo(tier in tier_strategy(), a in 0.0..1.0f64, b in 0.0..1.0f64) {
        prop_assume!(tier.has_divisions());
        let range = tier_range(tier);
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };

        let span = range.max - range.min;
        let low_elo = range.min + lo * span * 0.999;
        let high_elo = range.min + hi * span * 0.999;

        let low = get_division(low_elo, tier).unwrap();
        let high = get_division(high_elo, tier).unwrap();

        prop_assert!(low.index() <= high.index(),
            "elo {low_elo} gave {low} but {high_elo} gave {high}");
    }

    // --- scoring ---------------------------------------------------------

    /// Contributing more can never rank you lower. If this ever fails, someone
    /// has introduced a negative weight.
    #[test]
    fn more_contributions_never_lower_the_rank(
        base in stats_strategy(),
        extra_prs in 0.0..10_000.0f64,
        extra_reviews in 0.0..10_000.0f64,
    ) {
        let before = calculate_rank(&base);

        let mut after_stats = base.clone();
        after_stats.total_merged_prs += extra_prs;
        after_stats.total_code_reviews += extra_reviews;
        let after = calculate_rank(&after_stats);

        prop_assert!(after.elo >= before.elo,
            "elo fell from {} to {}", before.elo, after.elo);
        prop_assert!(after.wpi >= before.wpi);
    }

    /// Ranking is a pure function; the same stats must always rank the same.
    #[test]
    fn ranking_is_deterministic(stats in stats_strategy()) {
        prop_assert_eq!(calculate_rank(&stats), calculate_rank(&stats));
    }

    /// Percentile is a percentage, whatever the input.
    #[test]
    fn percentile_is_a_percentage(z in -50.0..50.0f64) {
        let percentile = calculate_percentile(z);
        prop_assert!((0.0..=100.0).contains(&percentile), "got {percentile} for z={z}");
    }

    /// Higher z-scores are strictly better placed.
    #[test]
    fn percentile_rises_with_z(a in -6.0..6.0f64, b in -6.0..6.0f64) {
        prop_assume!(a < b);
        prop_assert!(calculate_percentile(a) <= calculate_percentile(b));
    }

    /// The star cap is an anti-farming rule: past it, stars stop counting.
    #[test]
    fn stars_stop_mattering_past_the_cap(stars in 1_000.0..10_000_000.0f64) {
        let at_cap = AggregatedStats { total_stars: 1_000.0, ..Default::default() };
        let beyond = AggregatedStats { total_stars: stars, ..Default::default() };

        prop_assert_eq!(calculate_wpi(&at_cap), calculate_wpi(&beyond));
    }

    /// WPI is floored at 1 so the log transform stays finite for empty accounts.
    #[test]
    fn wpi_is_always_positive(stats in stats_strategy()) {
        let wpi = calculate_wpi(&stats);
        prop_assert!(wpi >= 1.0);
        prop_assert!(wpi.is_finite());
    }

    /// Nothing in the pipeline may produce NaN, which would poison every
    /// comparison downstream.
    #[test]
    fn no_stage_produces_nan(stats in stats_strategy()) {
        let rank = calculate_rank(&stats);

        prop_assert!(rank.wpi.is_finite());
        prop_assert!(rank.z_score.is_finite());
        prop_assert!(rank.percentile.is_finite());
        prop_assert!(rank.elo >= 0);
        prop_assert_eq!(calculate_z_score(rank.wpi).is_finite(), true);
    }

    // --- text ------------------------------------------------------------

    /// Measurement is linear in size, which the layout maths assumes.
    #[test]
    fn text_measurement_scales_with_size(text in "[ -~]{0,40}", size in 1.0..100.0f64) {
        prop_assume!(!text.is_empty());
        let single = measure(&text, size, Weight::Regular, 0.0);
        let double = measure(&text, size * 2.0, Weight::Regular, 0.0);

        prop_assert!((double - single * 2.0).abs() < 1e-6);
        prop_assert!(single >= 0.0);
    }

    /// Any printable input must render without panicking, and produce balanced
    /// markup.
    #[test]
    fn text_rendering_is_always_well_formed(text in "[ -~]{0,60}", size in 4.0..60.0f64) {
        let svg = text_svg(&text, 0.0, 0.0, &TextStyle { size, weight: Weight::Bold, fill: "#fff", letter_spacing: 0.0, anchor: Anchor::Start });

        prop_assert_eq!(svg.matches("<g ").count(), svg.matches("</g>").count());
        if !text.is_empty() {
            prop_assert!(svg.starts_with("<g transform="));
            prop_assert!(svg.ends_with("</g>"));
        }
    }

    /// Thousands separators must not alter the value or lose digits.
    #[test]
    fn thousands_separators_preserve_the_number(value in 0.0..1e12f64) {
        let formatted = thousands(value);
        let stripped: String = formatted.chars().filter(|c| *c != ',').collect();

        prop_assert_eq!(stripped.parse::<u64>().unwrap(), value.round() as u64);

        // Groups of three, except the leading one.
        let groups: Vec<&str> = formatted.split(',').collect();
        for group in groups.iter().skip(1) {
            prop_assert_eq!(group.len(), 3, "bad grouping in {}", formatted);
        }
        prop_assert!((1..=3).contains(&groups[0].len()), "bad leading group in {}", formatted);
    }

    // --- the card --------------------------------------------------------

    /// The renderer must never panic and must never emit an unsubstituted
    /// placeholder, for any tier, theme or username we would accept.
    #[test]
    fn cards_render_for_any_valid_input(
        username in "[a-zA-Z0-9][a-zA-Z0-9-]{0,20}[a-zA-Z0-9]",
        tier in tier_strategy(),
        theme in theme_strategy(),
        stats in stats_strategy(),
        elo in 0i64..4_000,
        gp in 0i64..100,
    ) {
        prop_assume!(is_valid_username(&username));

        let rank = RankResult {
            tier,
            division: tier.has_divisions().then_some(Division::II),
            elo, gp, percentile: 50.0, wpi: 1000.0, z_score: 0.0,
        };

        let svg = render_card(&CardInput {
            username: &username, rank: &rank, stats: &stats,
            theme, season: None, current_year: 2026,
        });

        prop_assert!(svg.starts_with("<svg "), "malformed opening");
        prop_assert!(svg.ends_with("</svg>"));
        prop_assert!(!svg.contains("{NS}"), "leaked namespace placeholder");
        prop_assert_eq!(svg.matches("<g ").count(), svg.matches("</g>").count());
        prop_assert_eq!(svg.matches("<defs>").count(), svg.matches("</defs>").count());
    }

    /// Identical inputs must produce identical bytes — the browser preview and
    /// the served badge depend on it.
    #[test]
    fn card_rendering_is_deterministic(
        tier in tier_strategy(),
        theme in theme_strategy(),
        stats in stats_strategy(),
    ) {
        let rank = RankResult {
            tier, division: tier.has_divisions().then_some(Division::IV),
            elo: 1500, gp: 25, percentile: 60.0, wpi: 900.0, z_score: 0.2,
        };
        let input = CardInput {
            username: "octocat", rank: &rank, stats: &stats,
            theme, season: None, current_year: 2026,
        };

        prop_assert_eq!(render_card(&input), render_card(&input));
    }

    // --- validation ------------------------------------------------------

    /// Validation must be total: no input may panic it.
    #[test]
    fn username_validation_never_panics(input in ".*") {
        let _ = is_valid_username(&input);
    }

    /// Anything accepted must satisfy GitHub's stated rule.
    #[test]
    fn accepted_usernames_satisfy_the_rule(input in "[a-zA-Z0-9-]{0,45}") {
        prop_assume!(is_valid_username(&input));

        prop_assert!((1..=39).contains(&input.len()));
        prop_assert!(input.chars().next().unwrap().is_ascii_alphanumeric());
        prop_assert!(input.chars().last().unwrap().is_ascii_alphanumeric());
        prop_assert!(!input.contains("--"));
    }
}
