# zkVM cycle cost of the weighted-math kernel

Measured 2026-07-10 with the RISC0 3.0.5 executor (no proving), guest rust
toolchain 1.94.1, `SCALE = 52`, guest built in release with
`overflow-checks = true` — the same semantics the on-chain program would
ship. Reproduce with `cd zkvm && cargo run --release` (see `zkvm/README.md`).
This table feeds RFP-016 P3.

## Host-vs-guest parity

`weighted-math-core` compiles unchanged for the RV32IM guest. Across the
whole fixture set — 373 evaluations: 87 pow cases through each of `pow`,
`pow_up`, `pow_down`, 11 ln, 12 exp, 12 expm1, 29 `calc_out_given_in`,
19 `calc_in_given_out`, 29 `spot_price` — every guest output is bit-identical
to the host's. The host harness (mpmath oracle gates in `crates/harness`)
remains the source of truth for correctness; the guest is verified against
it, and the executor run exits nonzero on any mismatch.

Cycle counts are deterministic: repeated runs produce byte-identical output.

## Per-call cost (CU table)

Cycles are RISC0 user cycles measured inside the guest with
`env::cycle_count()` deltas, minus the 98-cycle measurement baseline. No
LEZ fee schedule is published yet, so cycles are the compute unit here;
multiply by the eventual price per cycle. Min/max spread within an op is
input-dependent (normalization shift count, exp underflow early-exit, the
zero-payout guard), not noise.

| op                  | cases | net median | min    | max    | u128 divisions |
|---------------------|------:|-----------:|-------:|-------:|---------------:|
| `pow` (one power)   |    87 |      8,010 |  3,059 |  9,214 | 1 |
| `pow_up` / `pow_down` | 87 each | 8,043 | 3,092 |  9,247 | 1 |
| `ln`                |    11 |      4,608 |  2,722 |  5,023 | 1 |
| `exp`               |    12 |      4,139 |     57 |  4,140 | 0 |
| `expm1`             |    12 |      4,169 |     56 |  4,170 | 0 |
| `calc_out_given_in` (full buy) | 29 | 11,463 | 656 | 11,949 | 3 |
| `calc_in_given_out` |    19 |     14,197 | 10,976 | 14,915 | 4 |
| `spot_price`        |    29 |      2,187 |  1,141 |  3,028 | 2 |

Whole-program cost, running the batch-median case as a one-shot guest
session (startup and I/O included; measured session overhead is ~4,090
cycles on top of the call itself):

| program            | user cycles | padded segment cycles | segments |
|--------------------|------------:|----------------------:|---------:|
| one `pow`          |      12,194 | 65,536 (2^16 minimum) | 1 |
| one full buy       |      15,648 | 65,536 (2^16 minimum) | 1 |
| one exact-out sell |      18,382 | 65,536 (2^16 minimum) | 1 |

A single swap doesn't come close to filling even the minimum segment (a full
buy occupies 24% of 2^16). Against RISC0's default 2^20 segment, one buy is
about 1.1% of a segment, and a batched program fits roughly 90 buys per
segment ((1,048,576 − 4,087) / 11,561).

## Division count and the hotspot

Division is the most expensive primitive in this cost model, and on RV32IM a
`u128` division is a `compiler_builtins` software loop, not one instruction.
Measured cost by operand shape (numerator bits / denominator bits):

| shape     | net cycles |
|-----------|-----------:|
| 124 / 64  |      2,623 |
| 127 / 75  |      2,500 |
| 127 / 65  |      2,392 |
| 91 / 61   |      1,471 |
| 117 / 64  |        351 |
| 51 / 31   |        491 |

Cost varies almost 8x with shape; wide-quotient divisions at the kernel's
actual magnitudes sit at the expensive end, ~2.4–2.6k cycles each.

Static division counts, from the code (all confirmed by the measured deltas):

- `pow` and its wrappers: exactly 1 — forming `t = (m-1)/(m+1)` inside
  `ln_inner`. That one division is ~2.5k of the 8.0k cycles, the largest
  single-instruction cost in the pipeline (~31%).
- `exp` / `expm1`: 0. Range reduction multiplies by a precomputed `1/ln2`.
- `calc_out_given_in`: 3 — exponent formation, `ratio_up`, and the one
  inside `pow`. Together roughly half the 11.5k-cycle buy.
- `calc_in_given_out`: 4 — exponent, `ratio_down`, the pow division, and
  the `(1-p)/p` inversion.
- `spot_price`: 2, no `pow` at all.

The largest *block* is different from the largest instruction: the 20-term
exp Horner loop costs ~4.1k cycles (about half of `pow`), at roughly 210
cycles per 128-bit multiply-shift-add step. If the cycle budget ever needs
trimming, exp term count is the first knob; ADR 0004 chose 20 terms for
error-bound headroom, not cost.

For contrast, a Balancer-style `LogExpMath.pow` runs a couple dozen
divisions per call (range-reduction ladders plus per-term series divisions).
At ~2.5k cycles each that is ~60k cycles of division alone, roughly 5x our
entire buy path. Keeping the kernel at one division per pow is where the
cycle budget was won; this replaces the pre-implementation estimate from the
integer-width decision (ADR 0008, "single-digit thousands of cycles per pow")
with measured numbers of the same order.

## Caveats

- Executor user cycles, not proving time. Proving cost scales with padded
  segment cycles (the 2^po2 column), which is why "fits the minimum
  segment" is the number that matters for fees.
- The guest wrapper reads inputs with `env::read_slice` and commits raw
  words; a real LEZ program's serialization and state access are not
  modeled. Its startup shows up only in the whole-program rows.
- Numbers are for risc0 3.0.5 and rust 1.94.1; a circuit or toolchain
  change moves them. Re-run the harness rather than trusting this file.
