# Overflow-envelope proof

Status: current as of the `exp`/`expm1` saturation fix (see Findings). Covers
every arithmetic operation in `weighted-math-core` (`fixed.rs`, `pow.rs`,
`weighted.rs`, `wide.rs`).

Rust release builds can wrap integer arithmetic silently, and the kernel's
target is a RISC0 guest whose build profile we do not control from this
crate. The design brief therefore demands a written proof that, inside the
enforced envelope, no intermediate ever exceeds its type. This document is
that proof. Each section is a ledger: every multiply, add, subtract, and
shift in the code, its worst-case magnitude, and why that magnitude fits.

Two kinds of guarantee appear below and it matters which is which:

- proved: the bound follows from the envelope by arithmetic; the operation
  can never overflow in-domain. Some of these carry a `debug_assert!` as a
  cross-check, which compiles away in release.
- guarded: the bound is a representability limit of the *inputs* (a payment
  that genuinely exceeds `u128`, a spot price beyond `Fixed`). These are
  hard `assert!`s, active in release: the kernel halts instead of wrapping.

This workspace also sets `overflow-checks = true` for the release profile.
The proof does not lean on that (a guest image may build without it), and
the test suite passes with checks stripped; see the last section.

## The enforced envelope

All of these are hard `assert!`s (or `checked_add`), active in release.

| bound | enforced in |
|---|---|
| `balance_in >= 1`, `balance_out >= 1` | `weighted.rs` (both swap paths, `spot_price`) |
| `balance_in + amount_in < 2^128` | `calc_out_given_in` (`checked_add`) |
| `1 <= weight <= 2^64`, ratio within `[1/99, 99]` | `check_weights` |
| `amount_out <= floor(30% of balance_out)` | `calc_in_given_out` |
| `base in (0, 1)`, `exponent in (0, 99]` | `pow_62` / `pow_raw` domain asserts |
| `x in (0, 1]` for `ln`; `x <= 0` for `exp`/`expm1` | `pow.rs` public wrappers |
| widened products fit `u128` after their shift | `wide::mul_shr` / `mul_shr_up` |
| `Fixed` results fit `i128`; division numerators fit | `fixed.rs` (`checked_repr`, `numerator`) |
| `7 <= SCALE <= 60` | compile-time consts in `weighted.rs` / `pow.rs` |

Notation for the rest of the document: `S = SCALE` with `7 <= S <= 60`
(the harness further pins `S >= 40`, but the proof holds on the kernel's
own range), `L = LN_SCALE = 62`, `ONE = 2^S`, `ONE_62 = 2^62`. An `i128`
must stay below `2^127` in magnitude, a `u128` below `2^128`. Worst cases
are always taken at `S = 60`.

## The widening primitive (`wide.rs`)

`mul_wide(a, b)` splits each operand into 64-bit halves. Each of the four
partial products is at most `(2^64 - 1)^2 < 2^128`, so none can wrap. The
cross terms `lh + hl` can carry once; `overflowing_add` records that carry
and it is re-added into the high word at bit 64. The high-word sum
`hh + (mid >> 64) + carry_mid*2^64 + carry_lo` is a sum of nonnegative
terms whose mathematical total is `floor(a*b / 2^128) <= 2^128 - 2`, and a
running sum of nonnegative terms never exceeds its total, so no step of
that addition wraps either. The result is the exact 256-bit product.

`mul_shr` and `mul_shr_up` then assert `hi >> shift == 0`, i.e. that the
shifted product fits `u128`, before recombining. This assert is release-
active: any call site whose fit argument failed would panic, not wrap.
The proof obligation at each call site is therefore either "the product is
provably below `2^(128+shift)`" (the assert can never fire) or "the assert
firing is the intended envelope boundary" (guarded).

`mul_shr_up` additionally adds the rounding carry to `floor`. The carry can
only push the result to at most `ceil(a*b / 2^shift)`; when that value is
`2^128` the low-word reconstruction would already have tripped the fit
assert, so the `+ 1` cannot wrap. (Concretely: the carry is nonzero only
when discarded bits exist, and then `floor < ceil <= a*b/2^shift < 2^128`.)

## Ledger: `calc_out_given_in`

