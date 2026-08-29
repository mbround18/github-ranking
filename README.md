# GitHub Ranked

Competitive skill ratings for developers, from GitHub contributions. A Rust
service that renders badge SVGs, with a React dashboard that shares the
service's ranking and rendering code via WebAssembly.

A rewrite of [Shemarhn/Github_Ranked](https://github.com/Shemarhn/Github_Ranked)
(Next.js + Satori + Upstash Redis), with no Redis, no Satori, and no Vercel.

## Layout

| Path | What |
| --- | --- |
| `crates/core` | Ranking maths and card rendering. Pure — no I/O, no clock. Compiles native *and* to wasm. |
| `crates/auth-core` | The credential trait. No providers. |
| `crates/auth-pat` | Personal access token provider, an *optional* dependency of the server. |
| `crates/server` | axum service: GitHub client, cache, badge and JSON endpoints. |
| `crates/wasm` | wasm-bindgen wrapper so the browser runs the same engine. |
| `web` | Vite + React + Tailwind + shadcn/ui dashboard. |
| `e2e` | Playwright, against the real binary with a seeded cache. |
| `deploy/k8s` | Kustomize manifests. |
| `fixtures` | Golden ranks generated from the original TypeScript engine. |
| `upstream` | Read-only reference clone of the original. |

## Running it

```sh
make dev
```

That builds the wasm bundle, installs frontend dependencies if needed, and runs
the API on **10090** alongside the Vite dev server on **10173** — open the
latter. Both stop together on Ctrl-C.

Credentials are resolved from `gh auth token` first, falling back to
`GITHUB_TOKEN` in the environment. `make token` reports which one it found
without printing it.

`make` on its own lists every target. The ones worth knowing:

| | |
| --- | --- |
| `make dev` | API + frontend dev server, hot reloading |
| `make serve` | build everything, run the single binary as production does |
| `make test` | Rust tests and Playwright |
| `make check` | fmt, clippy and TypeScript |
| `make bench` | criterion benchmarks |
| `make docker-selfhost` | image that accepts a PAT in production |
| `make preview` | render a tier × theme contact sheet to look at |

> Ports are in the 10k range deliberately, and **10090 rather than 10080**:
> browsers refuse to connect to 10080 (`ERR_UNSAFE_PORT`), which is the only
> blocked port in that range.

## CI

Built on [paws](https://github.com/mbround18/paws), which drives the
single-ecosystem work — Rust build/test/coverage, the Vite frontend, and the
container image. Reproduce a CI failure locally with `make ci`.

The end-to-end job runs natively rather than through paws, because it needs
artefacts from both ecosystems in one place plus a running binary, which is not
what a per-toolchain pipeline is for.

Two jobs guard properties rather than correctness: the feature matrix asserts a
stock build does not link the PAT provider at all, and the manifest job asserts
the Secret template never gains a real credential.

## Self-hosting

Production auth is a compile-time choice. The PAT provider is a separate crate
and an optional dependency, so a stock production build does not merely disable
it — the provider is not linked into the binary at all. A personal access token
requires opting in:

```sh
docker build --build-arg CARGO_FEATURES=pat-in-production -t github-ranked:selfhost .
```

That is the right build for a personal instance — the quota is yours and there is
no OAuth surface. See [`docs/deployment.md`](docs/deployment.md).

## Testing

```sh
make test          # 139 Rust tests plus 37 Playwright
make test-features # every cargo feature combination
make bench
```

Three layers, deliberately:

- **Golden fixtures** pin known inputs against the original implementation.
- **Property tests** (proptest, ~4,600 generated cases) cover the rest of the
  input space — invariants like "more contributions never lower a rank" and
  "the renderer never panics or emits malformed SVG".
- **Playwright** drives the real binary and a real browser.

Benchmarks are baselined in [`docs/performance.md`](docs/performance.md).

## The constraint that shapes everything

Badge URLs are embedded in people's profile READMEs. **No published rank may
move.** So the ranking algorithm is not re-derived — it is pinned against the
original implementation by golden fixtures covering 3,016 stat combinations and
every integer Elo on the ladder. Every user-visible field is bit-exact.

See [`docs/fixtures.md`](docs/fixtures.md) for how that is enforced,
[`docs/scoring.md`](docs/scoring.md) for what the metrics actually measure
(two of the five are misnamed, which matters), and
[`docs/deployment.md`](docs/deployment.md) for running it on Kubernetes.

## Notable properties

- **No fonts are loaded, ever.** Card text is drawn as vector outlines from a
  glyph table embedded at build time, so a badge renders identically in any
  browser — including inside an `<img>`, where webfonts cannot load at all. The
  original fetched five fonts from Google on *every render*, cache hits included.
- **Two GitHub requests per profile, regardless of account age.** Every
  contribution year is fetched in one aliased GraphQL document rather than one
  request per year.
- **Request coalescing.** Concurrent requests for the same user trigger exactly
  one GitHub fetch, so a popular badge expiring doesn't stampede.
- **The dashboard runs the real engine.** Switching card themes costs no request
  and no quota, and the preview cannot drift from the served badge.

## License

MIT — see [`LICENSE`](LICENSE), which preserves the original project's copyright
notice, since the algorithm, tier emblems and themes derive from it.

The embedded glyph outlines derive from Noto Sans (SIL OFL 1.1); its licence
ships in [`licenses/`](licenses/).
