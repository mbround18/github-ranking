# Scoring: what v1 measures, and what it should

## The constraint

Badge URLs are embedded in people's profile READMEs. Any change to scoring
silently re-ranks every existing user. So v1 is reproduced exactly and pinned by
golden fixtures (`docs/fixtures.md`); everything below is a **v2 proposal**, not
a change already made.

## Finding: two of the five metrics do not measure what they claim

This came out of porting the aggregator, not from reading the docs.

| README says | Weight | Field actually queried | What that counts |
| --- | --- | --- | --- |
| Merged PRs — "code accepted by peers" | 35% | `totalPullRequestContributions` | PRs **opened** |
| Code Reviews — "mentorship, seniority" | 35% | `totalPullRequestReviewContributions` | reviews submitted ✓ |
| Issues Closed — "problem-solving" | 15% | `totalIssueContributions` | issues **opened** |
| Commits | 10% | `totalCommitContributions` | commits ✓ |
| Stars | 5% | summed `stargazers` | stars ✓ |

The internal field names (`totalMergedPRs`, `totalIssuesClosed`) say merged and
closed. The GraphQL fields behind them say created. Nothing filters by state
anywhere in the pipeline.

**Consequence: 50% of the score requires no peer validation at all.** Opening a
PR is unilateral — it does not need to be reviewed, approved, or merged. Opening
an issue is unilateral. The stated justification for weighting collaboration at
70% ("impossible to reach Diamond+ without peer interaction") does not hold: 35
of those 70 points are self-service.

## Issue #5

[Issue #5](https://github.com/Shemarhn/Github_Ranked/issues/5) raises three
criticisms. Assessed on merit:

### 1. Weights are arbitrary — *valid*

35/35/15/10/5 has no stated empirical basis, and neither do `MEAN_LOG_SCORE =
6.5` and `STD_DEV = 1.5`, which are what actually convert a score into "Top
0.1%". The tier percentiles in the README are **asserted, not measured** — they
follow from assuming a log-normal fit that was never validated against real
GitHub data.

Cheap honest fix: document them as design choices rather than findings.

Better fix, and a good use of the wasm build: ship alternate weight profiles
("Architect", "Maintainer") that recompute **client-side in the browser**. Users
explore trade-offs live, the canonical badge stays fixed and reproducible, and
it costs the server nothing.

### 2. Seasonal decay punishes foundational work — *valid, but the proposed fix is not implementable*

The concern is real: decay pushes people to keep pushing trivial commits to hold
a rank.

The proposal — exempt "core architectural contributions and foundational
documentation" from decay — cannot be built. Classifying a contribution as
architectural needs per-commit, per-file analysis across every repo a user has
touched. That is orders of magnitude more API traffic than the entire current
pipeline, and the classification would be unreliable anyway.

What *is* cheap: surface the undecayed all-time score alongside the decayed one,
so the decay is visible rather than hidden. The dashboard already fetches the raw
yearly breakdown needed for this.

### 3. Sybil / mutual farming — *valid, and worse than the issue states*

The issue describes two accounts approving each other's trivial PRs to reach
Challenger. Given the finding above, **collusion isn't even required**: PRs and
issues score on creation, so a single account can farm 50% of the weight alone,
against its own repositories.

The existing design does anticipate farming in two places — stars cap at 1,000,
and commits are weighted down to 10% — so the concern is in scope; it just
misses the largest hole.

Detecting reciprocity properly needs the interaction graph (who reviewed whose
PRs), which the aggregate counters don't carry and which would be expensive to
fetch. But most of the benefit is available cheaply, because
`contributionsCollection` exposes per-repository breakdowns
(`pullRequestContributionsByRepository`, `pullRequestReviewContributionsByRepository`)
**in the same request we already make**:

- contribution concentration across repositories
- share of contributions to repos the user owns
- how few distinct counterparties the reviews involve

A farming pattern is highly concentrated across few repos and owners; genuine
open-source work is not. That is a diversity multiplier, not true Sybil
detection, but it closes the cheap exploit without extra API cost.

## Proposed v2

1. Score merged PRs and closed issues for real. GitHub's search API gives exact
   per-year figures (`is:pr is:merged`, `is:issue is:closed`), one aliased node
   per year — **no additional requests**. `queries::verified_counts_fragment`
   already builds these.
2. Discount contributions to repositories the user owns.
3. Apply a contribution-diversity multiplier from the per-repository breakdown.
4. Publish the weight rationale; offer alternate profiles client-side via wasm.

**Migration:** v2 must not silently replace v1. Version the score, carry the
version in the cache key, and let v1 badges keep rendering as v1 until a
deliberate cutover. Re-ranking everyone without warning is the one thing this
rewrite is explicitly designed to avoid.
