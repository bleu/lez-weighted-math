//! Weighted-pool swap math (Balancer-style) built on top of [`crate::pow`].
//!
//! Wrappers over the core `pow` kernel that express the constant-value-invariant
//! swap formulae. Signatures only for now; see ADR 0001 for the open questions
//! about weight representation and rounding conventions.

use crate::fixed::Fixed;

/// Amount of `token_out` received for a given `amount_in` of `token_in`.
///
/// `out = balance_out * (1 - (balance_in / (balance_in + amount_in))^(weight_in / weight_out))`
pub fn calc_out_given_in(
    _balance_in: Fixed,
    _weight_in: Fixed,
    _balance_out: Fixed,
    _weight_out: Fixed,
    _amount_in: Fixed,
) -> Fixed {
    todo!()
}

/// Amount of `token_in` required to receive a given `amount_out` of `token_out`.
///
/// `in = balance_in * ((balance_out / (balance_out - amount_out))^(weight_out / weight_in) - 1)`
pub fn calc_in_given_out(
    _balance_in: Fixed,
    _weight_in: Fixed,
    _balance_out: Fixed,
    _weight_out: Fixed,
    _amount_out: Fixed,
) -> Fixed {
    todo!()
}

/// Spot price of `token_in` in terms of `token_out` (ignoring swap fees).
pub fn spot_price(
    _balance_in: Fixed,
    _weight_in: Fixed,
    _balance_out: Fixed,
    _weight_out: Fixed,
) -> Fixed {
    todo!()
}
