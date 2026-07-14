# ADR 0002: Balances are raw u128 at the API boundary

Status: **ACCEPTED** (initial design review). Surfaced while designing the
harness fixture schema.

## Context

The initial API sketch of `weighted.rs` took `balance_in`, `balance_out`, and
`amount_in` as `Fixed`. `Fixed` is `i128` scaled by `2^SCALE` (starting point
`2^52`). A realistic reserve of `1e27` wei, multiplied by `2^52`, is about
`4.5e42` — well past the `i128` maximum (~`1.7e38`). Balances therefore cannot
be `Fixed` values at the working scale; that signature was unsound.

The platform context reinforces the point: LEZ token amounts are native `u128`
with no decimals field (`CONTEXT.md`), and the LEE reference AMM does its swap
math in `u128` (the integer-width survey in ADR 0008).

## Decision

The kernel API takes **raw `u128`** balances and amounts. `Fixed` appears only
where the math genuinely needs a fractional value in a bounded range:

- `base = balance_in / (balance_in + amount_in)` — a ratio in (0,1).
- `exponent = weight_in / weight_out` — the weight ratio, `0.0101 → 99`.

The final `balance_out · (1 - power)` is exactly the one localised
`128 × 128 → 256` widening multiply the brief calls for: a raw `u128` balance
times a `Fixed` fraction in [0,1]. No general bignum, no promoting the whole
kernel to 256-bit.

Concretely, the `weighted.rs` signatures changed from `Fixed` to `u128` for
balance/amount parameters (the bodies were still `todo!()` at the time).
Fixtures store balances/amounts as integer wei strings, and `tokens_out` as
its floored integer wei value.

## Consequences

- The overflow proof reasons about the real `u128` values and the single
  widened multiply, not a squeezed `Fixed` — a smaller, more honest proof
  obligation.
- The harness passes raw integers; the kernel forms `base` and `exponent`
  internally.
- The `weighted.rs` signatures had to be corrected during implementation
  (this was tracked in `docs/archive/plans/0001-test-harness.md`).
- Matches the Logos platform convention (`u128` money math), which also helps
  the RFP "platform-pattern fluency" criterion.
