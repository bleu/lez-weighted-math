//! Fixed-point scalar type.
//!
//! Binary fixed-point (Q-format): a value `x` represents `x / 2^SCALE`.
//! `SCALE` is a plain const so it can be *swept* against the mpmath oracle
//! rather than hand-picked; `ONE` is the unit value at the chosen scale.
//! Binary scaling keeps the hot path on shifts — multiplying or dividing by
//! `ONE` is free, so `mul_down`/`mul_up` cost zero divisions and
//! `div_down`/`div_up` cost exactly one hardware division each.
//!
//! `Repr` is signed (`i128`): `ln x` for `x ∈ (0,1)` is negative and the
//! `exp` argument is `<= 0`. The directional wrappers below serve the pool
//! math and are defined on **nonnegative** operands only (asserted): in the
//! kernel, `Fixed` carries ratios, weights-ratios, and powers — balances
//! stay raw `u128` (ADR 0003).

use crate::wide;

/// Binary scale: the fixed-point value `x` represents `x / 2^SCALE`.
///
/// Sweepable: change this const to trade precision against cycle cost. The
/// harness re-grades at the new scale with no fixture regeneration
/// (ADR 0001 decision 2, ADR 0002).
pub const SCALE: u32 = 52;

/// Backing integer representation for a fixed-point value.
pub type Repr = i128;

/// A binary fixed-point number scaled by `2^SCALE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Fixed(pub Repr);

/// The unit value (`1.0`) at the current `SCALE`.
pub const ONE: Fixed = Fixed(1_i128 << SCALE);

impl Fixed {
    /// Fixed-point multiplication rounded down: `floor(self · rhs / 2^SCALE)`.
    ///
    /// The product is formed at 256 bits, so the only overflow surface is
    /// the *result* exceeding `Repr` — which panics (envelope violation)
    /// rather than wrapping.
    pub fn mul_down(self, rhs: Fixed) -> Fixed {
        Fixed(checked_repr(wide::mul_shr(
            nonneg(self),
            nonneg(rhs),
            SCALE,
        )))
    }

    /// Fixed-point multiplication rounded up: `ceil(self · rhs / 2^SCALE)`.
    pub fn mul_up(self, rhs: Fixed) -> Fixed {
        Fixed(checked_repr(wide::mul_shr_up(
            nonneg(self),
            nonneg(rhs),
            SCALE,
        )))
    }

    /// Fixed-point division rounded down: `floor(self · 2^SCALE / rhs)`.
    ///
    /// One hardware division. The numerator `self << SCALE` must fit u128,
    /// bounding `self < 2^(128 - 2·SCALE)` in value — ample for the
    /// kernel's ratio/weight domain (asserted, not wrapped).
    pub fn div_down(self, rhs: Fixed) -> Fixed {
        Fixed(checked_repr(numerator(self, rhs) / nonneg(rhs)))
    }

    /// Fixed-point division rounded up: `ceil(self · 2^SCALE / rhs)`.
    pub fn div_up(self, rhs: Fixed) -> Fixed {
        let d = nonneg(rhs);
        let n = numerator(self, rhs);
        // n < 2^127 and d < 2^127 (both come from Repr), so no overflow.
        Fixed(checked_repr((n + d - 1) / d))
    }

    /// The complement `ONE - self`, saturating at zero for `self > ONE`.
    /// Exact: pure integer subtraction, no rounding direction to choose.
    pub fn complement(self) -> Fixed {
        Fixed((ONE.0 - nonneg(self) as i128).max(0))
    }
}

fn nonneg(x: Fixed) -> u128 {
    assert!(
        x.0 >= 0,
        "fixed-point wrappers are defined on nonnegative values"
    );
    x.0 as u128
}

fn checked_repr(v: u128) -> Repr {
    assert!(
        v <= Repr::MAX as u128,
        "fixed-point result exceeds representation"
    );
    v as Repr
}

fn numerator(a: Fixed, b: Fixed) -> u128 {
    let a = nonneg(a);
    assert!(nonneg(b) > 0, "fixed-point division by zero");
    // Keep the numerator under 2^127 so div_up's `+ d - 1` cannot overflow.
    assert!(
        a >> (127 - SCALE) == 0,
        "fixed-point division numerator overflow"
    );
    a << SCALE
}
