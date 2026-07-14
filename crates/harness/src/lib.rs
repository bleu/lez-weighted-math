//! Test harness for `weighted-math-core`: fixture structs, the quantizer,
//! and the signed one-sided band checker. See ADR 0001.
//!
//! The grader is parametric over `(SCALE, BUDGET)`. `SCALE` comes from the
//! kernel crate (the sweepable knob); [`BUDGET_ULPS`] is the algorithmic
//! allowance on top of the one-ulp representation floor. Re-sweeping the
//! scale is a one-line change in the kernel; no fixture ever regenerates.

use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Deserialize;

pub use weighted_math_core::fixed::{Fixed, ONE, SCALE};

/// Algorithmic error allowance, in ulps at `SCALE`, on top of the one-ulp
/// quantization floor. The only knob besides `SCALE` itself.
pub const BUDGET_ULPS: u128 = 3;

/// The pass/fail line for the pow gate: representation floor + budget.
pub const fn bound_ulps() -> u128 {
    1 + BUDGET_ULPS
}

/// Fixture inputs are dyadic multiples of `2^-S40_BITS` (see oracle/gen.py),
/// so any `SCALE >= S40_BITS` represents them exactly.
pub const S40_BITS: u32 = 40;

// ---------------------------------------------------------------------------
// Fixture schema
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct PowCase {
    pub zone: String,
    pub base: String,
    pub base_s40: String,
    pub exponent: String,
    pub exponent_s40: String,
    pub pow_exact: String,
    pub pow_exact_q128: String,
}

#[derive(Debug, Deserialize)]
pub struct LnCase {
    pub x: String,
    pub x_s40: String,
    pub neg_ln_exact: String,
    pub neg_ln_q116: String,
}

#[derive(Debug, Deserialize)]
pub struct ExpCase {
    pub neg_x: String,
    pub neg_x_s40: String,
    pub exp_exact: String,
    pub exp_q128: String,
    pub neg_expm1_exact: String,
    pub neg_expm1_q128: String,
}

#[derive(Debug, Deserialize)]
pub struct LnExpFixture {
    pub ln: Vec<LnCase>,
    pub exp: Vec<ExpCase>,
}

#[derive(Debug, Deserialize)]
pub struct OutGivenInCase {
    pub zone: String,
    pub balance_in: String,
    pub amount_in: String,
    pub balance_out: String,
    pub weight_in: String,
    pub weight_out: String,
    pub base: String,
    pub exponent: String,
    pub power_exact: String,
    pub power_exact_q128: String,
    pub one_minus_power_exact: String,
    pub tokens_out_floor: String,
    pub sens_base_wei: String,
    pub sens_exp_wei: String,
}

#[derive(Debug, Deserialize)]
pub struct InGivenOutCase {
    pub zone: String,
    pub balance_in: String,
    pub amount_out: String,
    pub balance_out: String,
    pub weight_in: String,
    pub weight_out: String,
    pub base: String,
    pub exponent: String,
    pub power_exact: String,
    pub power_exact_q128: String,
    pub amount_in_ceil: String,
    pub sens_base_wei: String,
    pub sens_exp_wei: String,
    pub sens_pow_wei: String,
}

#[derive(Debug, Deserialize)]
pub struct ArithCase {
    pub a: String,
    pub a_s40: String,
    pub b: String,
    pub b_s40: String,
}

#[derive(Debug, Deserialize)]
pub struct ScaleCase {
    pub scale: u32,
    pub one: String,
    pub ulp_decimal: String,
    pub ulp_q128: String,
}

#[derive(Debug, Deserialize)]
pub struct BalancerInput {
    pub x18: String,
    pub y18: String,
    #[serde(default)]
    pub skip: bool,
    pub expected18_floor: Option<String>,
}

pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