| # | intermediate | worst case | why it fits |
|---|---|---|---|
| 1 | `total = balance_in + amount_in` | `2^128 - 1` | `checked_add`; the envelope boundary itself (guarded) |
| 2 | `99 * weight_out`, `99 * weight_in` | `99 * 2^64 < 2^71` | `u128`, weights capped at `2^64` |
| 3 | `weight_in << S` | `2^64 * 2^60 = 2^124` | `u128`; `S <= 60` is compile-time |
| 4 | `exponent = floor(w_in*2^S / w_out)` | `<= 99 * 2^S < 2^66.7` | fits `i128`; `>= 1` because `2^S >= 99` (the `S >= 7` const assert) |
| 5 | `base = ratio_up(balance_in, total)` | all terms `< 2^127` | ratio lemma below |
| 6 | the pow pipeline | see next section | |
| 7 | `omp62 = one_minus_pow_62(...)` | in `[0, 2^62]` | `.max(0)` floor; `pow_62 >= 0` and the pad only subtracts (debug-asserted) |
| 8 | `mul_shr(balance_out, omp62, 62)` | product `< 2^190` | 256-bit intermediate; `hi = product >> 128 < 2^62`, so the fit assert always passes; result `<= balance_out < 2^128` |

Step 8 is the full-width widened multiply the design brief names; see "The
128 x 128 -> 256 step" below.

If quantization pushes `base` to `ONE` or above (a trade too small to move
the ceiling-rounded ratio), the function returns 0 before touching `pow`,
which keeps `pow`'s `base < ONE` domain assert unreachable from here.

### Ratio lemma (`ratio_up`, `ratio_down`)

Both helpers pre-shift wide operands by
`excess = bitlen(widest operand) - (126 - S)` (when positive) so the
shifted values have at most `126 - S` bits:

- `ratio_up`: `n = (num >> excess) + 1 <= 2^(126-S)` and
  `d = den >> excess < 2^(126-S)`, with a release `assert!(d > 0)`. Then
  `n << S <= 2^126`, the ceiling bias `+ d - 1` keeps the numerator below
  `2^126 + 2^126 = 2^127` (`u128`, fine), and the quotient
  `q <= n << S <= 2^126 < 2^127`, so the cast to `i128` is safe
  (debug-asserted). The `d > 0` assert is a rounding guard, not an overflow
  one: `den >> excess == 0` happens only when `num` is the wider operand by
  more than the pre-shift can carry (a ratio past the `Fixed` range), where
  clamping `d` up to 1 would silently understate. It halts instead — the
  documented `spot_price` panic. The fund paths pass `num <= den`, so `den`
  is the wider operand and `den >> excess >= 1` always (the assert is
  unreachable there).
- `ratio_down` (requires `num < den`, debug-asserted): `n <= den >> excess
  < d`, so `n << S <= 2^126` as above and the quotient is strictly below
  `ONE` (debug-asserted).

The pre-shift bias directions (numerator up / denominator down for
`ratio_up`, the mirror for `ratio_down`) are a rounding-correctness
concern, not an overflow one; they are covered in the module docs.

## Ledger: the pow pipeline (`pow.rs`)

Everything below runs on `i128` at the internal scale `L = 62`.

### `ln_inner(x)`, `x in [1, 2^S]`

