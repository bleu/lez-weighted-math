# ADR 0005: There is no expm1 boundary — the guard bits carry 1-power everywhere

Status: **ACCEPTED** (kernel implementation). Settles the expm1-boundary
question from the design brief.

## Context

The brief flagged sale-start catastrophic cancellation: exponent ≈ 0.0101,
base ≈ 1, so `power ≈ 1` and `1 - power` loses precision. The open decision
was where to switch `calc_out_given_in` onto a `-expm1(y·ln base)` path.

## Decision

No threshold exists, because the danger was diagnosed away rather than
special-cased. Cancellation of this kind is a floating-point phenomenon:
subtracting two nearby floats discards significand bits. Fixed point has
*absolute* precision — `2^62 - exp_result` is an exact integer subtraction.
What actually loses sale-start payouts is materializing `1 - power` on the
coarse public grid, where a true value like `2^-46` keeps only a few
significant bits at `2^-52` resolution.

So the kernel keeps `1 - power` at the internal 62-bit scale from the exp
series all the way into the final widened payout multiply
(`one_minus_pow_62` in `pow.rs`): it is computed once, exactly, from the
series result, padded down by `2^-53` for pool safety, and never rounded
to `SCALE` at all. This *is* the expm1 path — `1 - exp(-s)·2^-k` computed
without intermediate rounding — applied unconditionally, with no boundary
to choose, test, or audit.

The public `expm1` function exists for API completeness and is graded as a
diagnostic by the harness; the swap path does not call it.

## Consequences

- Sale-start fixtures (base up to `1 - 2^-40`, exponent 0.0101) pass the
  economic gate with the payout error dominated by base-formation
  quantization, not by `1 - power` handling.
- One code path for all regions: no boundary constant, no branch to get
  wrong on either side, one fewer thing for the audit.
- The `2^-53` pad on `1 - power` is what caps the honest public scale at
  52 (see ADR 0003).
