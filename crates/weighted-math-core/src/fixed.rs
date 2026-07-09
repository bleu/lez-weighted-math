//! Fixed-point scalar type.
//!
//! Binary fixed-point (Q-format): a value `x` represents `x / 2^SCALE`. `SCALE`
//! is a plain const so it can be *swept* against the mpmath oracle rather than
//! hand-picked (the brief's starting point is ~52); `ONE` is the unit value at
//! the chosen scale. Binary scaling keeps the hot path on shifts, since
//! division is the most expensive RISC0 zkVM primitive.
//!
//! `Repr` is signed (`i128`): `ln x` for `x ∈ (0,1)` is negative and the `exp`
//! argument is ≤ 0. The representation width is a placeholder — the outer
//! `balanceOut·(1−power)` multiply is expected to need one localized
//! 128×128→256 widened intermediate. See ADR 0001.

/// Binary scale: the fixed-point value `x` represents `x / 2^SCALE`.
///
/// Sweepable: change this const to trade precision against cycle cost. Confirm
/// against the mpmath oracle before locking a value (ADR 0001, decision 2).
pub const SCALE: u32 = 52;

/// Backing integer representation for a fixed-point value.
pub type Repr = i128;

/// A binary fixed-point number scaled by `2^SCALE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Fixed(pub Repr);

/// The unit value (`1.0`) at the current `SCALE`.
pub const ONE: Fixed = Fixed(1_i128 << SCALE);

impl Fixed {
    /// Fixed-point multiplication rounding toward zero: `(self * rhs) / ONE`.
    pub fn mul_down(self, _rhs: Fixed) -> Fixed {
        todo!()
    }

    /// Fixed-point multiplication rounding away from zero.
    pub fn mul_up(self, _rhs: Fixed) -> Fixed {
        todo!()
    }

    /// Fixed-point division rounding toward zero: `(self * ONE) / rhs`.
    pub fn div_down(self, _rhs: Fixed) -> Fixed {
        todo!()
    }

    /// Fixed-point division rounding away from zero.
    pub fn div_up(self, _rhs: Fixed) -> Fixed {
        todo!()
    }

    /// The complement `ONE - self`, saturating at zero.
    pub fn complement(self) -> Fixed {
        todo!()
    }
}
