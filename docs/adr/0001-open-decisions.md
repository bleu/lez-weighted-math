# ADR 0001: Open decisions

Status: **PARTIALLY RESOLVED** — Phase 1 grill settled the harness-facing
decisions (1, 5, 6) and the balance representation (new, see ADR 0003). The
scale (2) has a settled *method* but no final value yet; the two kernel-internal
decisions (3, 4) remain open and belong to the TDD implementation phase.

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

## 2. Fixed-point scale — **METHOD RESOLVED, VALUE PENDING SWEEP**

Scale ~2^52 remains the starting point. The grill did not lock a final value —
that needs a working kernel to sweep — but it did settle *how* the sweep works:
the oracle emits a quantization floor per candidate scale (`scales.json`), and
the harness is parametric over `SCALE`, so re-sweeping is a one-line change with
no fixture regeneration (ADR 0002). The final value gets its own ADR once the
kernel exists and the sweep runs.

**Still open:** the concrete `SCALE` value.

## 3. Series choice + term count — **OPEN (kernel decision)**

Taylor vs minimax on the `[0, 0.693]` range-reduced remainder, and term count.
Deliberately left to the TDD implementation phase: the harness grades results,
not the method, so it stays agnostic. Resolve when building the kernel against
the oracle.

## 4. `expm1` boundary threshold — **OPEN (kernel decision)**

Where to switch to `-expm1(y · ln base)` to avoid catastrophic cancellation in
`1 - power` near the sale start. Also a kernel-internal decision for the TDD
phase. The harness only needs to *stress* this region, which it does via
danger-zone fixture weighting (ADR 0002), not by knowing the threshold.

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
