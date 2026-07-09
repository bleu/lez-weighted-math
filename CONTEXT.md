# Design brief

**Project:** a fixed-point weighted-pool `pow` kernel for a Balancer-style LBP on
the Logos Execution Zone (LEZ). Triple duty: (1) open-source track-record
artifact for RFP-016, (2) the M1 on-chain guest logic, (3) audit surface.
License MIT + Apache-2.0.

## Platform facts (verified against `lez-programs` main)

- LEZ programs are RISC Zero (RISC0) zkVM guests, RV32IM 32-bit words → `u128`
  is multi-limb; **DIVISION is the most expensive zkVM primitive** (minimize it).
- Token amounts are native `u128`; the token program has **no decimals field**.
- Rust release builds **wrap silently on overflow** — `u128` safety must be
  proven, not assumed.
- Determinism must be **bit-for-bit**.

## The math

```
tokensOut = balanceOut · (1 − (balanceIn/(balanceIn+amountIn))^(w_coll/w_token))
```

- `base = balanceIn/(balanceIn+amountIn) ∈ (0,1)`; `exponent = w_coll/w_token`,
  spanning ~0.0101 → 99 as weights shift 99/1 → 1/99.
- `pow` computed as `exp(y·ln x)`. Because `base < 1` and `exponent > 0`, the
  `exp` argument is always ≤ 0 → output ∈ (0,1]. This deletes Balancer's
  large-argument machinery.
- **Algorithm:** range-reduce by `ln2` (shifts, not divisions), series on the
  `[0, 0.693]` remainder.
- **Fixed-point scale ~2^52** (SWEEP against mpmath to confirm; don't hand-pick).
- **Rounding always favors the pool:** `powUp` → `complement(round down)` →
  `tokensOut` floors. PORT the `WeightedMath` wrappers (`powUp`/`divUp`/
  `mulDown`/`complement`), not just `pow`.
- **Sale-start catastrophic cancellation** (exponent ≈ 0.0101 → power ≈ 1 →
  `1 − power` loses precision): handle with `−expm1(y·ln base)`.
- **Overflow envelope:** enforced bounds are weight ratio ≤ 99/1 and total
  deposit < 2^128. The outer multiply `balanceOut·(1−power)` uses one localized
  128×128→256 intermediate (widened, not a bignum lib). Ships with a **written
  overflow proof**.
- **Ground truth = mpmath** (arbitrary precision). Balancer's `LogExpMath` is the
  secondary comparator. Claim is "correct to a proven error bound," **not**
  bit-identical to Balancer.

## Open decisions to resolve in the grill (Phase 1)

- Target error bound (the pass/fail number).
- Fixed-point scale via sweep.
- Series choice (Taylor vs minimax) + term count.
- `expm1` boundary threshold.
- Exact wrapper set + rounding-direction invariants.
- Which Balancer reference and how its outputs are pulled.
