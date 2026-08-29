//! Card rendering throughput.
//!
//! This is the badge endpoint's hot path on a cache miss, and it runs again in
//! the browser on every theme switch. It is also where dropping Satori was
//! supposed to pay off, so it is worth measuring rather than assuming.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use github_ranked_core::ranking::{calculate_rank, AggregatedStats, Division, RankResult, Tier};
use github_ranked_core::render::card::{render_card, CardInput};
use github_ranked_core::render::text::{measure, text_svg, Anchor, Weight};
use github_ranked_core::validation::Theme;
use std::hint::black_box;

fn sample() -> (RankResult, AggregatedStats) {
    let stats = AggregatedStats {
        total_merged_prs: 342.0,
        total_code_reviews: 1287.0,
        total_issues_closed: 96.0,
        total_commits: 4521.0,
        total_stars: 12480.0,
        ..Default::default()
    };
    (calculate_rank(&stats), stats)
}

fn bench_card(c: &mut Criterion) {
    let (rank, stats) = sample();
    let mut group = c.benchmark_group("render_card");

    let input = CardInput {
        username: "octocat", rank: &rank, stats: &stats,
        theme: Theme::Default, season: None, current_year: 2026,
    };

    // Report bytes/sec so the cost of the SVG's size is visible alongside time.
    group.throughput(Throughput::Bytes(render_card(&input).len() as u64));
    group.bench_function("default", |b| b.iter(|| render_card(black_box(&input))));

    // Themes differ in gradient work; confirm none is an outlier.
    for theme in [Theme::Minimal, Theme::Cyberpunk, Theme::Light] {
        let input = CardInput {
            username: "octocat", rank: &rank, stats: &stats,
            theme, season: None, current_year: 2026,
        };
        group.bench_with_input(
            BenchmarkId::from_parameter(theme.as_str()),
            &input,
            |b, input| b.iter(|| render_card(black_box(input))),
        );
    }

    group.finish();
}

fn bench_tiers(c: &mut Criterion) {
    let (_, stats) = sample();
    let mut group = c.benchmark_group("render_by_tier");

    // Emblem complexity varies by tier; Challenger has the most geometry.
    for tier in [Tier::Iron, Tier::Diamond, Tier::Challenger] {
        let rank = RankResult {
            tier,
            division: tier.has_divisions().then_some(Division::II),
            elo: 2274, gp: 74, percentile: 99.1, wpi: 48210.0, z_score: 2.85,
        };
        group.bench_function(BenchmarkId::from_parameter(tier.as_str()), |b| {
            b.iter(|| {
                render_card(black_box(&CardInput {
                    username: "octocat", rank: &rank, stats: &stats,
                    theme: Theme::Default, season: None, current_year: 2026,
                }))
            })
        });
    }

    group.finish();
}

fn bench_text(c: &mut Criterion) {
    let mut group = c.benchmark_group("text");

    group.bench_function("measure", |b| {
        b.iter(|| measure(black_box("GRANDMASTER"), 28.0, Weight::Bold, -0.02))
    });
    group.bench_function("outline", |b| {
        b.iter(|| {
            text_svg(black_box("GRANDMASTER"), 0.0, 0.0, 28.0, Weight::Bold, "#fff", -0.02, Anchor::Start)
        })
    });

    group.finish();
}

criterion_group!(benches, bench_card, bench_tiers, bench_text);
criterion_main!(benches);
