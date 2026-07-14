//! Weighted-pool swap math (Balancer-style) built on top of [`crate::pow`].
//!
//! Balances, amounts, and weights cross this API as **raw `u128`** — the LEZ
//! native token unit (no decimals field). `Fixed` appears only where the
//! math needs a fractional value in a bounded range: the ratio
//! `base = balance_in / (balance_in + amount_in)` and the exponent
//! `weight_in / weight_out`. See ADR 0003.
//!
//! Every rounding favours the pool. The composition (mirroring Balancer's
//! `computeOutGivenExactIn`, adapted to raw-integer balances):
//! - `base` rounds UP: `power = base^y` is overstated, `1 - power`
//!   understated, payout understated;
//! - `exponent` rounds DOWN: with `base < 1`, a smaller exponent also
//!   overstates the power;
//! - `1 - power` is padded down inside the kernel pipeline;
//! - the final payout multiply floors.
//! So the kernel payout never exceeds the true one.
//!
//! `calc_in_given_out` naively needs `pow` with base > 1, which this kernel
//! does not support (`base ∈ (0,1)` is what deletes the large-argument `exp`
//! machinery). Instead it is algebraically inverted into the native domain —
//! see its docs and ADR 0007. `spot_price` needs no `pow` at all.
//!
//! # Enforced envelope (asserted, so violations panic instead of wrapping)
//!
//! - reserves >= 1 wei; total deposit `balance_in + amount_in < 2^128`;
//! - weights >= 1, each <= 2^64, ratio within `[1/99, 99]`.
//!
//! # Overflow proof
//!
//! The full written proof, with the per-step bound ledger, is
//! `docs/overflow-proof.md`; this is the summary. With the envelope above,
//! every intermediate fits its type:
//! 1. `total = balance_in + amount_in` — `checked_add`, < 2^128 by bound.
//! 2. `exponent = (weight_in << SCALE) / weight_out` — `weight_in <= 2^64`
//!    and `SCALE <= 60`, so the numerator is < 2^124; the ratio bound puts
//!    the result in `[2^SCALE/99, 99·2^SCALE]` ⊂ i128.
//! 3. `base` (see [`ratio_up`]) — operands are pre-shifted so the numerator
//!    stays < 2^127 and the `+ d - 1` ceiling bias cannot overflow.
//! 4. inside `pow`: see the analysis in [`crate::pow`] (widened multiply
//!    for `y·ln x`; every series product bounded below 2^125).
//! 5. `balance_out · omp62` — the one place a 256-bit intermediate is
//!    genuinely needed: `balance_out < 2^128` times `omp62 <= 2^62` is up
//!    to 2^190, handled by the widening multiply and shifted back down by
//!    62, so the result is at most `balance_out`. This is the localized
//!    `128×128 → 256` multiply the design brief calls for.

use crate::fixed::{Fixed, ONE, SCALE};
use crate::{pow, wide};

// The extreme 1/99 weight ratio must stay a nonzero exponent:
// floor(2^SCALE / 99) >= 1 requires 2^SCALE >= 99.
// (a compile-time guard for the sweep, not a runtime assertion)
#[allow(clippy::assertions_on_constants)]
const _: () = assert!(
    SCALE >= 7,
    "SCALE must be >= 7 so the 1/99 exponent stays nonzero"
);

/// Amount of `token_out` received for a given `amount_in` of `token_in`.
///
/// `out = balance_out * (1 - (balance_in / (balance_in + amount_in))^(weight_in / weight_out))`
///
/// All quantities are raw `u128` wei / raw weight units. The result is
/// rounded down (pool-favouring); a trade too small to move the quantized
/// ratio pays zero.
pub fn calc_out_given_in(
    balance_in: u128,
    weight_in: u128,
    balance_out: u128,
    weight_out: u128,
    amount_in: u128,
) -> u128 {
    assert!(balance_in >= 1 && balance_out >= 1, "empty reserve");
    let total = balance_in
        .checked_add(amount_in)
        .expect("total deposit must stay below 2^128");
    check_weights(weight_in, weight_out);

    // Exponent rounds down, base rounds up: both overstate the power.
    let exponent = Fixed(((weight_in << SCALE) / weight_out) as i128);
    let base = ratio_up(balance_in, total);
    if base.0 >= ONE.0 {
        return 0;
    }

    // 1 - power at the internal 62-bit scale, rounded down (see pow.rs).
    let omp62 = pow::one_minus_pow_62(base, exponent);
    debug_assert!((0..=ONE_62).contains(&omp62));

    // tokens_out = floor(balance_out * omp62 / 2^62): the widened multiply.
    wide::mul_shr(balance_out, omp62 as u128, pow::LN_SCALE)
}