pub fn load_fixture<T: DeserializeOwned>(name: &str) -> T {
    let path = fixtures_dir().join(name);
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// Quantizer: fixture strings -> integers at the compiled-in SCALE
// ---------------------------------------------------------------------------

pub fn parse_u128(s: &str) -> u128 {
    s.parse()
        .unwrap_or_else(|e| panic!("parsing u128 {s:?}: {e}"))
}

/// Parses a decimal integer, or `None` if it exceeds `u128`. Sensitivity
/// fields can exceed u128 on deep-drain cases; `None` skips the magnitude
/// gate there while the direction gate stays strict.
pub fn parse_u128_checked(s: &str) -> Option<u128> {
    s.parse().ok()
}

// Fixture inputs are exact only for scales at or above the dyadic grid.
#[allow(clippy::assertions_on_constants)]
const _: () = assert!(SCALE >= S40_BITS, "SCALE must be >= the fixture grid");

/// Exact `Fixed` from a `value * 2^S40_BITS` fixture field.
pub fn s40_to_fixed(s40: &str) -> Fixed {
    let raw: i128 = s40
        .parse()
        .unwrap_or_else(|e| panic!("parsing s40 {s40:?}: {e}"));
    Fixed(raw << (SCALE - S40_BITS))
}

/// The reference value's floor and ceiling on the `2^-SCALE` grid, from a
/// `floor(value * 2^qbits)` fixture field.
pub fn q_to_scale_bounds(q: &str, qbits: u32) -> (i128, i128) {
    assert!(qbits >= SCALE);
    let q = parse_u128(q);
    let shift = qbits - SCALE;
    let floor = (q >> shift) as i128;
    let ceil = floor + i128::from(q & ((1u128 << shift) - 1) != 0 || shift == 0);
    (floor, ceil)
}

// ---------------------------------------------------------------------------
// Band checker: signed, one-sided
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Result must never understate the truth (`_up` wrappers).
    Up,
    /// Result must never overstate the truth (`_down` wrappers, payouts).
    Down,
}

/// Outcome of a band check. `WrongSide` is the fund-leak category and fails
/// regardless of magnitude; `TooFar` is a precision shortfall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    Ok { err_ulps: u128 },
    WrongSide { by_ulps: u128 },
    TooFar { err_ulps: u128, bound_ulps: u128 },
}

impl Band {
    pub fn is_ok(self) -> bool {
        matches!(self, Band::Ok { .. })
    }
}

/// Checks `actual` against the truth's grid bounds `(t_floor, t_ceil)`.
///
/// `Down`: the largest grid value not exceeding the truth is `t_floor`, so
/// direction holds iff `actual <= t_floor`, and the error is measured from
/// `t_floor`. `Up` is the mirror image around `t_ceil`.
pub fn check_directional(
    t_floor: i128,
    t_ceil: i128,
    actual: i128,
    direction: Direction,
    bound_ulps: u128,
) -> Band {
    let signed_err = match direction {
        Direction::Down => t_floor - actual,
        Direction::Up => actual - t_ceil,
    };
    if signed_err < 0 {
        Band::WrongSide {
            by_ulps: signed_err.unsigned_abs(),
        }
    } else if signed_err as u128 > bound_ulps {
        Band::TooFar {
            err_ulps: signed_err as u128,
            bound_ulps,
        }
    } else {
        Band::Ok {
            err_ulps: signed_err as u128,
        }
    }
}

/// Two-sided diagnostic check (used for `ln`/`exp`/`expm1` and the Balancer
/// self-validation, which have no pool-favouring direction of their own).
pub fn check_two_sided(t_floor: i128, t_ceil: i128, actual: i128, bound_ulps: u128) -> Band {
    let err = if actual < t_floor {
        (t_floor - actual) as u128
    } else if actual > t_ceil {
        (actual - t_ceil) as u128
    } else {
        0
    };
    if err > bound_ulps {
        Band::TooFar {
            err_ulps: err,
            bound_ulps,
        }
    } else {
        Band::Ok { err_ulps: err }
    }
}

// ---------------------------------------------------------------------------
// Curve-invariant referee (ADR 0007)
// ---------------------------------------------------------------------------

