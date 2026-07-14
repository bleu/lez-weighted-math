# ADR 0004: Series choice — atanh for ln, alternating Taylor for exp, Horner throughout

Status: **ACCEPTED** (kernel implementation). Settles the series-choice
question from the design brief.

## Context

Division is the most expensive RISC0 primitive, so the series question was
never just "Taylor vs minimax term count" — it was "how many divisions does
one `pow` cost." Balancer's `LogExpMath` spends a couple dozen divisions
per pow (range-reduction ladders plus per-term divides). The brief demanded
better.

## Decision

Both series run at the internal `LN_SCALE = 62` and evaluate via Horner
with precomputed reciprocal constants, so **one `pow` costs exactly one
hardware division** (forming `t` inside `ln`):

- **ln**: shift-normalize into `m ∈ [√2/2, √2)` (powers of two cost
  nothing), then the odd atanh series `ln m = 2t·(1 + t²/3 + t⁴/5 + …)`
  with `t = (m-1)/(m+1)`, `|t| <= 0.1716`. 13 terms of
  `round(2^62/(2i+1))`: the truncated tail is below `2^-67`.
- **exp**: range-reduce by `ln2` via one multiply by `round(2^30/ln2)`
  (plus a one-step correction loop), then the alternating Taylor series on
  the remainder `s ∈ [0, ln2)` with 20 signed constants
  `(-1)^i·round(2^62/i!)`: truncation below `2^-66`, and the alternating
  signs make the truncation bound trivial.

Taylor over minimax: at 62 internal bits the truncation error of both
series is already 4+ bits below the rounding noise of the Horner loop
itself, so a minimax polynomial would buy nothing measurable while making
the constants unverifiable by inspection (each Taylor constant is
`2^62/n` or `2^62/n!` — an auditor can recompute them in one line).

## Consequences

- Measured against the oracle at `SCALE = 52`: raw pow error stays inside
  the one-ulp quantization interval on every fixture case, including the
  sale-start and exponent-99 danger zones (see ADR 0003's sweep table).
- `calc_out_given_in` costs three divisions total: base ratio, exponent
  ratio, and `t` — versus ~25+ in the Balancer-shaped alternative.
- The constants block in `pow.rs` states each derivation formula inline;
  regeneration is a two-line mpmath script.