| intermediate | worst case | why |
|---|---|---|
| `m0 = x << (62 - S)` | `<= 2^62` | `x <= 2^S` |
| doubling count `k` | `<= S <= 60` | `m0 >= 2^(62-S)` and the loop stops at `~0.7071 * 2^62 < 2^62`; after `S` doublings `m >= 2^62` (debug-asserted) |
| normalized `m` | `in [0.7071, 1.4143) * 2^62 < 2^63` | loop invariant (debug-asserted) |
| `(m - 2^62) << 62` | `<= 0.4143 * 2^124 < 2^123` | `|m - 2^62| <= (sqrt(2)-1) * 2^62` |
| `m + 2^62` | `< 2.4143 * 2^62 < 2^64` | |
| `t` | `<= 0.17158 * 2^62 < 2^60` | `|t| <= (sqrt(2)-1)/(sqrt(2)+1) * 2^62` at both ends of the `m` range |
| `t * t` | `< 2^120` | |
| `u = t*t >> 62` | `<= 0.02945 * 2^62 < 2^57` | |
| Horner `p` | `<= 1.031 * 2^62 < 2^63` | recurrence `|p'| <= 2^62 + |p| * 0.02945` closes at `2^62 / (1 - 0.02945)` |
| `p * u` | `< 0.0304 * 2^124 < 2^120` | |
| `t * p` | `< 0.177 * 2^124 < 2^122` | |
| `ln_m = t*p >> 61` | `<= 0.354 * 2^62` | |
| `k * LN2_62` | `<= 60 * 0.6932 * 2^62 < 2^67.4` | `k <= S <= 60` |
| result | `in [0, 42 * 2^62] < 2^68` | nonnegative: `k = 0` forces `m <= 2^62` hence `ln_m <= 0`; `k >= 1` gives `k*ln2*2^62 >= 0.69 * 2^62 > |ln_m|` |

### The argument product `exponent * (-ln base)`

`exponent <= 99 * 2^S` (asserted) and `-ln base < 42 * 2^62` (above), so
the product is below `4158 * 2^(S+62) < 2^(S + 74.1)`. That exceeds `u128`
once `S >= 54`, which is why this multiply is widened even though it
happens to fit at the current `S = 52`. The fit assert needs the product
below `2^(128+S)`, which holds with over 50 bits to spare, so this call can
never panic in-domain. After `>> S` the argument is below `2^74.1`,
comfortably `i128`, and inside `exp_inner`'s `2^76` envelope.

### `exp_inner(neg_arg)`, envelope `0 <= neg_arg < 2^76`

The envelope is debug-asserted at entry. Its two caller classes are the pow
paths (`< 2^74.1`, proved above) and the public `exp`/`expm1`, which
saturate at `64 * 2^62 = 2^68` (see Finding 1).

| intermediate | worst case | why |
|---|---|---|
| `neg_arg as u128 * INV_LN2_30` | `< 2^76 * 1.4427 * 2^30 < 2^107` | the reason the envelope exists; `u128` holds it |
| initial `k` | `< 2^15` | `neg_arg / (ln2 * 2^62) < 2^14.6` |
| `k * LN2_62` | `<= neg_arg + ln2 * 2^62 < 2^76.1` | `i128` |
| correction loops | at most one step each | `INV_LN2_30` is round-to-nearest, relative error `< 2^-31`; over `neg_arg < 2^76` the estimate is off by `< 2^-16` plus the floor |
| reduced `s` | `in [0, 0.6932 * 2^62)` | loop postcondition |
| Horner `acc` | `<= 3.27 * 2^62 < 2^64` | recurrence `|acc'| <= 2^62 + |acc| * 0.6932` closes at `2^62 / (1 - 0.6932)` |
| `acc * s` | `< 2.27 * 2^124 < 2^126` | |
| final `acc` | `in [0, 2^62]` | upper: the depth-1 Horner partial equals `2^62 * (-1 + s'/2! - s'^2/3! + ...) <= -0.57 * 2^62` for `s' in [0, 0.694]` (rounding shifts it by `< 40` units), so the last step adds a negative correction to `C0 = 2^62`; lower: the error analysis (`docs/error-analysis.md`) keeps `acc` within `~20` units of `2^62 * exp(-s) >= 0.5 * 2^62` |
| `acc >> k` | `<= acc` | `k <= 62` here (larger `k` returned 0 already), so the shift amount is valid |

### Final rounding

`to_scale_nearest` adds `2^(62-S-1) <= 2^54` to a value `<= 2^62`; no
overflow. The directional pads (`POW_PAD_ULPS = 2`, `PAD_62 = 512`) adjust
values in `[0, 2^62]` by constants, clamped by `.max(0)` / `.min(ONE)`.

## Ledger: `calc_in_given_out`

