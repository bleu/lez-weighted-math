//! Fixed-point power kernel: `pow(base, y) = exp(y · ln base)`.
//!
//! Domain: `base ∈ (0, 1]`, `y ∈ (0, 99]`, so the `exp` argument is always
//! `<= 0` and every result lands in `(0, 1]` — Balancer's large-argument
//! `exp` machinery is deleted by construction.
//!
//! # Internal precision
//!
//! The public type carries `SCALE` fractional bits, but the whole pipeline
//! runs at [`LN_SCALE`] = 62 bits and rounds once at the end. The extra
//! guard bits are what absorb the error amplification `y · δ(ln x)` (worst
//! case `y = 99`) and keep `1 - power` accurate near the sale start.
//!
//! # Division count
//!
//! Division is the most expensive RISC0 primitive. One `pow` costs exactly
//! ONE division (forming `t` inside `ln`): range reduction multiplies by
//! `1/ln2`, both series run on precomputed reciprocal constants via Horner,
//! and all scale conversions are shifts.
//!
//! # Error bound (written analysis; validated by the harness sweep)
//!
//! At `LN_SCALE`, with `ulp = 2^-62`:
//! - `ln`: `t` carries a half-ulp from its division; the Horner loop adds
//!   `<= 13` half-ulp roundings damped by `u <= 0.03`; reconstruction via
//!   `k·ln2` adds `k` half-ulps of the constant. Total `|δ(ln x)| <~ 4 ulp`
//!   plus `k · 2^-63`.
//! - the argument product amplifies by `y`, but only bounded products
//!   matter: results below `2^-SCALE` underflow to the padded floor, so
//!   the reachable worst case is `y·k·ln2 <= ~44`, giving
//!   `y·|δ(ln x)| + |δ(arg)| <= ~2^-54`.
//! - `exp`: truncation after 20 alternating terms is `< 2^-66`; 19 Horner
//!   roundings add `< 20 ulp`; `exp(x) <= 1` keeps downstream error at most
//!   the argument error. Total pipeline error `< ~2^-53.5`, i.e. well under
//!   half an ulp at any `SCALE <= 56`.
//!
//! [`POW_PAD_ULPS`] converts that analysis into pool-safe directional
//! rounding: `pow_up = raw + pad`, `pow_down = raw - pad`. The harness
//! differential gate measures the real margins case by case.

use crate::fixed::{Fixed, ONE, SCALE};
use crate::wide;

/// Internal working scale for the ln/exp pipeline.
pub(crate) const LN_SCALE: u32 = 62;
const ONE_62: i128 = 1 << LN_SCALE;

// The pipeline needs guard bits over the public scale, and the fixed shift
// arithmetic below assumes at least two of them.
// (a compile-time guard for the sweep, not a runtime assertion)
#[allow(clippy::assertions_on_constants)]
const _: () = assert!(SCALE <= 60, "SCALE needs >= 2 guard bits under LN_SCALE");

/// round(ln(2) · 2^62)
const LN2_62: i128 = 3_196_577_161_300_663_915;
/// round(2^30 / ln(2))
const INV_LN2_30: u128 = 1_549_082_005;
/// round(2^62 / sqrt(2))
const SQRT2_HALF_62: i128 = 3_260_954_456_333_195_553;

