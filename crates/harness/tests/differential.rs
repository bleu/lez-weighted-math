//! Differential tests against the mpmath oracle fixtures.
//!
//! Two kinds of test live here (ADR 0002):
//! - kernel gates: RED until the kernel exists (they panic at `todo!()`),
//! - grader self-validation: GREEN from day one (grading Balancer's captured
//!   outputs, and structural sanity of the fixtures themselves). These prove
//!   the machinery works while the kernel path stays red.

use harness::*;
use weighted_math_core::{fixed, pow, weighted};

fn assert_band(band: Band, what: &str, context: &dyn std::fmt::Debug) {
    match band {
        Band::Ok { .. } => {}
        Band::WrongSide { by_ulps } => panic!(
            "DIRECTION VIOLATION (fund-leak side): {what} off by {by_ulps} ulps in {context:?}"
        ),
        Band::TooFar {
            err_ulps,
            bound_ulps,
        } => panic!(
            "magnitude violation: {what} err {err_ulps} ulps > bound {bound_ulps} in {context:?}"
        ),
    }
}

// ---------------------------------------------------------------------------
// GREEN path: the grader validates itself against Balancer + fixture sanity
// ---------------------------------------------------------------------------

/// Grades Balancer's captured LogExpMath outputs against the mpmath truth,
/// within Balancer's own documented accuracy (1e-14 relative + 2 wei of its
/// 1e-18 grid). Green here proves the fixture plumbing and the band-checking
/// machinery on a real, working implementation of this math.
#[test]
fn grader_validates_balancer_capture() {
    let inputs: Vec<BalancerInput> = load_fixture("balancer_inputs.json");
    let outputs: Vec<Option<String>> = load_fixture("balancer_pow.json");
    assert_eq!(
        inputs.len(),
        outputs.len(),
        "capture out of sync with inputs"
    );

    let mut graded = 0;
    for (input, output) in inputs.iter().zip(&outputs) {
        if input.skip {
            assert!(output.is_none(), "capture has a value for a skipped case");
            continue;
        }
        let actual = parse_u128(output.as_ref().expect("missing captured output"));
        let truth = parse_u128(input.expected18_floor.as_ref().unwrap());
        // Balancer's own error band, not the (stricter) kernel bound.
        let bound = truth / 100_000_000_000_000 + 2;
        let err = actual.abs_diff(truth);
        assert!(
            err <= bound,
            "Balancer capture outside its own band: x18={} y18={} actual={actual} \
             truth={truth} err={err} bound={bound}",
            input.x18,
            input.y18,
        );
        graded += 1;
    }
    assert!(graded > 60, "too few Balancer cases graded: {graded}");
}

/// The compiled-in SCALE must be one the oracle emitted a quantization floor
/// for, and the data must agree with the code's own ulp.
#[test]
fn grader_scale_is_covered_by_sweep_fixture() {
    let scales: Vec<ScaleCase> = load_fixture("scales.json");
    let entry = scales
        .iter()
        .find(|s| s.scale == SCALE)
        .unwrap_or_else(|| panic!("scales.json has no entry for SCALE={SCALE}"));
    assert_eq!(parse_u128(&entry.one), 1u128 << SCALE);
    assert_eq!(parse_u128(&entry.ulp_q128), 1u128 << (128 - SCALE));
    assert_eq!(ONE.0, (1i128) << SCALE);
}

/// Structural sanity of the primary fixture: danger zones present and
/// leading, truths in (0,1), inputs on the dyadic grid.
#[test]
fn grader_pow_fixture_sanity() {
    let cases: Vec<PowCase> = load_fixture("pow.json");
    assert!(cases.len() > 60);
    assert_eq!(
        cases[0].zone, "sale_start",
        "danger zone must lead the file"
    );
    assert!(cases.iter().any(|c| c.zone == "exp_high"));
    assert!(cases.iter().any(|c| c.zone == "small_base"));
    for c in &cases {
        let base = s40_to_fixed(&c.base_s40);
        assert!(base.0 > 0 && base.0 < ONE.0, "base out of (0,1): {c:?}");
        let expo = s40_to_fixed(&c.exponent_s40);
        assert!(
            expo.0 > 0 && expo.0 <= 99 * ONE.0,
            "exponent out of (0,99]: {c:?}"
        );
        let q = parse_u128(&c.pow_exact_q128);
        assert!(q < u128::MAX, "pow truth must be < 1");
    }
}

