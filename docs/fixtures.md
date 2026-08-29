# Golden fixtures

The rewrite's hard constraint: **nobody's published rank may move.** Those badges
are embedded in profile READMEs; a silent re-rank would be the most damaging
possible regression.

So the ranking algorithm is not re-derived from the spec — it is pinned against
the original implementation. `tools/oracle/regenerate.sh` runs the upstream
TypeScript engine over a generated input set and dumps its output to
`fixtures/`, which `server/tests/golden.rs` then asserts against.

| Fixture | Contents | Pins |
| --- | --- | --- |
| `rank_cases.json` | 3,016 stat combinations (16 hand-picked edge cases + 3,000 log-uniform random), covering all 10 tiers | `calculate_rank` end to end |
| `elo_sweep.json` | every integer Elo, 0–3400 | tier, division and GP boundaries from both sides |
| `zscore_sweep.json` | z from -6.00 to +6.00, step 0.01 | `calculate_percentile`, `calculate_elo` |
| `wpi_sweep.json` | WPI across 14 orders of magnitude | `calculate_z_score` |
| `decay_sweep.json` | years 2010–2030 across seasons 2024–2027 | seasonal decay |

## Float fidelity

Everything a user can see — tier, division, Elo, GP, percentile, WPI — is
**bit-exact** against upstream across the whole fixture set.

The one exception is the internal `z_score`, which diverges by at most **5 ULP**
(~2e-15 relative). V8's `Math.log` and Rust's `f64::ln` are different libm
implementations and disagree in the last bit or two. This is invisible in
practice: `elo` is rounded to an integer and `percentile` to one decimal place,
and `rank_fields_that_users_see_are_bit_exact` confirms the difference never
lands near enough to a rounding boundary to change either.

`js_round` in `engine.rs` exists for the same reason — JS `Math.round` breaks
ties toward +infinity, Rust's `f64::round` breaks them away from zero.