| # | intermediate | worst case | why it fits |
|---|---|---|---|
| 1 | `cap = b/10*3 + b%10*3/10` | `= floor(3b/10) < 2^128` | `b/10*3 <= 3b/10`; second term `<= 27/10 = 2` |
| 2 | `balance_out - amount_out` | `>= ceil(0.7 * balance_out) >= 1` | `amount_out <= floor(0.3 * balance_out)` and `balance_out >= 1` |
| 3 | `(w_out << S) + w_in - 1` | `<= 2^124 + 2^64 < 2^125` | `u128` |
| 4 | `exponent = ceil(w_out*2^S / w_in)` | `<= 99 * 2^S` | exact at the ratio-99 boundary (`w_out = 99*w_in` divides evenly), below it otherwise; `pow`'s domain assert can never fire in-envelope |
| 5 | `base = ratio_down(...)` | all terms `< 2^127` | ratio lemma; positive because the numerator is at least `0.7` of the denominator |
| 6 | `p62 = pow_62_down(...)` | `in [1, 2^62]` | lower bound is a hard assert; with the 30% cap the true power is `>= 0.7^99 ~ 2^-50.9`, about `2100` units at `2^62`, four times the `512` pad (upper bound debug-asserted) |
| 7 | `(2^62 - p62) << 62` | `<= 2^124` | `u128` |
| 8 | `num + p62 - 1` | `< 2^124 + 2^62 < 2^125` | `u128` |
| 9 | `r62 = ceil(num / p62)` | `<= (2^62 - 1) * 2^62 < 2^124` | maximized at `p62 = 1` (debug-asserted) |
| 10 | `mul_shr_up(balance_in, r62, 62)` | product `< 2^252` | 256-bit intermediate; the fit assert IS the "payment must be representable" envelope: a payment `>= 2^128` panics in release (guarded) |

Step 10 is the exact-out mirror of the payout multiply.

## Ledger: `spot_price` and the `Fixed` wrappers

`spot_price` composes `ratio_up(balance_in, balance_out)` (lemma above,
result `<= 2^126`) with `wratio = ceil(w_out*2^S / w_in) <= 99 * 2^S <
2^66.7` through `Fixed::mul_up`. The 256-bit product is at most `2^192.7`;
whether it survives the `>> S` fit assert and the `i128` cast depends on
the actual price. Prices beyond the `Fixed` range panic (guarded) — either
at `ratio_up`'s `d > 0` assert when the balance ratio alone is out of range,
or at `mul_up`'s fit assert when the weight ratio tips an in-range balance
ratio over. The function is informational and moves no funds; the panic
boundary is documented in its rustdoc.

The `Fixed` wrappers themselves:

- `mul_down` / `mul_up`: `mul_wide` plus the fit assert plus
  `checked_repr` (`< 2^127`). Out-of-range products panic, never wrap.
