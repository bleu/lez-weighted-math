//! Coverage-guided companion to the proptest invariant properties
//! (ADR 0007): on any trade inside the enforced envelope, the curve value
//! `b_in^w_in * b_out^w_out` must not decrease, and the kernel must not
//! panic. The referee is the harness's exact bigint comparison.
//!
//! Raw bytes are shaped *into* the envelope before calling the kernel:
//! out-of-envelope inputs panic by design (documented asserts), so an
//! unconstrained target would drown in intentional crashes. One selector
//! bit picks exact-in or exact-out, so all fuzz time exercises the shared
//! pow pipeline.
//!
//! Unlike the proptest generators, weights here are arbitrary pairs in
//! 1..=99 (not just w / 100-w), reaching every exponent quantization p/q
//! with p, q <= 99.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use weighted_math_core::weighted;

#[derive(Arbitrary, Debug)]
struct Trade {
    exact_out: bool,
    balance_in: u128,
    balance_out: u128,
    weight_in: u8,
    weight_out: u8,
    amount: u128,
}

fuzz_target!(|t: Trade| {
    let balance_in = t.balance_in.max(1);
    let balance_out = t.balance_out.max(1);
    let w_in = u128::from(t.weight_in % 99) + 1;
    let w_out = u128::from(t.weight_out % 99) + 1;

    if t.exact_out {
        // Envelope: amount_out <= 30% of the reserve, and the payment must
        // fit u128 — the same sufficient condition the proptest generator
        // uses (exponent * drain_fraction <= 1/4), since exceeding it hits
        // the kernel's documented fit assert, not a bug.
        let cap30 = balance_out / 10 * 3 + balance_out % 10 * 3 / 10;
        let cap_fit = (balance_out / (4 * w_out))
            .checked_mul(w_in)
            .unwrap_or(u128::MAX);
        let max_out = cap30.min(cap_fit);
        if max_out == 0 {
            return;
        }
        let amount_out = 1 + t.amount % max_out;
        let cost = weighted::calc_in_given_out(balance_in, w_in, balance_out, w_out, amount_out);
        assert!(
            harness::invariant_preserved(balance_in, w_in, balance_out, w_out, cost, amount_out),
            "curve decreased: pool undercharged for the trade"
        );
    } else {
        // Envelope: total deposit stays below 2^128. Wider than the
        // proptest swap generator, which also caps the trade at the
        // reserve; the kernel promises the full range.
        let max_in = u128::MAX - balance_in;
        if max_in == 0 {
            return;
        }
        let amount_in = 1 + t.amount % max_in;
        let out = weighted::calc_out_given_in(balance_in, w_in, balance_out, w_out, amount_in);
        assert!(
            harness::invariant_preserved(balance_in, w_in, balance_out, w_out, amount_in, out),
            "curve decreased: pool paid too much for the trade"
        );
    }
});
