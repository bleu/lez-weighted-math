//! Fixed-point power kernel: `pow(base, y) = exp(y · ln base)`.
//!
//! Domain: `base ∈ (0, 1]`, `y ∈ (0, 99]`. The `exp` argument is always
//! `<= 0` and every result lands in `(0, 1]`, so no large-argument `exp`
//! code is needed.
//!
//! The pipeline runs at [`LN_SCALE`] = 62 bits and rounds once at the end;
//! the guard bits absorb the error growth `y · δ(ln x)` and keep
//! `1 - power` accurate near the sale start. One `pow` costs exactly one
//! division (forming `t` inside `ln`) — everything else is shifts,
//! multiplies, and precomputed constants.
//!
//! Total pipeline error is under half an ulp at any `SCALE <= 56`. The
//! accounting is in `docs/error-analysis.md`; the overflow bounds are in
//! `docs/overflow-proof.md`.

use crate::fixed::{Fixed, ONE, SCALE};
use crate::wide;

/// Internal working scale for the ln/exp pipeline.
pub(crate) const LN_SCALE: u32 = 62;
const ONE_62: i128 = 1 << LN_SCALE;

// The shift arithmetic below needs at least two guard bits over SCALE.
#[allow(clippy::assertions_on_constants)]
const _: () = assert!(SCALE <= 60, "SCALE needs >= 2 guard bits under LN_SCALE");

/// round(ln(2) · 2^62)
const LN2_62: i128 = 3_196_577_161_300_663_915;
/// round(2^30 / ln(2))
const INV_LN2_30: u128 = 1_549_082_005;
/// round(2^62 / sqrt(2))
const SQRT2_HALF_62: i128 = 3_260_954_456_333_195_553;

/// Padding applied by `pow_up`/`pow_down`, in ulps at `SCALE`. Covers the
/// pipeline error plus the final rounding (docs/error-analysis.md).
const POW_PAD_ULPS: i128 = 2;

/// atanh series constants: round(2^62 / (2i+1)) for i = 0..=12.
/// ln(m) = 2t·(1 + t²/3 + t⁴/5 + …) with t = (m-1)/(m+1), |t| <= 0.1716.
const ATANH_COEFF_62: [i128; 13] = [
    4_611_686_018_427_387_904,
    1_537_228_672_809_129_301,
    922_337_203_685_477_581,
    658_812_288_346_769_701,
    512_409_557_603_043_100,
    419_244_183_493_398_900,
    354_745_078_340_568_300,
    307_445_734_561_825_860,
    271_275_648_142_787_524,
    242_720_316_759_336_205,
    219_604_096_115_589_900,
    200_508_087_757_712_518,
    184_467_440_737_095_516,
];

/// exp Taylor constants: (-1)^i · round(2^62 / i!) for i = 0..=19, so the
/// Horner loop evaluates exp(-s) directly from s >= 0.
const EXP_COEFF_62: [i128; 20] = [
    4_611_686_018_427_387_904,
    -4_611_686_018_427_387_904,
    2_305_843_009_213_693_952,
    -768_614_336_404_564_651,
    192_153_584_101_141_163,
    -38_430_716_820_228_233,
    6_405_119_470_038_039,
    -915_017_067_148_291,
    114_377_133_393_536,
    -12_708_570_377_060,
    1_270_857_037_706,
    -115_532_457_973,
    9_627_704_831,
    -740_592_679,
    52_899_477,
    -3_526_632,
    220_414,
    -12_966,
    720,
    -38,
];

/// `-ln(x)` at `LN_SCALE`, for `x` at `SCALE` in `(0, ONE]`. Result `>= 0`.
///
/// Shift-normalize into `m ∈ [√2/2, √2)`, one division for
/// `t = (m-1)/(m+1)`, then the odd atanh series:
/// `-ln x = k·ln2 - 2t·P(t²)`.
pub(crate) fn ln_inner(x: i128) -> i128 {
    debug_assert!(x > 0 && x <= ONE.0);
    let mut m = x << (LN_SCALE - SCALE);
    let mut k: i128 = 0;
    while m < SQRT2_HALF_62 {
        m <<= 1;
        k += 1;
    }
    debug_assert!(k <= SCALE as i128);
    debug_assert!((SQRT2_HALF_62..2 * SQRT2_HALF_62).contains(&m));
    let t = ((m - ONE_62) << LN_SCALE) / (m + ONE_62);
    let u = (t * t) >> LN_SCALE;
    let mut p = ATANH_COEFF_62[ATANH_COEFF_62.len() - 1];
    for i in (0..ATANH_COEFF_62.len() - 1).rev() {
        p = ATANH_COEFF_62[i] + ((p * u) >> LN_SCALE);
    }
    let ln_m = (t * p) >> (LN_SCALE - 1); // 2·t·P(u)
    k * LN2_62 - ln_m
}

/// `exp(-neg_arg)` at `LN_SCALE`, for `neg_arg >= 0` at `LN_SCALE`.
///
/// Range-reduce by `ln2` with one multiply by `1/ln2` (no division), then
/// the alternating Taylor series on the remainder `s ∈ [0, ln2)`.
/// `k > 62` underflows the 62-bit grid and returns 0.
pub(crate) fn exp_inner(neg_arg: i128) -> i128 {
    // Caller envelope, proven in docs/overflow-proof.md.
    debug_assert!((0..1i128 << 76).contains(&neg_arg));
    let mut k = ((neg_arg as u128 * INV_LN2_30) >> (30 + LN_SCALE)) as i128;
    let mut s = neg_arg - k * LN2_62;
    while s < 0 {
        k -= 1;
        s += LN2_62;
    }
    while s >= LN2_62 {
        k += 1;
        s -= LN2_62;
    }
    if k > LN_SCALE as i128 {
        return 0;
    }
    let mut acc = EXP_COEFF_62[EXP_COEFF_62.len() - 1];
    for i in (0..EXP_COEFF_62.len() - 1).rev() {
        acc = EXP_COEFF_62[i] + ((acc * s) >> LN_SCALE);
    }
    acc >> k
}

