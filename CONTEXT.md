# Design overview

**Project:** a fixed-point weighted-pool `pow` kernel for a Balancer-style
LBP on the Logos Execution Zone (LEZ). It does triple duty: open-source
track-record artifact for RFP-016, the M1 on-chain guest logic, and an audit
surface. Licensed MIT.

## Platform constraints (verified against `lez-programs` main)

- LEZ programs are RISC Zero (RISC0) zkVM guests. The native word is 32 bits,
  so `u128` is multi-limb and division is the most expensive primitive — the
  kernel is shaped around minimizing divisions.
- Token amounts are native `u128`; the token program has no decimals field.
- Rust release builds wrap silently on overflow, so `u128` safety is proven
  (`docs/overflow-proof.md`), not assumed.
- Determinism is bit-for-bit: the zkVM workspace checks host-vs-guest parity
  over the whole fixture set.

## The math

```
tokensOut = balanceOut · (1 − (balanceIn/(balanceIn+amountIn))^(w_coll/w_token))
```

- `base = balanceIn/(balanceIn+amountIn) ∈ (0,1)`; `exponent = w_coll/w_token`
  spans ~0.0101 to 99 as weights shift from 99/1 to 1/99.
- `pow` is computed as `exp(y·ln x)`. Because `base < 1` and `exponent > 0`,
  the `exp` argument is always ≤ 0 and the output lands in (0,1] — Balancer's
  large-argument `exp` machinery is unnecessary by construction.
- Range reduction is by powers of two and `ln2` (shifts and one multiply, not
  divisions); series run on the small remainder. One `pow` costs exactly one
  hardware division (ADR 0004).
- Balances and amounts are raw `u128` wei; `Fixed` (i128, 52 fractional bits)
  appears only for the internal ratio and exponent (ADRs 0002, 0003, 0008).
- Rounding always favours the pool. The payout path keeps `1 − power` at the
  internal 62-bit scale into the final widened multiply, which is also what
  handles the sale-start cancellation risk — no expm1 branch exists (ADR 0005).
- The one `128 × 128 → 256` widening sits behind a fit assertion; results past
  `u128` halt rather than wrap.
- Ground truth is mpmath at arbitrary precision; Balancer's `LogExpMath` is a
  secondary comparator. The claim is "correct to a proven error bound", not
  bit-identical to Balancer (ADR 0001).

## Where the decisions live

Each significant decision has an ADR in `docs/adr/`: harness architecture and
error bounds (0001), u128 balances (0002), `SCALE = 52` (0003), series choice
(0004), no expm1 boundary (0005), exact-out inversion and the 30% out cap
(0006), the exact big-integer invariant referee (0007), integer width (0008).
Superseded working documents are under `docs/archive/`.
