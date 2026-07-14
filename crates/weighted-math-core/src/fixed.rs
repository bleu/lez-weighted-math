//! Fixed-point scalar type.
//!
//! Binary Q-format: a value `x` represents `x / 2^SCALE`. Multiplying or
//! dividing by `ONE` is a shift, so `mul_*` cost no divisions and `div_*`
//! cost one each.
//!
//! `Repr` is signed because `ln` results are negative. The wrappers below
//! are defined on nonnegative operands only (asserted); balances stay raw
//! `u128` (ADR 0003).

use crate::wide;

/// Binary scale: the fixed-point value `x` represents `x / 2^SCALE`.
///
/// Sweepable against the oracle with no fixture regeneration (ADR 0002);
/// 52 was chosen by the sweep (ADR 0004).
pub const SCALE: u32 = 52;

/// Backing integer representation for a fixed-point value.
pub type Repr = i128;

/// A binary fixed-point number scaled by `2^SCALE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Fixed(pub Repr);

/// The unit value (`1.0`) at the current `SCALE`.
pub const ONE: Fixed = Fixed(1_i128 << SCALE);

impl Fixed {
    /// `floor(self · rhs / 2^SCALE)`. The product is formed at 256 bits;
    /// a result past `Repr` panics, never wraps.
    pub fn mul_down(self, rhs: Fixed) -> Fixed {
        Fixed(checked_repr(wide::mul_shr(
            nonneg(self),
            nonneg(rhs),
            SCALE,
        )))
    }

    /// `ceil(self · rhs / 2^SCALE)`.
    pub fn mul_up(self, rhs: Fixed) -> Fixed {
        Fixed(checked_repr(wide::mul_shr_up(
            nonneg(self),
            nonneg(rhs),
            SCALE,
        )))
    }

    /// `floor(self · 2^SCALE / rhs)` — one division. The numerator must
    /// fit `u128` (asserted).
    pub fn div_down(self, rhs: Fixed) -> Fixed {
        Fixed(checked_repr(numerator(self, rhs) / nonneg(rhs)))
    }

    /// `ceil(self · 2^SCALE / rhs)`.
    pub fn div_up(self, rhs: Fixed) -> Fixed {
        let d = nonneg(rhs);
        let n = numerator(self, rhs);
        // n < 2^127 and d < 2^127 (both come from Repr), so no overflow.
        Fixed(checked_repr((n + d - 1) / d))
    }

    /// `ONE - self`, saturating at zero. Exact integer subtraction.
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
