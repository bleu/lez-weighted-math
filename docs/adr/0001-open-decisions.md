# ADR 0001: Open decisions

Status: **FULLY RESOLVED** — Phase 1 grill settled the harness-facing
decisions (1, 5, 6) and the balance representation (new, see ADR 0003); the
kernel TDD phase settled the scale value (2, see ADR 0004), the series
choice (3, see ADR 0005), and the expm1 boundary (4, see ADR 0006).

These are the decisions carried over from the design brief (`CONTEXT.md`). Each
is tracked here as the index; the resolved ones point to the ADR that records
the rationale.

## 1. Target error bound — **RESOLVED** (see ADR 0002)

The pass/fail number is not a hand-picked constant. It is parametric:
`BOUND = quant_floor(SCALE) + BUDGET`, where the oracle emits the unavoidable
representation floor per candidate scale and `BUDGET` is the algorithmic
allowance. There are two hard gates (`tokens_out` in wei; `pow` at the ulp
level) plus a diagnostic on `1 - power`. The band is signed and one-sided:
`[0, BOUND]`. Full rationale in ADR 0002.

## 2. Fixed-point scale — **RESOLVED** (see ADR 0004)

`SCALE = 52`, confirmed by the post-kernel sweep over {44, 48, 52, 56, 60}:
the finest public scale the internal 62-bit pipeline can honestly back. The
sweep mechanism worked as designed in ADR 0002 — a one-line change, no
fixture regeneration. Sweep table and the two ceilings above 52 in ADR 0004.

## 3. Series choice + term count — **RESOLVED** (see ADR 0005)

atanh series for `ln` (13 terms), alternating Taylor for `exp` (20 terms),
both at internal scale 2^62, Horner with precomputed reciprocal constants.
One hardware division per `pow`. Taylor over minimax because truncation is
already far below rounding noise and the constants stay auditable.

## 4. `expm1` boundary threshold — **RESOLVED** (see ADR 0006)

There is no threshold: `1 - power` is kept at the internal 62-bit scale from
the exp series into the final widened payout multiply, unconditionally. The
cancellation risk was a floating-point framing; in fixed point the
subtraction is exact and only premature rounding to `SCALE` loses payouts —
so the kernel never does it.

## 5. Wrapper set + rounding-direction invariants — **RESOLVED** (see ADR 0002)

Full set: `pow_up`/`pow_down`, `mul_up`/`mul_down`, `div_up`/`div_down`, plus
`complement`. Each has a signed directional invariant (`_up ≥ true`,
`_down ≤ true`). A wrong-side result is a failure regardless of magnitude.
`calc_out_given_in` rounds down (floored payout). Rationale in ADR 0002.

## 6. Balancer reference + how outputs are pulled — **RESOLVED** (see ADR 0002)

Canonical Solidity `LogExpMath` at a pinned commit is the secondary comparator.
Outputs are captured offline via a committed `forge` script and frozen into a
fixture; CI needs neither Foundry nor Python. The Balancer source stays external
(GPL-3.0, incompatible with this repo's MIT/Apache license) — only the captured
numbers and our script are committed. Rationale in ADR 0002.

## New: balance representation — **RESOLVED** (see ADR 0003)

Surfaced during the grill: pool balances and amounts are raw `u128` wei, not
`Fixed`. `Fixed` appears only for the internal ratio and exponent. See ADR 0003.
