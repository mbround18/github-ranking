# Deployment

## Cache state is per pod

There is no Redis, so there is no shared cache. Each pod has its own in-memory
layer plus its own SQLite file. That is a deliberate trade, and it shapes how
this should be run.

**Intended shape: one replica with a PVC**, `Recreate` strategy (SQLite is
single-writer, and `ReadWriteOnce` will not attach to two pods anyway).

For a badge service this is fine. Ranks have a 24-hour TTL and GitHub's camo
proxy absorbs most repeat traffic, so the origin sees far less load than the
badge's view count suggests.

**Scaling out still works correctly**, it just costs more GitHub quota: with N
replicas a cold key can be fetched up to N times instead of once, because
request coalescing is per process. With a token pool that is affordable. What
you must not do is run multiple replicas against one `ReadWriteMany` volume —
SQLite over shared network storage corrupts.

If the cache ever needs to be shared, the seam is `cache::Cache`: it is the only
thing that touches storage.

## Operational endpoints

| Path | Probe | Meaning | On failure |
| --- | --- | --- | --- |
| `/healthz` | liveness | process is up; depends on nothing external | restart the pod |
| `/readyz` | readiness | a credential with quota is available | remove from load balancer |
| `/startupz` | startup | initialization finished; reports version and uptime | keep waiting, then restart |
| `/metrics` | — | Prometheus exposition | — |

Wire them up that way round. Running out of GitHub quota must **not** restart the
pod: a restart cannot restore quota and throws away a warm cache. It should only
stop new traffic arriving until quota returns.

The startup probe is separate from liveness so a slow first boot can be given a
generous `failureThreshold` without also loosening the liveness deadline for the
rest of the pod's life.

### Metrics

Counters for requests, responses by status class, cache hits and misses, GitHub
requests, errors and rate limits, and cards rendered; gauges for available
credentials and cache size; a latency histogram. Labels are deliberately
low-cardinality — the matched route, never the raw path, so `/api/rank/{username}`
is one series rather than one per user. Scrapes do not count themselves.

The two worth alerting on:

- `github_ranked_credentials_available == 0` — every credential is out of quota,
  so cold renders are failing even though cached ones still serve.
- `rate(github_ranked_cache_misses_total[5m])` climbing without a matching rise
  in requests — usually a cache volume that failed to mount, which is silent
  otherwise because the service degrades to memory-only rather than refusing to
  start.

## Testing a deployment

`e2e/` runs Playwright against the real binary with a pre-seeded cache, so it
needs neither a GitHub token nor network access. It covers the HTTP contract,
every probe path, the Prometheus output, and — in a real browser — that a badge
renders inside an `<img>` with no fonts available, which is the condition it
actually faces in a README.

```sh
cargo build --release -p github-ranked
cd e2e && npm install && npx playwright test
```

## Signals

SIGTERM triggers a graceful drain, so set `terminationGracePeriodSeconds`
comfortably above `REQUEST_TIMEOUT_SECS` (default 20) or Kubernetes will SIGKILL
mid-render during a rollout.

## Credentials

Production authentication is a **compile-time** choice, not a configuration one.

| Build | Production auth | Use for |
| --- | --- | --- |
| stock (`cargo build`) | GitHub App only | public, multi-tenant |
| `--features pat-in-production` | personal access token permitted | single-tenant self-hosting |

A stock binary is physically incapable of serving production traffic on a PAT —
no environment variable, ConfigMap or Secret can enable it, because the check is
a compile-time constant. That is deliberate: an env-var escape hatch is exactly
the kind of thing that gets set during an incident and never unset.

For a self-hosted instance a PAT is usually the right call: the quota being spent
is your own, and there is no OAuth flow or user-token storage to secure. Build it
explicitly:

```sh
docker build --build-arg CARGO_FEATURES=pat-in-production -t github-ranked:selfhost .
```

Such a build logs a warning at startup, and `/startupz` reports
`build.patInProduction` so you can tell which binary is deployed without
guessing.

The trade-off to be aware of: one token's 5,000 points per hour is shared by
every visitor. That is ample for a personal instance and will not scale to a
public one.

## Configuration

| Variable | Default | Notes |
| --- | --- | --- |
| `APP_ENV` | `development` | `production` disables PAT auth entirely |
| `HOST` / `PORT` | `0.0.0.0` / `10090` | **not 10080** — browsers block it as an unsafe port |
| `CACHE_PATH` | `./data/cache.db` | empty or `none` for memory-only |
| `CACHE_MAX_ENTRIES` | `10000` | in-memory ceiling |
| `WEB_ROOT` | `./web/dist` | built frontend |
| `REQUEST_TIMEOUT_SECS` | `20` | per request |
| `MAX_CONCURRENT_REQUESTS` | `256` | load shedding |
| `GITHUB_TOKEN`, `GITHUB_TOKEN_1..N` | — | dev, or a `pat-in-production` build |

A malformed value fails startup rather than silently defaulting — a typo'd
`PORT` should not quietly serve on the wrong one.

## Rollouts and cached data

A release that changes the cached payload's shape does not break: entries that
no longer decode are treated as misses and recomputed. A rollout costs a
refetch, not an outage.
