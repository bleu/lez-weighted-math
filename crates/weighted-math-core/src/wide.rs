//! Widening 128-bit multiply primitives.
//!
//! On the RV32IM target a `u128` is already multi-limb, so a full
//! `128x128 -> 256` product is just a handful of word multiplies — not a
//! special regime. What the kernel *never* does is a 256-bit division
//! (division is the most expensive zkVM primitive); these helpers only
//! multiply, add, and shift.

/// Full `128x128 -> 256`-bit product as `(hi, lo)`.
pub(crate) const fn mul_wide(a: u128, b: u128) -> (u128, u128) {
    const MASK: u128 = u64::MAX as u128;
    let (a_hi, a_lo) = (a >> 64, a & MASK);
    let (b_hi, b_lo) = (b >> 64, b & MASK);
    // Each partial product is at most (2^64 - 1)^2 < 2^128: no overflow.
    let ll = a_lo * b_lo;
    let lh = a_lo * b_hi;
    let hl = a_hi * b_lo;
    let hh = a_hi * b_hi;
    let (mid, carry_mid) = lh.overflowing_add(hl);
    let (lo, carry_lo) = ll.overflowing_add(mid << 64);
    let hi = hh + (mid >> 64) + ((carry_mid as u128) << 64) + carry_lo as u128;
    (hi, lo)
}

/// `floor(a * b / 2^shift)` for `0 < shift < 128`.
///
/// Panics if the shifted product does not fit `u128`; every call site must
/// argue it fits (the overflow proof lives at those call sites).
pub(crate) fn mul_shr(a: u128, b: u128, shift: u32) -> u128 {
    debug_assert!(shift > 0 && shift < 128);
    let (hi, lo) = mul_wide(a, b);
    assert!(hi >> shift == 0, "widened product exceeds u128 after shift");
    (hi << (128 - shift)) | (lo >> shift)
}

/// `ceil(a * b / 2^shift)` for `0 < shift < 128`, same fit requirement.
pub(crate) fn mul_shr_up(a: u128, b: u128, shift: u32) -> u128 {
    debug_assert!(shift > 0 && shift < 128);
    let (hi, lo) = mul_wide(a, b);
    assert!(hi >> shift == 0, "widened product exceeds u128 after shift");
    let floor = (hi << (128 - shift)) | (lo >> shift);
    floor + (lo & ((1u128 << shift) - 1) != 0) as u128
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_native_when_in_range() {
        assert_eq!(mul_wide(0, u128::MAX), (0, 0));
        assert_eq!(mul_wide(3, 5), (0, 15));
        let a = 0x1234_5678_9abc_def0_u128;
        let b = 0xfedc_ba98_7654_3210_u128;
        assert_eq!(mul_wide(a, b), (0, a * b));
    }

    #[test]
    fn full_width_corner() {
        // (2^128 - 1)^2 = 2^256 - 2^129 + 1
        assert_eq!(mul_wide(u128::MAX, u128::MAX), (u128::MAX - 1, 1));
    }

    #[test]
    fn shifted_rounding() {
        // 7 * 5 / 8 = 4.375
        assert_eq!(mul_shr(7, 5, 3), 4);
        assert_eq!(mul_shr_up(7, 5, 3), 5);
        // exact multiples round identically
        assert_eq!(mul_shr(6, 4, 3), 3);
        assert_eq!(mul_shr_up(6, 4, 3), 3);
        // a genuinely 256-bit intermediate
        assert_eq!(mul_shr(u128::MAX, 1 << 62, 62), u128::MAX);
    }
}