/// Amount of `token_in` required to receive a given `amount_out` of
/// `token_out`.
///
/// `in = balance_in * ((balance_out / (balance_out - amount_out))^(weight_out / weight_in) - 1)`
///
/// Rounded up (the pool never undercharges). Internally the base-above-one
/// form is inverted into the kernel's native domain:
/// `(b/(b-a))^y - 1 = (1-p)/p` with `p = ((b-a)/b)^y ∈ (0,1)`, so every
/// rounding direction flips once: base' DOWN, exponent UP, power padded
/// DOWN — all of which overstate the payment.
///
/// Envelope (ADR 0007): `amount_out <= 30%` of the reserve (Balancer
/// parity; also keeps `p` far above the kernel's pad floor), and the
/// resulting payment must fit `u128` (the widened multiply asserts).
pub fn calc_in_given_out(
    balance_in: u128,
    weight_in: u128,
    balance_out: u128,
    weight_out: u128,
    amount_out: u128,
) -> u128 {
    assert!(balance_in >= 1 && balance_out >= 1, "empty reserve");
    check_weights(weight_in, weight_out);
    // floor(0.3 · balance_out) without overflowing 3·balance_out
    let cap = balance_out / 10 * 3 + balance_out % 10 * 3 / 10;
    assert!(
        amount_out <= cap,
        "amount_out envelope: <= 30% of the reserve"
    );
    if amount_out == 0 {
        return 0;
    }

    // Exponent rounds up, base' rounds down: both understate p.
    let exponent = Fixed((((weight_out << SCALE) + weight_in - 1) / weight_in) as i128);
    let base = ratio_down(balance_out - amount_out, balance_out);
    let p62 = pow::pow_62_down(base, exponent);
    // With the 30% cap, p >= 0.7^99 ~ 2^-50.6, four times the pad floor.
    assert!(p62 > 0, "power underflows the internal scale");
    debug_assert!(p62 <= ONE_62);

    // (1 - p)/p at LN_SCALE, rounded up: numerator <= 2^124 fits u128.
    let num = ((ONE_62 - p62) as u128) << pow::LN_SCALE;
    let r62 = (num + p62 as u128 - 1) / p62 as u128;
    debug_assert!(r62 <= 1 << 124);

    // amount_in = ceil(balance_in · r62 / 2^62): the widened multiply. Its
    // fit assertion is the "payment must be representable" envelope.
    wide::mul_shr_up(balance_in, r62, pow::LN_SCALE)
}

/// Spot price of `token_in` in terms of `token_out` (ignoring swap fees):
/// `(balance_in / weight_in) / (balance_out / weight_out)`, rounded up.
///
/// Informational (no funds move on it): composed from two up-rounded
/// ratios, so it never understates the true price and sits within a few
/// ulps above it (checked exactly by the harness at double width). Panics
/// if the price exceeds the `Fixed` range.
pub fn spot_price(balance_in: u128, weight_in: u128, balance_out: u128, weight_out: u128) -> Fixed {
    assert!(balance_in >= 1 && balance_out >= 1, "empty reserve");
    check_weights(weight_in, weight_out);
    let wratio = Fixed((((weight_out << SCALE) + weight_in - 1) / weight_in) as i128);
    ratio_up(balance_in, balance_out).mul_up(wratio)
}

const ONE_62: i128 = 1 << pow::LN_SCALE;

fn check_weights(weight_in: u128, weight_out: u128) {
    assert!(weight_in >= 1 && weight_out >= 1, "zero weight");
    assert!(
        weight_in <= 1 << 64 && weight_out <= 1 << 64,
        "weight envelope: <= 2^64"
    );
    assert!(
        weight_in <= 99 * weight_out && weight_out <= 99 * weight_in,
        "weight ratio envelope: within [1/99, 99]"
    );
}

/// `num / den` as a `Fixed`, rounded UP, for operands of full `u128` range —
/// one hardware division. `num` may exceed `den` (spot-price ratios).
///
/// When the operands are too wide for the `num << SCALE` numerator to fit,
/// both are pre-shifted down, biased so the ratio can only grow (numerator
/// rounds up, denominator truncates): the result never understates the true
/// ratio. The bias costs at most `~2^-73` relative — far below one ulp at
/// any swept SCALE.
///
/// The pre-shift only preserves the round-up direction while the shifted
/// denominator keeps at least one bit. Once `den >> excess` truncates to
/// zero the numerator dwarfs the denominator by more than the 128-bit
/// division can carry — a ratio far past the `Fixed` range — so the helper
/// halts rather than clamp the denominator up to 1 (which would silently
/// *understate* the ratio, the one direction it must never take). The fund
/// paths never reach this: they pass `num <= den`, so `den` is the wider
/// operand and its shift never underflows.
fn ratio_up(num: u128, den: u128) -> Fixed {
    debug_assert!(den > 0);
    let bits = 128 - num.max(den).leading_zeros();
    let excess = bits.saturating_sub(126 - SCALE);
    let (n, d) = if excess > 0 {
        let d = den >> excess;
        assert!(d > 0, "ratio exceeds the representable Fixed range");
        ((num >> excess) + 1, d)
    } else {
        (num, den)
    };
    // n <= 2^(126-SCALE), so n << SCALE <= 2^126 and `+ d - 1` fits.
    let q = ((n << SCALE) + d - 1) / d;
    debug_assert!(q >> 127 == 0, "ratio fits the signed representation");
    Fixed(q as i128)
}

/// `num / den` as a `Fixed`, rounded DOWN, for `num < den` — the mirror of
/// [`ratio_up`]: the pre-shift bias truncates the numerator and rounds the
/// denominator up, so the result never overstates the true ratio.
fn ratio_down(num: u128, den: u128) -> Fixed {
    debug_assert!(num < den);
    let den_bits = 128 - den.leading_zeros();
    let excess = den_bits.saturating_sub(126 - SCALE);
    let (n, d) = if excess > 0 {
        (num >> excess, (den >> excess) + 1)
    } else {
        (num, den)
    };
    let q = (n << SCALE) / d;
    debug_assert!(q < ONE.0 as u128, "num < den keeps the ratio below one");
    Fixed(q as i128)
}
