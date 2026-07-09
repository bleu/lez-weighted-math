# ADR 0004: Fixed-point scale is 2^52

Status: **ACCEPTED** (post-kernel sweep). Resolves ADR 0001 decision 2.

## Context

The brief's starting point was "~2^52, sweep against mpmath to confirm;
don't hand-pick." The harness was built parametric over `SCALE` (ADR 0002)
so the sweep is a one-line edit plus a recompile. With the kernel
implemented, the sweep ran over the candidates the oracle emitted floors
for: 44, 48, 52, 56, 60.

## Sweep results

Measured with `cargo test -p harness --test differential -- --ignored
--nocapture` (worst case over the 87-case pow fixture and the 29-case
swap fixture, errors in ulps at the candidate scale):

| SCALE | raw pow error beyond grid | pow_up/down err (bound 4) | full suite |
|------:|--------------------------:|--------------------------:|------------|
| 44    | 0                         | 2 / 2                     | green      |
| 48    | 0                         | 2 / 2                     | green      |
| 52    | 0                         | 2 / 2                     | green      |
| 56    | 0                         | 2 / 2                     | economic gate fails |
| 60    | 8–18                      | 10 / 20                   | pow gate + arith envelope fail |

Two independent ceilings appear above 52:

- The internal pipeline runs at `LN_SCALE = 62` with a pool-favouring pad
  of `2^-53` on `1 - power`. At `SCALE = 56` that pad alone exceeds the
  economic gate's `4·2^-56`-per-reserve allowance; the public scale would
  be claiming precision the internal scale cannot back.
- At `SCALE = 60` the two remaining guard bits cannot absorb series and
  rounding error (raw pow drifts up to 20 ulps), and the `Fixed` division
  envelope `value < 2^(127-2·SCALE)` collapses to 128.

Below 52 everything passes but precision is given away for nothing: cycle
cost is unchanged (same series, same single division) since the pipeline
width is `LN_SCALE`, not `SCALE`.

## Decision

`SCALE = 52` — the finest public scale the 62-bit internal pipeline can
honestly back, with 2 ulps of measured slack against the 4-ulp bound.
`BUDGET_ULPS = 3` in the harness (bound `= 1 + 3`); measured directional
error is exactly the deliberate ±2-ulp pad of `pow_up`/`pow_down`.

## Consequences

- The one-ulp quantization floor is `2^-52 ≈ 2.2e-16`, about 45x finer
  than Balancer's 1e-18-grid pow with its 1e-14 relative error bound.
- Re-running the sweep after any kernel change is
  `sed` + `cargo test`; fixtures never regenerate (ADR 0002 held).
- A future move to `SCALE = 56` would require widening the internal
  pipeline (LN_SCALE > 62 needs i256 series arithmetic) — a cost/precision
  trade the current LBP use case does not justify.
