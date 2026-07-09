//! Oracle-free property tests (ADR 0002): what holds by logic across the
//! whole domain, with danger zones oversampled. CI has no Python, so there
//! is no answer key for random inputs — accuracy stays with the fixtures.
//!
//! Kernel invariants are RED until the kernel exists; the generator-sanity
//! tests are GREEN today and prove the generators themselves.

use proptest::prelude::*;

use harness::{Fixed, ONE, SCALE};
use weighted_math_core::{pow, weighted};

const MAX_EXPONENT: i128 = 99 * (1i128 << SCALE);

// ---------------------------------------------------------------------------
// Generators (danger zones weighted per ADR 0002 / plan decision 10)
// ---------------------------------------------------------------------------

/// LBP-style normalised weight pair: ratio spans exactly 1/99 ..= 99.
fn weights() -> impl Strategy<Value = (u128, u128)> {
    (1u128..=99).prop_map(|w| (w, 100 - w))
}

/// Sale-start weights: exponent in [1/99, ~0.05].
fn sale_start_weights() -> impl Strategy<Value = (u128, u128)> {
    (1u128..=5).prop_map(|w| (w, 100 - w))
}

/// Realistic reserves (~1e18..1e27 wei) plus an overflow-stress band pushed
/// toward 2^128.
fn balance() -> impl Strategy<Value = u128> {
    prop_oneof![
        4 => 10u128.pow(18)..=10u128.pow(27),
        1 => (1u128 << 120)..(1u128 << 127),
    ]
}

/// A full swap: total deposit stays inside the 2^128 envelope; trades run
/// from 1 wei to the full reserve.
fn swap() -> impl Strategy<Value = (u128, u128, u128, (u128, u128))> {
    (balance(), balance(), weights()).prop_flat_map(|(balance_in, balance_out, w)| {
        let max_in = balance_in.min(u128::MAX - balance_in);
        (Just(balance_in), 1..=max_in, Just(balance_out), Just(w))
    })
}

/// Sale-start swap: tiny trades against a fresh pool, base >= 0.999.
fn sale_start_swap() -> impl Strategy<Value = (u128, u128, u128, (u128, u128))> {
    (
        10u128.pow(18)..=10u128.pow(24),
        10u128.pow(24)..=10u128.pow(27),
        sale_start_weights(),
    )
        .prop_flat_map(|(balance_in, balance_out, w)| {
            (
                Just(balance_in),
                1..=balance_in / 1000,
                Just(balance_out),
                Just(w),
            )
        })
}

fn any_swap() -> impl Strategy<Value = (u128, u128, u128, (u128, u128))> {
    prop_oneof![2 => sale_start_swap(), 3 => swap()]
}

/// pow domain: base in (0,1), exponent in (0, 99], with the sale-start
/// corner (base ~ 1, exponent ~ 0.0101) oversampled.
fn pow_input() -> impl Strategy<Value = (Fixed, Fixed)> {
    let one = ONE.0;
    let general = (1..one, one / 99..=MAX_EXPONENT);
    let sale = (one - one / 1000..one, one / 99..=one / 20);
    let small_base = (1..one / 1_000_000, one / 99..=MAX_EXPONENT);
    prop_oneof![3 => sale, 4 => general, 2 => small_base].prop_map(|(b, e)| (Fixed(b), Fixed(e)))
}

/// Nonnegative fixed-point values whose pairwise products stay inside the
/// kernel's documented envelope.
fn small_fixed() -> impl Strategy<Value = Fixed> {
    (0..=200 * ONE.0).prop_map(Fixed)
}

// ---------------------------------------------------------------------------
// GREEN today: generator sanity
// ---------------------------------------------------------------------------