/// Does the trade keep the curve value `b_in^w_in * b_out^w_out` from
/// decreasing? Exact big-integer comparison with no rounding of its own
/// and no code shared with the kernel: a `false` is a real fund leak
/// (ADR 0007). Weights must be integers in 1..=99 so they can be
/// exponents here.
pub fn invariant_preserved(
    balance_in: u128,
    weight_in: u128,
    balance_out: u128,
    weight_out: u128,
    amount_in: u128,
    amount_out: u128,
) -> bool {
    use num_bigint::BigUint;
    assert!(
        (1..=99).contains(&weight_in) && (1..=99).contains(&weight_out),
        "referee weights must be small integer exponents"
    );
    if amount_out > balance_out {
        return false;
    }
    let (w_in, w_out) = (weight_in as u32, weight_out as u32);
    let before = BigUint::from(balance_in).pow(w_in) * BigUint::from(balance_out).pow(w_out);
    let after = (BigUint::from(balance_in) + amount_in).pow(w_in)
        * BigUint::from(balance_out - amount_out).pow(w_out);
    after >= before
}

// ---------------------------------------------------------------------------
// Double-width helpers for the exact arithmetic-wrapper checks
// ---------------------------------------------------------------------------

/// Full 128x128 -> 256-bit product, as (hi, lo).
pub fn mul_wide(a: u128, b: u128) -> (u128, u128) {
    let (a_hi, a_lo) = (a >> 64, a & u64::MAX as u128);
    let (b_hi, b_lo) = (b >> 64, b & u64::MAX as u128);
    let ll = a_lo * b_lo;
    let lh = a_lo * b_hi;
    let hl = a_hi * b_lo;
    let hh = a_hi * b_hi;
    let (mid, carry1) = lh.overflowing_add(hl);
    let (lo, carry2) = ll.overflowing_add(mid << 64);
    let hi = hh + (mid >> 64) + ((carry1 as u128) << 64) + carry2 as u128;
    (hi, lo)
}

/// `(value as u256) << shift`, as (hi, lo). `shift < 128`.
pub fn shl_wide(value: u128, shift: u32) -> (u128, u128) {
    if shift == 0 {
        (0, value)
    } else {
        (value >> (128 - shift), value << shift)
    }
}

pub fn wide_le(a: (u128, u128), b: (u128, u128)) -> bool {
    a.0 < b.0 || (a.0 == b.0 && a.1 <= b.1)
}

pub fn wide_lt(a: (u128, u128), b: (u128, u128)) -> bool {
    a.0 < b.0 || (a.0 == b.0 && a.1 < b.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_checker_directions() {
        // truth strictly between grid points: floor 100, ceil 101
        assert!(check_directional(100, 101, 100, Direction::Down, 2).is_ok());
        assert_eq!(
            check_directional(100, 101, 101, Direction::Down, 2),
            Band::WrongSide { by_ulps: 1 }
        );
        assert_eq!(
            check_directional(100, 101, 97, Direction::Down, 2),
            Band::TooFar {
                err_ulps: 3,
                bound_ulps: 2
            }
        );
        assert!(check_directional(100, 101, 101, Direction::Up, 2).is_ok());
        assert_eq!(
            check_directional(100, 101, 100, Direction::Up, 2),
            Band::WrongSide { by_ulps: 1 }
        );
    }

    #[test]
    fn invariant_referee_verdicts() {
        // 50/50 pool, constant product 100*100 = 10000.
        // Pay 10 in, take 9 out: 110*91 = 10010 >= 10000.
        assert!(invariant_preserved(100, 50, 100, 50, 10, 9));
        // Take 10 out for 10 in: 110*90 = 9900 < 10000 — a leak.
        assert!(!invariant_preserved(100, 50, 100, 50, 10, 10));
        // Equality is allowed: nothing moves, nothing leaks.
        assert!(invariant_preserved(100, 50, 100, 50, 0, 0));
        // Draining past the reserve is a leak by definition.
        assert!(!invariant_preserved(100, 50, 100, 50, u128::MAX, 101));
        // Asymmetric weights, huge balances: 2^127-scale operands must not
        // overflow the referee (they can't — it's bigint end to end).
        assert!(invariant_preserved(1 << 127, 1, 1 << 127, 99, 1 << 90, 0));
    }

    #[test]
    fn wide_mul_matches_native_in_range() {
        let a = 0xdead_beef_u128;
        let b = 0x1234_5678_9abc_u128;
        assert_eq!(mul_wide(a, b), (0, a * b));
        let (hi, lo) = mul_wide(u128::MAX, u128::MAX);
        // (2^128 - 1)^2 = 2^256 - 2^129 + 1
        assert_eq!(hi, u128::MAX - 1);
        assert_eq!(lo, 1);
    }
}