/// Rounds a nonnegative `LN_SCALE` value to `SCALE`, nearest.
fn to_scale_nearest(v62: i128) -> i128 {
    (v62 + (1 << (LN_SCALE - SCALE - 1))) >> (LN_SCALE - SCALE)
}

/// Natural logarithm of `x ∈ (0, 1]`; result `<= 0`, nearest-rounded.
///
/// # Panics
///
/// If `x` is outside `(0, 1]`.
pub fn ln(x: Fixed) -> Fixed {
    assert!(x.0 > 0 && x.0 <= ONE.0, "ln domain: (0, 1]");
    Fixed(-to_scale_nearest(ln_inner(x.0)))
}

/// `exp`/`expm1` saturate at `x <= -64`, where the true `exp(x) < 2^-92`.
/// Compared before negation (so `i128::MIN` is safe); also caps
/// `exp_inner`'s argument at `2^68`, inside its proven envelope.
const EXP_SATURATION: i128 = 64 << SCALE;

/// Natural exponential of `x <= 0`; result in `[0, 1]`, nearest-rounded
/// (zero means the true value underflows the `2^-SCALE` grid).
///
/// # Panics
///
/// If `x > 0`.
pub fn exp(x: Fixed) -> Fixed {
    assert!(x.0 <= 0, "exp domain: x <= 0");
    if x.0 <= -EXP_SATURATION {
        return Fixed(0);
    }
    Fixed(to_scale_nearest(exp_inner((-x.0) << (LN_SCALE - SCALE))))
}

/// `exp(x) - 1` for `x <= 0`, accurate near zero; result in `[-1, 0]`.
///
/// Computed as the exact 62-bit complement `1 - exp_inner(-x)`, so there
/// is no cancellation (ADR 0005).
///
/// # Panics
///
/// If `x > 0`.
pub fn expm1(x: Fixed) -> Fixed {
    assert!(x.0 <= 0, "expm1 domain: x <= 0");
    if x.0 <= -EXP_SATURATION {
        return Fixed(-ONE.0);
    }
    let omp62 = ONE_62 - exp_inner((-x.0) << (LN_SCALE - SCALE));
    Fixed(-to_scale_nearest(omp62))
}

/// Pool-favouring pad for the 62-bit swap-path results: `2^-53`, twice
/// the pipeline error bound (docs/error-analysis.md).
const PAD_62: i128 = 512;

/// `exp(exponent · ln base)` at `LN_SCALE`, unpadded — shared core of the
/// 62-bit swap paths.
fn pow_62(base: Fixed, exponent: Fixed) -> i128 {
    assert!(base.0 > 0 && base.0 < ONE.0, "domain: base in (0, 1)");
    assert!(
        exponent.0 > 0 && exponent.0 <= 99 * ONE.0,
        "domain: exponent in (0, 99]"
    );
    let neg_ln = ln_inner(base.0);
    let neg_arg = wide::mul_shr(exponent.0 as u128, neg_ln as u128, SCALE) as i128;
    exp_inner(neg_arg)
}

/// `1 - base^exponent` at `LN_SCALE`, padded down (pool-favouring), for
/// the payout path. The subtraction is exact and the guard bits carry
/// values as small as `2^-46` into the final widened multiply — this is
/// what keeps sale-start payouts accurate (ADR 0005).
pub(crate) fn one_minus_pow_62(base: Fixed, exponent: Fixed) -> i128 {
    (ONE_62 - pow_62(base, exponent) - PAD_62).max(0)
}

/// `base^exponent` at `LN_SCALE`, padded down, for the exact-out payment
/// path: an understated power overstates the payment.
pub(crate) fn pow_62_down(base: Fixed, exponent: Fixed) -> i128 {
    (pow_62(base, exponent) - PAD_62).max(0)
}

/// `exp(exponent · ln base)`, nearest-rounded to `SCALE`.
fn pow_raw(base: Fixed, exponent: Fixed) -> i128 {
    assert!(base.0 > 0 && base.0 <= ONE.0, "pow domain: base in (0, 1]");
    assert!(
        exponent.0 > 0 && exponent.0 <= 99 * ONE.0,
        "pow domain: exponent in (0, 99]"
    );
    let neg_ln = ln_inner(base.0);
    // The product can pass 2^128 at swept scales, hence the widening.
    let neg_arg = wide::mul_shr(exponent.0 as u128, neg_ln as u128, SCALE) as i128;
    to_scale_nearest(exp_inner(neg_arg))
}

/// `base^exponent`, both fixed-point. Error within [`POW_PAD_ULPS`] either
/// side; use `pow_down` / `pow_up` when the direction matters.
///
/// # Panics
///
/// If `base` is outside `(0, 1]` or `exponent` outside `(0, 99]` (also the
/// panic condition of `pow_down` and `pow_up`).
pub fn pow(base: Fixed, exponent: Fixed) -> Fixed {
    Fixed(pow_raw(base, exponent))
}

/// `base^exponent` rounded down (never overstates the result).
pub fn pow_down(base: Fixed, exponent: Fixed) -> Fixed {
    Fixed((pow_raw(base, exponent) - POW_PAD_ULPS).max(0))
}

/// `base^exponent` rounded up (never understates the result). Never zero:
/// the true result is positive, so the padded floor is at least one ulp.
pub fn pow_up(base: Fixed, exponent: Fixed) -> Fixed {
    Fixed((pow_raw(base, exponent) + POW_PAD_ULPS).min(ONE.0))
}
