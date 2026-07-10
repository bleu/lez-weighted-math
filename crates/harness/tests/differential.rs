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

/// The reverse economic gate: the ceiled payment in wei. Direction is
/// absolute (never undercharge — that is the fund-loss surface for exact-out
/// trades); magnitude mirrors the out gate, with an extra first-order term
/// for the kernel's own power error (`sens_pow · bound_ulps · 2^-SCALE`).
#[test]
fn kernel_in_given_out_gate() {
    let cases: Vec<InGivenOutCase> = load_fixture("in_given_out.json");
    assert_eq!(
        cases[0].zone, "sale_start",
        "danger zone must lead the file"
    );
    for c in &cases {
        let amount_in = weighted::calc_in_given_out(
            parse_u128(&c.balance_in),
            parse_u128(&c.weight_in),
            parse_u128(&c.balance_out),
            parse_u128(&c.weight_out),
            parse_u128(&c.amount_out),
        );
        let truth = parse_u128(&c.amount_in_ceil);
        let balance_in = parse_u128(&c.balance_in);

        // Direction: the pool must never charge less than the true price.
        assert!(
            amount_in >= truth,
            "UNDERCHARGE (fund leak): kernel {amount_in} < truth {truth} in {c:?}"
        );

        // Magnitude: input-formation sensitivities (doubled for curvature)
        // plus the kernel's power-error and grid terms. Sensitivities beyond
        // u128 mean the case is direction-only (deep-drain corner).
        let sens = parse_u128_checked(&c.sens_base_wei)
            .and_then(|a| parse_u128_checked(&c.sens_exp_wei).and_then(|b| a.checked_add(b)));
        let sens_pow = parse_u128_checked(&c.sens_pow_wei);
        if let (Some(sens), Some(sens_pow)) = (sens, sens_pow) {
            let bound_wei = (sens >> (SCALE - 1))
                .saturating_add(wide_shr_or_max(sens_pow, bound_ulps()))
                .saturating_add((balance_in >> SCALE) * bound_ulps())
                .saturating_add(1);
            let overcharge = amount_in - truth;
            assert!(
                overcharge <= bound_wei,
                "overcharge {overcharge} wei > bound {bound_wei} in {c:?}"
            );
        }
    }
}

/// `sens_pow * ulps / 2^SCALE`, saturating (sens_pow can be near u128::MAX).
fn wide_shr_or_max(sens_pow: u128, ulps: u128) -> u128 {
    let (hi, lo) = mul_wide(sens_pow, ulps);
    if hi >> SCALE != 0 {
        u128::MAX
    } else {
        (hi << (128 - SCALE)) | (lo >> SCALE)
    }
}

/// Spot price: exact rational band check, no oracle value needed. The
/// implementation composes two up-rounded steps (reserve ratio, weight
/// ratio), so it is >= the true rational (direction, checked exactly at
/// double width) and within a small computable band above it.
#[test]
fn kernel_spot_price_check() {
    let cases: Vec<OutGivenInCase> = load_fixture("out_given_in.json");
    let mut checked = 0;
    for c in &cases {
        let (b_in, w_in) = (parse_u128(&c.balance_in), parse_u128(&c.weight_in));
        let (b_out, w_out) = (parse_u128(&c.balance_out), parse_u128(&c.weight_out));
        // The exact check multiplies cross terms at double width; skip the
        // few cases whose cross products already exceed u128.
        let (Some(num), Some(den)) = (b_in.checked_mul(w_out), b_out.checked_mul(w_in)) else {
            continue;
        };
        let spot = weighted::spot_price(b_in, w_in, b_out, w_out);
        assert!(spot.0 > 0, "spot must be positive in {c:?}");
        let spot_repr = spot.0 as u128;
        if spot_repr >= 1 << 74 {
            continue; // outside the band-check envelope
        }

        // Direction: spot * den >= num * 2^SCALE, exactly.
        let lhs = mul_wide(spot_repr, den);
        let rhs = shl_wide(num, SCALE);
        assert!(
            wide_le(rhs, lhs),
            "spot understates the true price (wrong side) in {c:?}"
        );

        // Magnitude: spot <= true + band, checked as
        // (spot - band) * den <= num * 2^SCALE. The band mirrors the
        // implementation's two up-roundings: 2 ulps through the weight
        // ratio, 2 through the reserve ratio, plus slack.
        let wr = (w_out << SCALE).div_ceil(w_in);
        let r_est = ((spot_repr << SCALE) / wr) + 1;
        let band = 2 * (wr >> SCALE) + (r_est >> SCALE) + 4;
        let lhs = mul_wide(spot_repr.saturating_sub(band), den);
        assert!(
            wide_le(lhs, rhs),
            "spot too far above the true price: band {band} in {c:?}"
        );
        checked += 1;
    }
    assert!(checked > 20, "too few spot cases checked: {checked}");
}

/// Not a gate: reports the worst pow/payout errors at the compiled SCALE,
/// for the scale sweep and the error-bound writeup. Run explicitly:
/// `cargo test -p harness --test differential -- --ignored --nocapture`
#[test]
#[ignore]
fn measure_error_margins() {
    let cases: Vec<PowCase> = load_fixture("pow.json");
    // (zone, worst raw offset below truth, above truth, worst up/down err)
    let mut worst_low = 0i128; // raw below t_floor, in ulps
    let mut worst_high = 0i128; // raw above t_ceil, in ulps
    let (mut worst_up, mut worst_down) = (0i128, 0i128);
    for c in &cases {
        let base = s40_to_fixed(&c.base_s40);
        let expo = s40_to_fixed(&c.exponent_s40);
        let (t_floor, t_ceil) = q_to_scale_bounds(&c.pow_exact_q128, 128);
        let raw = pow::pow(base, expo).0;
        worst_low = worst_low.max(t_floor - raw);
        worst_high = worst_high.max(raw - t_ceil);
        worst_up = worst_up.max(pow::pow_up(base, expo).0 - t_ceil);
        worst_down = worst_down.max(t_floor - pow::pow_down(base, expo).0);
    }
    println!(
        "SCALE={SCALE} raw_below={worst_low} raw_above={worst_high} \
         up_err={worst_up} down_err={worst_down} bound={}",
        bound_ulps()
    );

    let swaps: Vec<OutGivenInCase> = load_fixture("out_given_in.json");
    let mut worst_rel_num = 0u128; // shortfall as multiple of balance_out ulps
    for c in &swaps {
        let tokens_out = weighted::calc_out_given_in(
            parse_u128(&c.balance_in),
            parse_u128(&c.weight_in),
            parse_u128(&c.balance_out),
            parse_u128(&c.weight_out),
            parse_u128(&c.amount_in),
        );
        let truth = parse_u128(&c.tokens_out_floor);
        assert!(tokens_out <= truth);
        let shortfall = truth - tokens_out;
        let ulp_of_reserve = (parse_u128(&c.balance_out) >> SCALE).max(1);
        worst_rel_num = worst_rel_num.max(shortfall / ulp_of_reserve);
    }
    println!("payout worst shortfall = {worst_rel_num} reserve-ulps (balance_out * 2^-SCALE)");
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