- `div_down` / `div_up`: `numerator()` asserts `a < 2^(127-S)` so
  `a << S < 2^127`; `div_up`'s ceiling bias `n + d - 1 < 2^127 + 2^127 =
  2^128` stays in `u128`.
- `complement`: `ONE - x` with `x in [0, 2^127)` lands in
  `(2^S - 2^127, 2^S]`, inside `i128`.

## The 128 x 128 -> 256 step

The design brief calls for "one localized 128 x 128 -> 256 intermediate".
Since ADR 0007 added the exact-out path, the accurate statement is one
full-width widened multiply per swap path, both landing at the very end of
their pipeline:

- `calc_out_given_in` step 8: `balance_out * omp62`, up to
  `2^128 * 2^62 = 2^190`. This is the step the brief names.
- `calc_in_given_out` step 10: `balance_in * r62`, up to
  `2^128 * 2^124 = 2^252`, its ADR 0007 mirror.

`mul_wide` has two other call sites, widened for uniformity and sweep
headroom rather than out of full-width necessity: the pow argument product
(`< 2^(S+74.1)`, only passes `2^128` at `S >= 54`) and `Fixed::mul_*`
(reachable at width only through `spot_price`, where the fit assert
guards). No path performs a 256-bit division anywhere; the widened values
are only multiplied, added, and shifted.

## What the proof surfaced

### Finding 1 (bug, fixed): `exp`/`expm1` truncated deep-negative arguments

The public `exp`/`expm1` used `(-x.0).checked_shl(LN_SCALE - SCALE)` and
treated `None` as underflow. But `checked_shl` returns `None` only when the
*shift amount* is `>= 128`; it never detects discarded value bits. The
`None` arm was dead code, and the actual behavior partitioned into three
broken regimes (at `S = 52`; measured, not just derived):

- `x.0 <= -2^117`: the shift silently drops high bits in every build
  profile, overflow-checks included. `exp(Fixed(-(1 << 120)))` returned
  exactly `ONE`, i.e. `exp(-2^68) = 1.0`.
- `-2^117 < x.0 <= ~-2^88`: the shift survives but `neg_arg *
  INV_LN2_30` overflows `u128` inside `exp_inner` (panic with
  overflow-checks, garbage `k` without).
- `x.0 = i128::MIN`: the negation itself overflows.

Not reachable from the swap paths, which call `exp_inner` with the proven
`< 2^74.1` argument, but the ln/exp wrappers are public crate API and part
of the audit surface. Fixed by saturating at `x <= -64` before negating
(`exp(-64) < 2^-92`, below any sweepable grid), which also caps
`exp_inner`'s argument at `2^68`. Regression coverage:
`exp_saturates_deep_negative` and `exp_total_on_nonpositive` in
`crates/harness/tests/overflow_envelope.rs`.

### Finding 2 (fixed): `exp_inner` had no stated argument envelope

Its `k` estimate multiplies `neg_arg` by a 31-bit constant in `u128`, which
silently assumed `neg_arg < ~2^97`. The envelope is now explicit
(`< 2^76`), debug-asserted at entry, and discharged by both caller classes
(pow paths `< 2^74.1`, public wrappers `<= 2^68`).

### Finding 3 (fixed): the sweep had no lower `SCALE` bound

`SCALE <= 60` was compile-time-asserted, but nothing stopped a sweep below
`2^S >= 99`, where the 1/99 exponent floors to zero and trips `pow`'s
runtime domain assert (a panic, not a wrap, but still an envelope hole).
Now a compile-time assert in `weighted.rs`: `SCALE >= 7`.

### Accepted panic boundaries (by design, release-active)

These are the guarded bounds, where halting is the specified behavior:
total deposit past `u128` (`checked_add`), an exact-out payment past
`u128` (step 10 fit assert), a spot price past `Fixed` (fit assert plus
`checked_repr`), and the `Fixed` wrapper range asserts. None of them can
wrap; all fire in release builds regardless of `overflow-checks`.

## How the proof is backed in code

Cross-check `debug_assert!`s were added at the bound entry points; each
one restates a "proved" row above (they compile away in release, which is
fine, because those rows cannot fail in-domain):

- `pow.rs`: `exp_inner` argument envelope (`< 2^76`); `ln_inner`
  normalization invariants (`k <= SCALE`, `m` in the sqrt(2) window).
- `weighted.rs`: `omp62 in [0, 2^62]`, `p62 <= 2^62`, `r62 <= 2^124`,
  `ratio_up` fits `i128`, `ratio_down` stays below `ONE`.

`crates/harness/tests/overflow_envelope.rs` hammers the envelope in ways a
wrapped intermediate could not survive, and is meant to run in release as
well as debug:

- `calc_out_at_the_deposit_edge`: total deposit pinned to exactly
  `2^128 - 1`, balances up to full width, 99/1 weights oversampled, with a
  payout floor a wrap would break.
- `payout_multiply_is_linear_in_balance_out` and
  `payment_multiply_is_linear_in_balance_in`: the two full-width widened
  multiplies checked by exact 2x scaling near `2^127`.
- `pow_at_the_domain_corners`: maximal `|ln|` times maximal exponent.
- `exp_saturates_deep_negative`, `exp_total_on_nonpositive`: Finding 1
  regressions over the whole nonpositive `i128` line.
- `unrepresentable_payment_panics`, `unrepresentable_spot_price_panics`:
  the guarded boundaries fire as panics in release, not wraps.

Runs that back this document (all green at `SCALE = 52`):

```
cargo test --workspace
cargo test --workspace --release
cargo test --workspace --release --config 'profile.release.overflow-checks=false'
```

The last one strips the workspace's release overflow checks to simulate a
guest-like build; the invariants still hold, so nothing in the suite was
being saved by a checked panic.