// ---------------------------------------------------------------------------
// RED path: the kernel gates (panic at todo!() until the kernel exists)
// ---------------------------------------------------------------------------

/// The primitive gate: pow_up / pow_down at the ulp level, signed one-sided.
#[test]
fn kernel_pow_gate() {
    let cases: Vec<PowCase> = load_fixture("pow.json");
    for c in &cases {
        let base = s40_to_fixed(&c.base_s40);
        let expo = s40_to_fixed(&c.exponent_s40);
        let (t_floor, t_ceil) = q_to_scale_bounds(&c.pow_exact_q128, 128);

        let up = pow::pow_up(base, expo);
        assert_band(
            check_directional(t_floor, t_ceil, up.0, Direction::Up, bound_ulps()),
            "pow_up",
            c,
        );
        let down = pow::pow_down(base, expo);
        assert_band(
            check_directional(t_floor, t_ceil, down.0, Direction::Down, bound_ulps()),
            "pow_down",
            c,
        );
        // Output domain: (0,1]. pow_up of a positive base never hits zero.
        assert!(up.0 > 0 && up.0 <= ONE.0, "pow_up out of (0,1] in {c:?}");
        assert!(
            down.0 >= 0 && down.0 <= up.0,
            "pow_down out of range in {c:?}"
        );
    }
}

/// Diagnostic tier: the internal bricks (ln, exp, expm1), two-sided.
#[test]
fn kernel_ln_exp_diagnostics() {
    let fixture: LnExpFixture = load_fixture("ln_exp.json");
    for c in &fixture.ln {
        let x = s40_to_fixed(&c.x_s40);
        let (t_floor, t_ceil) = q_to_scale_bounds(&c.neg_ln_q116, 116);
        let actual = pow::ln(x);
        assert!(actual.0 <= 0, "ln of x <= 1 must be <= 0 in {c:?}");
        assert_band(
            check_two_sided(t_floor, t_ceil, -actual.0, bound_ulps()),
            "ln",
            c,
        );
    }
    for c in &fixture.exp {
        let x = Fixed(-s40_to_fixed(&c.neg_x_s40).0);
        let (t_floor, t_ceil) = q_to_scale_bounds(&c.exp_q128, 128);
        let actual = pow::exp(x);
        assert_band(
            check_two_sided(t_floor, t_ceil, actual.0, bound_ulps()),
            "exp",
            c,
        );

        let (m_floor, m_ceil) = q_to_scale_bounds(&c.neg_expm1_q128, 128);
        let em1 = pow::expm1(x);
        assert!(em1.0 <= 0, "expm1 of x <= 0 must be <= 0 in {c:?}");
        assert_band(
            check_two_sided(m_floor, m_ceil, -em1.0, bound_ulps()),
            "expm1",
            c,
        );
    }
}

