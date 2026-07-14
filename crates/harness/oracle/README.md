# Oracle: mpmath ground-truth generator

Offline only. CI reads the committed `../fixtures/*.json` and never runs
Python. Regeneration is deterministic — rerunning must produce byte-identical
files.

## Regenerate

```sh
pip install -r requirements.txt
python3 gen.py
```

## What each fixture means

- `pow.json` — the primitive-gate cases: `(base, exponent)` pairs with the
  exact `base^exponent` as a 78-digit decimal and as `floor(value * 2^128)`.
  Inputs are dyadic rationals on a `2^-40` grid (also stored as `*_s40 =
  value * 2^40` integers), so every candidate `SCALE >= 40` represents them
  exactly and the gate measures only algorithmic error. Danger zones lead
  the file: `sale_start` first, then `exp_high`.
- `ln_exp.json` — diagnostic cases for the kernel's internal bricks
  (`ln`, `exp`, `expm1`). `-ln` truths are stored as `floor(value * 2^116)`
  since `-ln` exceeds 1; `exp`/`-expm1` truths as q128.
- `out_given_in.json` — the economic gate: full swaps with raw `u128`
  balances/amounts/weights (ADR 0003) and the true pool-favouring floored
  payout `tokens_out_floor`. `sens_base_wei`/`sens_exp_wei` are first-order
  payout sensitivities (wei per unit of input error) so the harness derives
  a SCALE-parametric magnitude bound; `one_minus_power_exact` is computed
  via `expm1`, never by subtraction.
- `in_given_out.json` — the reverse economic gate: exact-out trades with
  the true ceiled payment `amount_in_ceil` (the pool never undercharges)
  and three sensitivities (`sens_base_wei`, `sens_exp_wei`, `sens_pow_wei`)
  for the parametric magnitude bound. Note the danger zone flips here:
  sale-start exact-out trades sit at exponent 99, not 0.0101 (ADR 0007).
- `arith.json` — input pairs for the mul/div/complement wrappers. No truth
  stored: the harness recomputes the exact rational at double width.
  `spot_price` is likewise checked against the exact rational at double
  width (over the `out_given_in.json` inputs), so it needs no fixture.
- `scales.json` — the quantization floor per candidate `SCALE` for the
  scale sweep (mechanism in ADR 0002; result in ADR 0004).
- `balancer_inputs.json` — the pow cases rounded onto Balancer's 1e18 grid,
  with truth recomputed at that grid and out-of-domain cases marked `skip`.
  `../balancer-ref/inputs_flat.json` is the same data flattened for the
  forge capture script (see `../balancer-ref/README.md`).
