//! Weighted-pool swap math (Balancer-style) built on top of [`crate::pow`].
//!
//! Balances, amounts, and weights cross this API as **raw `u128`** — the LEZ
//! native token unit (no decimals field). `Fixed` appears only where the math
//! needs a fractional value in a bounded range: the ratio
//! `base = balance_in / (balance_in + amount_in)` and the exponent
//! `weight_in / weight_out`. See ADR 0003.
//!
//! Rounding always favours the pool: `calc_out_given_in` returns the floored
//! payout in wei.
//!
//! The scaffold's `calc_in_given_out` and `spot_price` are gone: both need
//! `pow` with base > 1, which this kernel deliberately does not support
//! (`base ∈ (0,1)` is what deletes the large-argument `exp` machinery,
//! see `CONTEXT.md`). They come back only with their own design work.

/// Amount of `token_out` received for a given `amount_in` of `token_in`.
///
/// `out = balance_out * (1 - (balance_in / (balance_in + amount_in))^(weight_in / weight_out))`
///
/// All quantities are raw `u128` wei / raw weight units. The result is rounded
/// down (pool-favouring).
pub fn calc_out_given_in(
    _balance_in: u128,
    _weight_in: u128,
    _balance_out: u128,
    _weight_out: u128,
    _amount_in: u128,
) -> u128 {
    todo!()
}