/// Directional padding applied by `pow_up`/`pow_down`, in ulps at `SCALE`.
/// Dominates the pipeline error (see the module analysis: true error is
/// under half an ulp, and nearest-rounding to `SCALE` adds another half).
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
/// Range-reduces by powers of two (shifts) into `m ∈ [√2/2, √2)`, then one
/// division forms `t = (m-1)/(m+1)` and the odd atanh series does the rest:
/// `-ln x = k·ln2 - 2t·P(t²)`.
///
/// Overflow: `m ∈ [0.707, 1.414)·2^62`, so `|m - 2^62| <= 0.414·2^62` and
/// the `<< 62` numerator stays below `2^125`; `|t| <= 0.1716·2^62` keeps
/// every Horner product below `2^122`; `k <= SCALE` bounds `k·ln2 < 2^68`.
pub(crate) fn ln_inner(x: i128) -> i128 {
    debug_assert!(x > 0 && x <= ONE.0);
    let mut m = x << (LN_SCALE - SCALE);
    let mut k: i128 = 0;
    while m < SQRT2_HALF_62 {
        m <<= 1;
        k += 1;
    }
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
/// Range reduction is a multiply by `1/ln2` (no division): `k` is found from
/// the product's high bits, then corrected by at most one step so that
/// `s = neg_arg - k·ln2 ∈ [0, ln2)`. The remainder runs through the
/// alternating Taylor series (Horner, constants only). `k > 62` underflows
/// the 62-bit grid and returns 0 — this also keeps the final shift in range.
pub(crate) fn exp_inner(neg_arg: i128) -> i128 {
    debug_assert!(neg_arg >= 0);
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
pub fn ln(x: Fixed) -> Fixed {
    assert!(x.0 > 0 && x.0 <= ONE.0, "ln domain: (0, 1]");
    Fixed(-to_scale_nearest(ln_inner(x.0)))
}

/// Natural exponential of `x <= 0`; result in `[0, 1]`, nearest-rounded
/// (zero means the true value underflows the `2^-SCALE` grid).
pub fn exp(x: Fixed) -> Fixed {
    assert!(x.0 <= 0, "exp domain: x <= 0");
    match (-x.0).checked_shl(LN_SCALE - SCALE) {
        Some(neg_arg) => Fixed(to_scale_nearest(exp_inner(neg_arg))),
        // |x| >= 2^117: deeper underflow than any representable result.
        None => Fixed(0),
    }
}

/// `exp(x) - 1` for `x <= 0`, accurate near zero; result in `[-1, 0]`.
///
/// At `LN_SCALE` this is the *exact* complement `1 - exp_inner(-x)`: fixed
/// point has absolute precision, so no cancellation occurs — the guard bits
/// carry the small result until the single final rounding.
pub fn expm1(x: Fixed) -> Fixed {
    assert!(x.0 <= 0, "expm1 domain: x <= 0");
    let omp62 = match (-x.0).checked_shl(LN_SCALE - SCALE) {
        Some(neg_arg) => ONE_62 - exp_inner(neg_arg),
        None => ONE_62,
    };
    Fixed(-to_scale_nearest(omp62))
}

/// Pool-favouring padding at `LN_SCALE` for the 62-bit swap-path results:
/// `2^-53`, twice the module error analysis's pipeline bound.
const PAD_62: i128 = 512;

/// `exp(exponent · ln base)` at `LN_SCALE`, unpadded — the shared core of
/// the 62-bit swap paths.
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

/// `1 - base^exponent` held at `LN_SCALE` and rounded DOWN (pool-favouring),
/// for the payout path. Never materializing `1 - power` at `SCALE` is what
/// preserves sale-start accuracy: near `power ~ 1` the true value can be as
/// small as `2^-46`, far below a `2^-SCALE` ulp's relative resolution, but
/// the guard bits carry it into the final widened payout multiply intact.
/// The subtraction itself is exact — fixed point has absolute precision —
/// so the only error is the pipeline's, dominated by [`PAD_62`].
pub(crate) fn one_minus_pow_62(base: Fixed, exponent: Fixed) -> i128 {
    (ONE_62 - pow_62(base, exponent) - PAD_62).max(0)
}

/// `base^exponent` held at `LN_SCALE` and rounded DOWN (pool-favouring),
/// for the exact-out payment path: an understated power overstates the
/// required payment `balance_in · (1 - p)/p`.
pub(crate) fn pow_62_down(base: Fixed, exponent: Fixed) -> i128 {
    (pow_62(base, exponent) - PAD_62).max(0)
}

/// The shared pow pipeline: `exp(exponent · ln base)` nearest-rounded to
/// `SCALE`, with the whole computation held at `LN_SCALE`.
fn pow_raw(base: Fixed, exponent: Fixed) -> i128 {
    assert!(base.0 > 0 && base.0 <= ONE.0, "pow domain: base in (0, 1]");
    assert!(
        exponent.0 > 0 && exponent.0 <= 99 * ONE.0,
        "pow domain: exponent in (0, 99]"
    );
    let neg_ln = ln_inner(base.0);
    // exponent · (-ln base): 99·2^SCALE times ~89·2^62 needs the widened
    // multiply; after >> SCALE the argument is < 2^76, comfortably i128.
    let neg_arg = wide::mul_shr(exponent.0 as u128, neg_ln as u128, SCALE) as i128;
    to_scale_nearest(exp_inner(neg_arg))
}

/// `base^exponent`, both fixed-point. Rounding direction unspecified (error
/// within [`POW_PAD_ULPS`] either side); use `pow_down` / `pow_up` when the
/// direction matters.
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
