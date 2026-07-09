# ADR 0001: Open decisions

Status: **PROPOSED** — to resolve in grill (Phase 1).

These are the open decisions carried over from the design brief (`CONTEXT.md`).
Each stays **PROPOSED — to resolve in grill** until settled, at which point it
gets its own ADR (or an update here) recording the decision and rationale.

## 1. Target error bound

The pass/fail number the kernel is validated against (vs. the mpmath oracle).
Defines what "correct to a proven error bound" means in practice.

**PROPOSED — to resolve in grill.**

## 2. Fixed-point scale

Scale ~2^52 is the starting point, to be confirmed by sweeping candidate scales
against mpmath rather than hand-picked. Interacts with the error bound and with
`u128`/widened-intermediate overflow headroom.

**PROPOSED — to resolve in grill.**

## 3. Series choice + term count

Taylor vs minimax polynomial for the series on the `[0, 0.693]` range-reduced
remainder, and how many terms — the accuracy/cycle-cost trade-off.

**PROPOSED — to resolve in grill.**

## 4. `expm1` boundary threshold

Where to switch to the `−expm1(y·ln base)` path to avoid catastrophic
cancellation in `1 − power` near the sale start (exponent ≈ 0.0101, power ≈ 1).

**PROPOSED — to resolve in grill.**

## 5. Exact wrapper set + rounding-direction invariants

Which `WeightedMath` wrappers to port (`powUp`/`divUp`/`mulDown`/`complement`,
…) and the precise rounding-direction invariants that keep every result
favoring the pool.

**PROPOSED — to resolve in grill.**

## 6. Balancer reference + how outputs are pulled

Which Balancer `LogExpMath`/`WeightedMath` reference (version/commit) is the
secondary comparator, and the mechanism for pulling its outputs for differential
comparison.

**PROPOSED — to resolve in grill.**