proptest! {
    /// Every generated swap lands inside the enforced envelope.
    #[test]
    fn generator_swaps_in_domain((balance_in, amount_in, balance_out, (w_in, w_out)) in any_swap()) {
        prop_assert!(amount_in >= 1);
        prop_assert!(balance_in.checked_add(amount_in).is_some(), "total deposit overflows");
        prop_assert!(balance_out >= 1);
        prop_assert!((1..=99).contains(&w_in) && w_in + w_out == 100);
    }

    /// The sale-start generator actually hits the danger zone.
    #[test]
    fn generator_sale_start_hits_zone((balance_in, amount_in, _bo, (w_in, w_out)) in sale_start_swap()) {
        // base = b/(b+a) >= 0.999  <=>  1000*b >= 999*(b+a)  <=>  b >= 999*a
        prop_assert!(balance_in >= 999 * amount_in);
        prop_assert!(w_in <= 5 && w_out >= 95);
    }

    /// pow inputs are in-domain and the sale corner is reachable.
    #[test]
    fn generator_pow_inputs_in_domain((base, expo) in pow_input()) {
        prop_assert!(base.0 > 0 && base.0 < ONE.0);
        prop_assert!(expo.0 > 0 && expo.0 <= MAX_EXPONENT);
    }
}

// ---------------------------------------------------------------------------
// RED until the kernel exists: kernel invariants
// ---------------------------------------------------------------------------

proptest! {
    /// pow output lives in (0,1]; directional variants bracket each other.
    /// Running at all is the empirical no-panic/no-overflow companion to the
    /// written overflow proof.
    #[test]
    fn pow_bounds_and_rounding((base, expo) in pow_input()) {
        let up = pow::pow_up(base, expo);
        let down = pow::pow_down(base, expo);
        prop_assert!(up.0 > 0, "pow_up must stay in (0,1]");
        prop_assert!(up.0 <= ONE.0);
        prop_assert!(down.0 >= 0);
        prop_assert!(down.0 <= up.0, "pow_down must not exceed pow_up");
    }

    /// pow is monotone: decreasing in exponent (base < 1), increasing in base.
    #[test]
    fn pow_monotonicity((base, expo) in pow_input(), bump in 1i128..(1i128 << 40)) {
        let p = pow::pow_down(base, expo);
        if expo.0 + bump <= MAX_EXPONENT {
            prop_assert!(pow::pow_down(base, Fixed(expo.0 + bump)).0 <= p.0 + 1);
        }
        if base.0 + bump < ONE.0 {
            prop_assert!(pow::pow_down(Fixed(base.0 + bump), expo).0 + 1 >= p.0);
        }
    }

    /// Rounding self-consistency of the fixed-point bricks.
    #[test]
    fn arith_rounding_consistency(a in small_fixed(), b in small_fixed()) {
        let md = a.mul_down(b);
        let mu = a.mul_up(b);
        prop_assert!(md.0 <= mu.0);
        prop_assert!(mu.0 - md.0 <= 1, "up and down differ by at most one ulp");
        if b.0 > 0 {
            let dd = a.div_down(b);
            let du = a.div_up(b);
            prop_assert!(dd.0 <= du.0);
            prop_assert!(du.0 - dd.0 <= 1);
            // mul/div round-trip never manufactures value
            prop_assert!(dd.mul_down(b).0 <= a.0);
            prop_assert!(du.mul_up(b).0 >= a.0);
        }
    }

    /// complement is an exact involution on [0, ONE] and saturates above.
    #[test]
    fn complement_exactness(a in small_fixed()) {
        let c = a.complement();
        if a.0 <= ONE.0 {
            prop_assert_eq!(c.0, ONE.0 - a.0);
            prop_assert_eq!(c.complement().0, a.0);
        } else {
            prop_assert_eq!(c.0, 0);
        }
    }

    /// The payout never exceeds the reserve and never panics anywhere in the
    /// enforced envelope.
    #[test]
    fn calc_out_bounded((balance_in, amount_in, balance_out, (w_in, w_out)) in any_swap()) {
        let out = weighted::calc_out_given_in(balance_in, w_in, balance_out, w_out, amount_in);
        prop_assert!(out <= balance_out);
    }

    /// Paying more in never yields less out (weak monotonicity), so the
    /// payout function cannot be gamed by splitting or inflating trades.
    #[test]
    fn calc_out_monotone_in_amount(
        (balance_in, amount_in, balance_out, (w_in, w_out)) in any_swap(),
        extra in 1u128..=10u128.pow(18),
    ) {
        prop_assume!(balance_in.checked_add(amount_in).and_then(|t| t.checked_add(extra)).is_some());
        let out1 = weighted::calc_out_given_in(balance_in, w_in, balance_out, w_out, amount_in);
        let out2 = weighted::calc_out_given_in(balance_in, w_in, balance_out, w_out, amount_in + extra);
        prop_assert!(out2 >= out1);
    }
}
