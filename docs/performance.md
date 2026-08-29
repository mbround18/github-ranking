# Performance

Baselines on x86_64, `cargo bench -p github-ranked-core`. Recorded so a
regression is obvious rather than something you notice in production.

## Ranking engine

| Operation | Time |
| --- | --- |
| `calculate_rank` (end to end) | **~26 ns** |
| ├ `calculate_wpi` | 0.87 ns |
| ├ `calculate_z_score` | 4.4 ns |
| ├ `calculate_elo` | 1.7 ns |
| ├ `get_tier` | 1.7 ns |
| ├ `get_division` | 1.5 ns |
| └ `calculate_percentile` | **18 ns** |

The percentile is 70% of the cost — it is the only transcendental in the path
(`exp`, inside the erf approximation). Worth knowing before anyone "improves"
the accuracy there: a more precise erf would dominate the entire engine.

Cost is flat across magnitudes, so a Challenger account ranks as fast as an
empty one.

## Card rendering

| Operation | Time |
| --- | --- |
| `render_card` (default theme) | **~26 µs** |
| `render_card` (Challenger) | 30 µs |
| `render_card` (light) | 28 µs |
| `text_svg` (one 11-character run) | 1.9 µs |
| `measure` (one run) | 12 ns |

About **38,000 cards per second per core**, or ~1.1 GiB/s of SVG. Tier choice
moves it by ~20% — Challenger has the most emblem geometry — and no theme is an
outlier.

## What this means in practice

Rendering is free relative to everything around it. A cold badge is dominated
entirely by GitHub:

| Stage | Time |
| --- | --- |
| GitHub fetch (12-year account, 2 requests) | ~4.3 s |
| Cache hit, end to end over HTTP | ~6 ms |
| Card render | 0.026 ms |

So the levers that matter are the cache and the request count, not the renderer.
That is why the work went into alias-batched GraphQL (2 requests per profile
regardless of account age, 1 quota point each) and request coalescing, rather
than into micro-optimising the SVG.

It also means client-side rendering is unambiguously worth it: a theme switch in
the browser costs ~26 µs of wasm instead of a network round trip.

## Running them

```sh
cargo bench -p github-ranked-core              # full run, HTML reports in target/criterion
cargo bench -p github-ranked-core -- --quick   # rough numbers, much faster
```
