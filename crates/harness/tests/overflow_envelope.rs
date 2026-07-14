//! Overflow-envelope tests backing `docs/overflow-proof.md`.
//!
//! Runs in both debug and release (release wraps silently). Each check is
//! an invariant a wrapped intermediate would break; the `should_panic`
//! tests prove the envelope asserts fire in release.

use proptest::prelude::*;

use harness::{Fixed, ONE, SCALE};
use weighted_math_core::{pow, weighted};

const MAX_EXPONENT: i128 = 99 * ONE.0;

// ---------------------------------------------------------------------------
// Deterministic corners
// ---------------------------------------------------------------------------

/// Finding 1 regression (docs/overflow-proof.md): deep-negative `exp`
/// arguments once truncated via `checked_shl` and returned 1.0. All of
/// these must saturate to the true underflow values.
#[test]
fn exp_saturates_deep_negative() {
    let deep = [
        i128::MIN,
        i128::MIN + 1,
        -(1i128 << 120),
        -(1i128 << 90),
        -(64i128 << SCALE),
        -(64i128 << SCALE) + 1, // just inside the cutoff: still underflows
    ];
    for x in deep {
        assert_eq!(pow::exp(Fixed(x)).0, 0, "exp({x}) must underflow to 0");
        assert_eq!(pow::expm1(Fixed(x)).0, -ONE.0, "expm1({x}) must be -1");
    }
}

/// Domain corners: maximal |ln| times maximal exponent is the worst case
/// of the widened argument product.
#[test]
fn pow_at_the_domain_corners() {
    let corners = [
        (Fixed(1), Fixed(MAX_EXPONENT)), // max |ln base|, max exponent
        (Fixed(1), Fixed(1)),
        (Fixed(ONE.0 - 1), Fixed(MAX_EXPONENT)),
        (Fixed(ONE.0 - 1), Fixed(1)),
    ];
    for (base, expo) in corners {
        let up = pow::pow_up(base, expo);
        let down = pow::pow_down(base, expo);
        assert!(up.0 > 0 && up.0 <= ONE.0);
        assert!(down.0 >= 0 && down.0 <= up.0);
    }
}

/// A payment past u128 must halt at the fit assert in release, never
/// wrap. Here the payment is ~2^51 · balance_in.
#[test]
#[should_panic(expected = "widened product exceeds u128")]
fn unrepresentable_payment_panics() {
    let b = 1u128 << 100;
    weighted::calc_in_given_out(b, 1, b, 99, b / 10 * 3);
}

// ---------------------------------------------------------------------------
// Property hammers at the envelope edge
// ---------------------------------------------------------------------------

/// Weight pairs with the two 99/1 extremes oversampled.
fn edge_weights() -> impl Strategy<Value = (u128, u128)> {
    prop_oneof![
        1 => Just((1u128, 99u128)),
        1 => Just((99u128, 1u128)),
        2 => (1u128..=99).prop_map(|w| (w, 100 - w)),
    ]
}

proptest! {
    /// Total deposit pinned to exactly 2^128 - 1. When the trade at least
    /// doubles the reserve, 1 - power >= ~0.00697, so a wrapped
    /// intermediate could not keep the payout above balance_out/256.
    #[test]
    fn calc_out_at_the_deposit_edge(
        balance_in in 1u128..u128::MAX,
        balance_out in (1u128 << 20)..=u128::MAX,
        (w_in, w_out) in edge_weights(),
    ) {
        let amount_in = u128::MAX - balance_in;
        let out = weighted::calc_out_given_in(balance_in, w_in, balance_out, w_out, amount_in);
        prop_assert!(out <= balance_out);
        if amount_in >= balance_in {
            prop_assert!(out >= balance_out / 256);
        }
    }

    /// The payout multiply is linear in balance_out: doubling the reserve
    /// doubles the payout up to one floor ulp. Pins the widened multiply
    /// near 2^127, where a wrap breaks exact 2x scaling.
    #[test]
    fn payout_multiply_is_linear_in_balance_out(
        balance_in in (1u128 << 64)..(1u128 << 126),
        amount_frac in 1u128..=1000,
        balance_out in (1u128 << 100)..(1u128 << 126),
        (w_in, w_out) in edge_weights(),
    ) {
        let amount_in = (balance_in / 1000) * amount_frac;
        let once = weighted::calc_out_given_in(balance_in, w_in, balance_out, w_out, amount_in);
        let twice = weighted::calc_out_given_in(balance_in, w_in, 2 * balance_out, w_out, amount_in);
        prop_assert!(twice == 2 * once || twice == 2 * once + 1);
    }

    /// Exact-out mirror: the payment multiply is linear in balance_in.
    /// w_out <= w_in plus the 30% cap keep the doubled call inside u128.
    #[test]
    fn payment_multiply_is_linear_in_balance_in(
        balance_in in (1u128 << 64)..(1u128 << 126),
        balance_out in (1u128 << 20)..(1u128 << 126),
        w_in in 50u128..=99,
        out_frac in 1u128..=300,
    ) {
        let w_out = 100 - w_in;
        let amount_out = (balance_out / 1000) * out_frac; // <= floor(30%)
        let once = weighted::calc_in_given_out(balance_in, w_in, balance_out, w_out, amount_out);
        let twice = weighted::calc_in_given_out(2 * balance_in, w_in, balance_out, w_out, amount_out);
        // twice = ceil(2X) and 2*once = 2*ceil(X) differ by at most one.
        prop_assert!(twice == 2 * once || twice + 1 == 2 * once);
    }

    /// exp/expm1 are total on the whole nonpositive line (Finding 1, full
    /// domain) and agree with each other to one ulp.
    #[test]
    fn exp_total_on_nonpositive(x in i128::MIN..=0) {
        let e = pow::exp(Fixed(x));
        let m = pow::expm1(Fixed(x));
        prop_assert!((0..=ONE.0).contains(&e.0));
        prop_assert!((-ONE.0..=0).contains(&m.0));
        prop_assert!(((e.0 - ONE.0) - m.0).abs() <= 1);
    }
}
