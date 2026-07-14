//! Weighted-pool swap math (Balancer-style) built on [`crate::pow`].
//!
//! Balances, amounts, and weights are raw `u128` — the LEZ native token
//! unit. `Fixed` is used only for the base ratio and the weight-ratio
//! exponent (ADR 0002).
//!
//! Every rounding favours the pool: base up, exponent down, `1 - power`
//! padded down, final payout multiply floored — the payout never exceeds
//! the true value. `calc_in_given_out` is inverted into the same
//! `base ∈ (0,1)` domain (ADR 0006).
//!
//! Envelope (asserted; violations panic, never wrap): reserves >= 1 wei,
//! total deposit < 2^128, weights in `[1, 2^64]` with ratio in
//! `[1/99, 99]`. The per-step overflow ledger is `docs/overflow-proof.md`.

use crate::fixed::{Fixed, ONE, SCALE};
use crate::{pow, wide};

// The 1/99 weight ratio must stay a nonzero exponent: 2^SCALE >= 99.
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
///
/// ```
/// use weighted_math_core::weighted::calc_out_given_in;
///
/// // Sell 1000 wei of token A into a 99/1 pool.
/// let out = calc_out_given_in(
///     1_000_000_000, // balance_in  (raw u128 wei)
///     99,            // weight_in
///     500_000_000,   // balance_out
///     1,             // weight_out
///     1_000,         // amount_in
/// );
/// assert!(out > 0 && out < 500_000_000);
/// ```
///
/// # Panics
///
/// On envelope violations: empty reserve, total deposit past `u128`, or
/// weights outside `[1, 2^64]` / ratio outside `[1/99, 99]`.
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
/// Rounded up: the pool never undercharges. The base-above-one form is
/// inverted into the kernel's `(0,1)` domain via
/// `(b/(b-a))^y - 1 = (1-p)/p`, flipping every rounding direction once
/// (ADR 0006).
///
/// ```
/// use weighted_math_core::weighted::calc_in_given_out;
///
/// // Price an exact-out purchase of 1000 wei of token B.
/// let amount_in = calc_in_given_out(
///     1_000_000_000, // balance_in
///     99,            // weight_in
///     500_000_000,   // balance_out
///     1,             // weight_out
///     1_000,         // amount_out
/// );
/// assert!(amount_in > 0);
/// ```
///
/// # Panics
///
/// As [`calc_out_given_in`], plus: `amount_out` above 30% of the reserve,
/// or a payment past `u128` (ADR 0006).
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

    // ceil(balance_in · r62 / 2^62); the fit assert is the payment envelope.
    wide::mul_shr_up(balance_in, r62, pow::LN_SCALE)
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

/// `num / den` as a `Fixed`, rounded up, for `num <= den` — one division.
///
/// Wide operands are pre-shifted down, biased so the result can only
/// round up (cost `< 2^-73` relative).
fn ratio_up(num: u128, den: u128) -> Fixed {
    debug_assert!(num <= den && den > 0);
    let den_bits = 128 - den.leading_zeros();
    let excess = den_bits.saturating_sub(126 - SCALE);
    let (n, d) = if excess > 0 {
        ((num >> excess) + 1, den >> excess)
    } else {
        (num, den)
    };
    // n <= 2^(126-SCALE), so n << SCALE <= 2^126 and `+ d - 1` fits.
    let q = ((n << SCALE) + d - 1) / d;
    debug_assert!(q >> 127 == 0, "ratio fits the signed representation");
    Fixed(q as i128)
}

/// `num / den` as a `Fixed`, rounded down, for `num < den` — the mirror
/// of [`ratio_up`].
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
