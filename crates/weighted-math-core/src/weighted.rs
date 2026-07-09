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
//! The scaffold's `calc_in_given_out` and `spot_price` are gone: both need
//! `pow` with base > 1, which this kernel deliberately does not support
//! (`base ∈ (0,1)` is what deletes the large-argument `exp` machinery,
//! see `CONTEXT.md`). They come back only with their own design work.
//!
//! # Enforced envelope (asserted, so violations panic instead of wrapping)
//!
//! - reserves >= 1 wei; total deposit `balance_in + amount_in < 2^128`;
//! - weights >= 1, each <= 2^64, ratio within `[1/99, 99]`.
//!
//! # Overflow proof
//!
//! With the envelope above, every intermediate fits its type:
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
    assert!(weight_in >= 1 && weight_out >= 1, "zero weight");
    assert!(
        weight_in <= 1 << 64 && weight_out <= 1 << 64,
        "weight envelope: <= 2^64"
    );
    assert!(
        weight_in <= 99 * weight_out && weight_out <= 99 * weight_in,
        "weight ratio envelope: within [1/99, 99]"
    );

    // Exponent rounds down, base rounds up: both overstate the power.
    let exponent = Fixed(((weight_in << SCALE) / weight_out) as i128);
    let base = ratio_up(balance_in, total);
    if base.0 >= ONE.0 {
        return 0;
    }

    // 1 - power at the internal 62-bit scale, rounded down (see pow.rs).
    let omp62 = pow::one_minus_pow_62(base, exponent);

    // tokens_out = floor(balance_out * omp62 / 2^62): the widened multiply.
    wide::mul_shr(balance_out, omp62 as u128, pow::LN_SCALE)
}

/// `num / den` as a `Fixed`, rounded UP, for `num <= den` of full `u128`
/// range — one hardware division.
///
/// When `den` is too wide for the `num << SCALE` numerator to fit, both
/// operands are pre-shifted down, biased so the ratio can only grow
/// (numerator rounds up, denominator truncates): the result still never
/// understates the true ratio. The bias costs at most `~2^-73` relative —
/// far below one ulp at any swept SCALE. Capped at `ONE`.
fn ratio_up(num: u128, den: u128) -> Fixed {
    debug_assert!(num <= den && den > 0);
    let den_bits = 128 - den.leading_zeros();
    let excess = den_bits.saturating_sub(126 - SCALE);
    let (n, d) = if excess > 0 {
        ((num >> excess) + 1, den >> excess)
    } else {
        (num, den)
    };
    // n <= 2^(126-SCALE) + 1, so n << SCALE < 2^127 and `+ d - 1` fits.
    let q = ((n << SCALE) + d - 1) / d;
    Fixed((q as i128).min(ONE.0))
}
