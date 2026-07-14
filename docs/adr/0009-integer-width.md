# ADR 0009: i128/u128 arithmetic with one localized widening, not i256

Status: **ACCEPTED**. Decided before the kernel was written (recorded at the
time in `docs/archive/handoffs/2026-07-09-i128-vs-i256-decision.md`); since
confirmed by the accuracy sweep (ADR 0004) and the measured cycle table
(`docs/zkvm-cycles.md`).

## Context

Should the kernel compute in `i128`/`u128`, widening only where a specific
step needs it, or promote everything to 256-bit integers the way EVM AMMs do?
The EVM precedent is strong — every major AMM does its math in 256 bits — so
choosing narrower needed a positive argument, not just an assumption.

## Decision

`i128`/`u128` throughout, with exactly one localized `128 × 128 → 256`
widening at the outer payout/payment multiply (`wide::mul_shr`). No 256-bit
type anywhere else, and no bignum dependency in the kernel.

The fallback, had the accuracy sweep failed: widen selectively at the step
that needs it, never the whole kernel. The sweep passed at `SCALE = 52`
(ADR 0004), so the fallback was never used.

## Rationale

Three independent reasons point the same way.

**It is the Logos platform default.** Surveyed across the `logos-co`
organization and the LEE v0.3 spec:

- Token amounts, supply, and mint/burn in the Token Program spec are `u128`
  (`logos-co/logos-lips`,
  `docs/blockchain/raw/lez/lee-v0.3-specifications/appendices/builtin-programs.md`).
- The same spec's reference AMM (the Liquidity Pool Program) keeps its
  reserves in `u128` and does plain multiply-then-divide `u128` swap math.
- Existing LEZ programs handle overflow with checked/saturating `u128`
  arithmetic, not widening (`logos-co/lez-payment-streams`,
  `lez-payment-streams-core/src/vault.rs`).
- The SPEL codec's widest primitive is `i128`/`u128`; no 256-bit type exists
  (`logos-co/spel`, `spel-framework-core/src/decode.rs`).
- `U256` appears only at the Ethereum boundary (`eth-lez-atomic-swaps`,
  the EVM wallet backend) — inside LEZ guests it is a foreign type.

**It is cheaper in the RISC0 cost model.** The guest word is 32 bits, so
every width doubling multiplies the cost of the division-heavy pow pipeline.
The EVM intuition inverts here: on the EVM, 256-bit is the native word and
narrow costs extra masking gas; on RV32IM, wide is emulated and expensive.
Measured numbers are in `docs/zkvm-cycles.md` — one `pow` is ~8k cycles at
i128, with the single `u128` division alone costing ~2.5k.

**It is the smaller proof surface.** The overflow proof
(`docs/overflow-proof.md`) reasons about real `u128` values and one widened
multiply. Promoting the kernel to 256-bit would not remove the proof
obligation; it would move every bound up 128 bits and add an emulated
integer type to the audit.

## Consequences

- `Fixed` is `i128` at `SCALE = 52`; the internal pipeline runs at 62
  fractional bits, comfortably inside `i128` (ADR 0004 records the two
  ceilings that cap the public scale).
- The one widening lives in `wide.rs` behind a fit assertion; a result past
  `u128` halts rather than wraps (see the overflow-envelope tests).
- A future precision demand beyond `SCALE = 56` would force 256-bit series
  arithmetic (ADR 0004) — that is the point at which this decision would be
  revisited.
