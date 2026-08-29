//! Ranking engine throughput.
//!
//! The engine runs on every cache miss and on every client-side recompute in
//! the browser, so it wants to be cheap. It is also the piece most likely to
//! grow accidentally expensive — the percentile calculation involves `exp`, and
//! a "small" change there is easy to miss.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use github_ranked_core::ranking::{
    calculate_elo, calculate_percentile, calculate_rank, calculate_wpi, calculate_z_score,
    get_division, get_tier, AggregatedStats,
};
use std::hint::black_box;

fn stats(scale: f64) -> AggregatedStats {
    AggregatedStats {
        total_merged_prs: 342.0 * scale,
        total_code_reviews: 1287.0 * scale,
        total_issues_closed: 96.0 * scale,
        total_commits: 4521.0 * scale,
        total_stars: 12480.0 * scale,
        ..Default::default()
    }
}

fn bench_rank(c: &mut Criterion) {
    let mut group = c.benchmark_group("calculate_rank");

    // Across magnitudes, since the log transform's cost could plausibly vary.
    for scale in [0.001, 1.0, 1000.0] {
        let input = stats(scale);
        group.bench_with_input(BenchmarkId::from_parameter(scale), &input, |b, input| {
            b.iter(|| calculate_rank(black_box(input)));
        });
    }

    group.finish();
}

fn bench_stages(c: &mut Criterion) {
    let mut group = c.benchmark_group("stages");
    let input = stats(1.0);

    group.bench_function("wpi", |b| b.iter(|| calculate_wpi(black_box(&input))));
    group.bench_function("z_score", |b| b.iter(|| calculate_z_score(black_box(48_210.0))));
    group.bench_function("elo", |b| b.iter(|| calculate_elo(black_box(2.85))));
    // The erf approximation — the only transcendental in the hot path.
    group.bench_function("percentile", |b| b.iter(|| calculate_percentile(black_box(2.85))));
    group.bench_function("tier", |b| b.iter(|| get_tier(black_box(2274.0))));
    group.bench_function("division", |b| {
        let tier = get_tier(2274.0);
        b.iter(|| get_division(black_box(2274.0), black_box(tier)))
    });

    group.finish();
}

criterion_group!(benches, bench_rank, bench_stages);
criterion_main!(benches);
