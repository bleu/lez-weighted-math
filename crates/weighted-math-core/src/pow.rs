//! Fixed-point power function `pow(base, exponent)`.
//!
//! This is the kernel the whole crate exists for. The intended implementation
//! is `pow(base, y) = exp(y · ln base)` with `base ∈ (0,1)` and `y > 0`, so the
//! `exp` argument is always ≤ 0 and the result lands in (0,1]. Range-reduce by
//! `ln2` with shifts (not divisions), then a series on the `[0, 0.693]`
//! remainder. Series choice, term count, and error bounds are open decisions —
//! see ADR 0001. For now everything is a signature plus `todo!()`.

use crate::fixed::Fixed;

/// `base^exponent`, both fixed-point. Rounding direction unspecified until we
/// pin the error model; use `pow_down` / `pow_up` when the direction matters.
pub fn pow(_base: Fixed, _exponent: Fixed) -> Fixed {
    todo!()
}

/// `base^exponent` rounded down (never overstates the result).
pub fn pow_down(_base: Fixed, _exponent: Fixed) -> Fixed {
    todo!()
}

/// `base^exponent` rounded up (never understates the result).
pub fn pow_up(_base: Fixed, _exponent: Fixed) -> Fixed {
    todo!()
}

/// Natural logarithm of a fixed-point value.
pub fn ln(_x: Fixed) -> Fixed {
    todo!()
}

/// Natural exponential of a fixed-point value.
pub fn exp(_x: Fixed) -> Fixed {
    todo!()
}

/// `exp(x) - 1`, accurate for `x` near zero.
///
/// Used to compute `1 − pow(base, y)` as `−expm1(y · ln base)` near the sale
/// start, where `power ≈ 1` and the direct subtraction loses precision to
/// catastrophic cancellation. See ADR 0001, decision 4.
pub fn expm1(_x: Fixed) -> Fixed {
    todo!()
}