/// The fixed-point bricks: exact directional rounding, checked against the
/// true rational at double width. No error allowance at all — these are
/// pure integer operations and must be exactly correctly rounded.
#[test]
fn kernel_arith_wrappers() {
    let cases: Vec<ArithCase> = load_fixture("arith.json");
    let one = ONE.0 as u128;
    for c in &cases {
        let a = s40_to_fixed(&c.a_s40);
        let b = s40_to_fixed(&c.b_s40);
        let (au, bu) = (a.0 as u128, b.0 as u128);

        // mul: down = floor(a*b / 2^SCALE), up = ceil
        let product = mul_wide(au, bu);
        let down = fixed::Fixed::mul_down(a, b).0 as u128;
        let up = fixed::Fixed::mul_up(a, b).0 as u128;
        assert!(
            wide_le(shl_wide(down, SCALE), product) && wide_lt(product, shl_wide(down + 1, SCALE)),
            "mul_down not the floor in {c:?}"
        );
        if product == (0, 0) {
            assert_eq!(up, 0, "mul_up of exact zero in {c:?}");
        } else {
            assert!(
                wide_le(product, shl_wide(up, SCALE))
                    && (up == 0 || wide_lt(shl_wide(up - 1, SCALE), product)),
                "mul_up not the ceiling in {c:?}"
            );
        }

        // div: down = floor(a * 2^SCALE / b), up = ceil (skip b == 0)
        if bu != 0 {
            let numerator = shl_wide(au, SCALE);
            let qd = fixed::Fixed::div_down(a, b).0 as u128;
            let qu = fixed::Fixed::div_up(a, b).0 as u128;
            assert!(
                wide_le(mul_wide(qd, bu), numerator) && wide_lt(numerator, mul_wide(qd + 1, bu)),
                "div_down not the floor in {c:?}"
            );
            if numerator == (0, 0) {
                assert_eq!(qu, 0, "div_up of exact zero in {c:?}");
            } else {
                assert!(
                    wide_le(numerator, mul_wide(qu, bu))
                        && (qu == 0 || wide_lt(mul_wide(qu - 1, bu), numerator)),
                    "div_up not the ceiling in {c:?}"
                );
            }
        }

        // complement: exact ONE - x, saturating at zero.
        let comp = fixed::Fixed::complement(a).0 as u128;
        assert_eq!(comp, one.saturating_sub(au), "complement wrong in {c:?}");
    }
}

/// The economic gate: the floored payout in wei. Direction is absolute
/// (never overpay — that is the fund-loss surface); magnitude is bounded by
/// first-order input-formation sensitivity plus the kernel allowance, all
/// parametric in SCALE.
#[test]
fn kernel_out_given_in_gate() {
    let cases: Vec<OutGivenInCase> = load_fixture("out_given_in.json");
    assert_eq!(
        cases[0].zone, "sale_start",
        "danger zone must lead the file"
    );
    for c in &cases {
        let tokens_out = weighted::calc_out_given_in(
            parse_u128(&c.balance_in),
            parse_u128(&c.weight_in),
            parse_u128(&c.balance_out),
            parse_u128(&c.weight_out),
            parse_u128(&c.amount_in),
        );
        let truth = parse_u128(&c.tokens_out_floor);
        let balance_out = parse_u128(&c.balance_out);

        // Direction: never pay out more than the true floored payout.
        assert!(
            tokens_out <= truth,
            "OVERPAYMENT (fund leak): kernel {tokens_out} > truth {truth} in {c:?}"
        );
        assert!(tokens_out <= balance_out, "payout exceeds reserve in {c:?}");

        // Magnitude: base is formed with one directed rounding (<= 1 ulp) and
        // the exponent likewise; first-order sensitivities convert those to
        // wei, doubled for curvature headroom. The kernel's own pow error
        // adds bound_ulps() of balance_out. Sensitivities beyond u128 mean
        // no meaningful first-order bound exists for the case (deep-drain
        // trades); the direction gate above still applies in full.
        let sens = parse_u128_checked(&c.sens_base_wei)
            .and_then(|a| parse_u128_checked(&c.sens_exp_wei).and_then(|b| a.checked_add(b)));
        if let Some(sens) = sens {
            let bound_wei = (sens >> (SCALE - 1))
                .saturating_add((balance_out >> SCALE) * bound_ulps())
                .saturating_add(1);
            let shortfall = truth - tokens_out;
            assert!(
                shortfall <= bound_wei,
                "payout too small: shortfall {shortfall} wei > bound {bound_wei} in {c:?}"
            );
        }
    }
}
