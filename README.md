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
# 1. The wasm bundle the frontend imports (regenerate whenever core changes)
./crates/wasm/build.sh

# 2. The frontend
cd web && npm install && npm run build && cd ..

# 3. The service
GITHUB_TOKEN=ghp_... WEB_ROOT=./web/dist cargo run --release -p github-ranked
```

Then <http://localhost:10090>.

For frontend work, `cd web && npm run dev` serves on **10173** and proxies
`/api` to the service on **10090**.

> Ports are in the 10k range deliberately, and **10090 rather than 10080**:
> browsers refuse to connect to 10080 (`ERR_UNSAFE_PORT`), which is the only
> blocked port in that range.

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
cargo test                       # 139 tests: golden ranks, properties, HTTP
cargo bench -p github-ranked-core -- --quick
cd e2e && npx playwright test    # 37 tests, no GitHub token or network needed
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
