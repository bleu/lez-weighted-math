#!/usr/bin/env python3
"""Ground-truth fixture generator for the weighted-math harness.

Runs OFFLINE only — CI reads the committed JSON, never this script.

Everything here is deterministic: no RNG, hand-enumerated case grids, and
fixed mpmath precision. Regenerating must be byte-identical.

Design notes (see docs/adr/0002-test-harness-architecture.md):

* Kernel inputs (pow base/exponent, arith pairs) are dyadic rationals on a
  2^-GRID_BITS grid. They are stored as exact decimal strings (human) and as
  `value * 2^GRID_BITS` integers (machine, field suffix `_s40`). Any binary
  SCALE >= GRID_BITS represents them exactly, so the pow gate measures only
  the kernel's algorithmic error — input quantization contributes nothing,
  and re-sweeping SCALE never regenerates a fixture.
* Truth values are stored as ~78-digit decimal strings (human) plus a
  q-format integer (machine): `floor(value * 2^128)` for values in [0,1),
  `floor(value * 2^116)` for -ln (which can reach ~28 on our input grid).
* out_given_in balances/amounts/weights are raw u128 integers (ADR 0003).
  Truth is computed at the exact rational inputs: exact Fraction arithmetic
  when the exponent is an integer, mpmath at 220 digits otherwise.
  `tokens_out_floor` is the true payout rounded down (pool-favouring), so
  the Rust gate is a pure integer comparison. Each case also carries
  first-order sensitivities (wei of payout per unit of base/exponent error)
  so the harness can derive a SCALE-parametric magnitude bound; the
  direction check (never overpay) is absolute and needs no bound.
"""

import json
from fractions import Fraction
from pathlib import Path

from mpmath import mp, mpf

mp.dps = 220

GRID_BITS = 40  # kernel inputs are dyadic multiples of 2^-GRID_BITS
DIGITS = 78     # human-facing decimal digits for truth values

FIXTURES = Path(__file__).resolve().parent.parent / "fixtures"

# Candidate fixed-point scales for the sweep (ADR 0001 decision 2).
SCALES = [40, 44, 48, 52, 56, 60]


def d(num, den=1) -> Fraction:
    """Round num/den to the dyadic grid, returning the exact grid value."""
    return Fraction(round(Fraction(num, den) * (1 << GRID_BITS)), 1 << GRID_BITS)


def dyadic_str(f: Fraction) -> str:
    """Exact finite decimal string of a dyadic rational (den divides 2^k)."""
    bits = f.denominator.bit_length() - 1
    assert f.denominator == 1 << bits, f"not dyadic: {f}"
    scaled = f.numerator * 5**bits  # f == scaled / 10^bits
    s = str(scaled)
    if bits == 0:
        return s
    s = s.rjust(bits + 1, "0")
    return (s[:-bits] + "." + s[-bits:]).rstrip("0").rstrip(".")


def s40(f: Fraction) -> str:
    """f * 2^GRID_BITS, which is exact by construction."""
    v = f * (1 << GRID_BITS)
    assert v.denominator == 1
    return str(v.numerator)


def to_mpf(f: Fraction) -> mpf:
    return mpf(f.numerator) / mpf(f.denominator)


def nstr(v) -> str:
    return mp.nstr(v, DIGITS)


def qfmt(v, bits=128) -> str:
    """floor(v * 2^bits) for v >= 0, as a decimal integer string."""
    return str(int(mp.floor(mp.ldexp(v, bits))))


def frac_qfmt(f: Fraction, bits=128) -> str:
    """Exact floor(f * 2^bits) for a nonnegative rational."""
    return str((f.numerator << bits) // f.denominator)


# ---------------------------------------------------------------------------
# pow.json — the primitive gate. Danger zones first.
# ---------------------------------------------------------------------------

def pow_truth(base: Fraction, expo: Fraction):
    """(decimal_str, q128_str) for base^expo, exact where possible."""
    if expo.denominator == 1:
        p = base ** expo.numerator
        return nstr(to_mpf(p)), frac_qfmt(p)
    v = mp.power(to_mpf(base), to_mpf(expo))
    return nstr(v), qfmt(v)


def pow_cases():
    one = Fraction(1)
    cases = []

    def add(zone, bases, exps):
        for b in bases:
            for y in exps:
                dec, q128 = pow_truth(b, y)
                cases.append({
                    "zone": zone,
                    "base": dyadic_str(b),
                    "base_s40": s40(b),
                    "exponent": dyadic_str(y),
                    "exponent_s40": s40(y),
                    "pow_exact": dec,
                    "pow_exact_q128": q128,
                })

    # Sale start: power ~ 1, catastrophic-cancellation territory for 1-power.
    add(
        "sale_start",
        [one - Fraction(1, 1 << 40), one - Fraction(1, 1 << 30),
         one - Fraction(1, 1 << 20), one - Fraction(1, 1 << 14),
         d(999, 1000), d(9995, 10000)],
        [d(1, 99), d(2, 100), d(1, 30), d(5, 100)],
    )
    # Near-bound / high exponent: the exponent-99 end of the envelope.
    add(
        "exp_high",
        [one - Fraction(1, 1 << 40), one - Fraction(1, 1 << 20),
         one - Fraction(1, 1 << 10), d(9, 10)],
        [Fraction(99), Fraction(50)],
    )
    # Small bases: deep range reduction, underflow-to-zero paths.
    add(
        "small_base",
        [Fraction(1, 1 << 40), Fraction(1, 1 << 33),
         Fraction(1, 1 << 20), d(1, 10**6)],
        [d(1, 99), d(1, 2), Fraction(1), Fraction(2), Fraction(99)],
    )
    # General grid across the domain interior.
    add(
        "general",
        [d(1, 10), Fraction(1, 4), Fraction(1, 2), Fraction(3, 4), d(9, 10)],
        [d(1, 99), d(1, 2), Fraction(1), Fraction(2), d(73, 10),
         d(3333, 100), Fraction(99)],
    )
    return cases


# ---------------------------------------------------------------------------
# ln_exp.json — diagnostic tier for the kernel's internal bricks.
# ---------------------------------------------------------------------------

def ln_exp_cases():
    one = Fraction(1)
    ln_inputs = [
        Fraction(1, 1 << 40), d(1, 10**6), d(1, 10), Fraction(1, 4),
        Fraction(1, 2), d(70711, 100000), Fraction(3, 4), d(9, 10),
        d(999, 1000), one - Fraction(1, 1 << 40), one,
    ]
    ln_cases = []
    for x in ln_inputs:
        neg_ln = -mp.log(to_mpf(x))
        ln_cases.append({
            "x": dyadic_str(x),
            "x_s40": s40(x),
            "neg_ln_exact": nstr(neg_ln),
            "neg_ln_q116": qfmt(neg_ln, 116),
        })

    # exp arguments are <= 0 in this kernel; store the magnitude.
    exp_inputs = [
        Fraction(1, 1 << 40), d(1, 10**6), d(1, 10), Fraction(1, 2),
        d(693147, 10**6), Fraction(1), Fraction(3), Fraction(10),
        Fraction(36), d(375, 10), Fraction(50), Fraction(90),
    ]
    exp_cases = []
    for t in exp_inputs:
        v = mp.exp(-to_mpf(t))
        neg_em1 = -mp.expm1(-to_mpf(t))
        exp_cases.append({
            "neg_x": dyadic_str(t),
            "neg_x_s40": s40(t),
            "exp_exact": nstr(v),
            "exp_q128": qfmt(v),
            "neg_expm1_exact": nstr(neg_em1),
            "neg_expm1_q128": qfmt(neg_em1),
        })
    return {"ln": ln_cases, "exp": exp_cases}


# ---------------------------------------------------------------------------
# out_given_in.json — the economic gate. Danger zones first.
# ---------------------------------------------------------------------------

def out_case(balance_in, amount_in, balance_out, weight_in, weight_out, zone):
    assert balance_in + amount_in < 1 << 128, "total deposit envelope"
    assert Fraction(1, 99) <= Fraction(weight_in, weight_out) <= 99

    base = Fraction(balance_in, balance_in + amount_in)
    y = Fraction(weight_in, weight_out)
    mbase, my = to_mpf(base), to_mpf(y)

    if y.denominator == 1:
        power = base ** y.numerator          # exact rational
        omp = 1 - power
        tokens = (balance_out * omp.numerator) // omp.denominator
        power_dec, power_q128 = nstr(to_mpf(power)), frac_qfmt(power)
        omp_dec = nstr(to_mpf(omp))
    else:
        power = mp.power(mbase, my)
        omp = -mp.expm1(my * mp.log(mbase))  # never 1 - power by subtraction
        tokens = int(mp.floor(balance_out * omp))
        power_dec, power_q128 = nstr(power), qfmt(power)
        omp_dec = nstr(omp)

    mpower = to_mpf(power) if isinstance(power, Fraction) else power
    # First-order payout sensitivities to input error (wei per unit):
    #   |d tokens / d base| = balance_out * y * base^(y-1)
    #   |d tokens / d y|    = balance_out * base^y * |ln base|
    sens_base = int(mp.ceil(balance_out * my * mp.power(mbase, my - 1)))
    sens_exp = int(mp.ceil(balance_out * mpower * abs(mp.log(mbase))))

    return {
        "zone": zone,
        "balance_in": str(balance_in),
        "amount_in": str(amount_in),
        "balance_out": str(balance_out),
        "weight_in": str(weight_in),
        "weight_out": str(weight_out),
        "base": nstr(mbase),
        "exponent": nstr(my),
        "power_exact": power_dec,
        "power_exact_q128": power_q128,
        "one_minus_power_exact": omp_dec,
        "tokens_out_floor": str(tokens),
        "sens_base_wei": str(sens_base),
        "sens_exp_wei": str(sens_exp),
    }


def out_given_in_cases():
    U127 = (1 << 127) - 1
    U128 = (1 << 128) - 1
    specs = [
        # Sale start: LBP opens collateral-heavy, tiny trades, power ~ 1.
        (10**18, 1, 10**27, 1, 99, "sale_start"),
        (10**18, 10**12, 10**27, 1, 99, "sale_start"),
        (10**18, 10**13, 10**27, 1, 99, "sale_start"),
        (10**18, 10**15, 10**27, 1, 99, "sale_start"),
        (10**18, 10**16, 10**27, 1, 99, "sale_start"),
        (25 * 10**18, 10**15, 4 * 10**26, 1, 99, "sale_start"),
        (10**18, 10**17, 10**27, 2, 98, "sale_start"),
        (5 * 10**20, 5 * 10**17, 10**27, 5, 95, "sale_start"),
        # Overflow edge: balances pushed toward 2^128.
        (15 * 10**37, 15 * 10**37, 3 * 10**38, 1, 99, "overflow_edge"),
        (15 * 10**37, 15 * 10**37, 3 * 10**38, 99, 1, "overflow_edge"),
        (U127, 1 << 126, U128, 1, 99, "overflow_edge"),
        (U127, 1 << 126, U128, 99, 1, "overflow_edge"),
        (10**30, 339 * 10**36, U128, 1, 99, "overflow_edge"),
        (2 * 10**38, 14 * 10**37, 10**27, 50, 50, "overflow_edge"),
        # Dust trades: payout floors to (near) zero, pool-favouring.
        (10**27, 1, 10**27, 1, 99, "dust"),
        (10**27, 1, 10**27, 99, 1, "dust"),
        (10**18, 1, 10**18, 50, 50, "dust"),
        # General mid-sale shapes.
        (10**21, 10**19, 10**24, 50, 50, "general"),
        (10**21, 10**20, 10**24, 60, 40, "general"),
        (10**21, 5 * 10**20, 10**24, 40, 60, "general"),
        (777 * 10**18, 333 * 10**17, 25 * 10**22, 30, 70, "general"),
        (10**18, 10**18, 10**18, 50, 50, "general"),
        (10**24, 3 * 10**23, 8 * 10**22, 70, 30, "general"),
        (123456789 * 10**12, 9876543 * 10**10, 5 * 10**25, 55, 45, "general"),
        # Sale end: exponent at the 99 bound.
        (10**27, 10**24, 10**20, 99, 1, "exp_high"),
        (10**27, 10**26, 10**20, 99, 1, "exp_high"),
        (10**27, 10**27, 10**20, 99, 1, "exp_high"),
        (3 * 10**26, 10**25, 7 * 10**19, 95, 5, "exp_high"),
        (10**24, 10**21, 10**21, 90, 10, "exp_high"),
    ]
    return [out_case(*s) for s in specs]


# ---------------------------------------------------------------------------
# in_given_out.json — the reverse economic gate. The kernel computes it via
# the inverted base p = ((b_out - a)/b_out)^(w_out/w_in), which lives in
# (0,1), and amount_in = b_in · (1-p)/p rounded UP (the user can never pay
# less than the true price). amount_out is capped at 30% of the reserve
# (Balancer parity), which also keeps p far above the kernel's pad floor.
# ---------------------------------------------------------------------------

def in_case(balance_in, weight_in, balance_out, weight_out, amount_out, zone):
    assert amount_out <= balance_out * 3 // 10, "30% out-ratio cap"
    assert Fraction(1, 99) <= Fraction(weight_in, weight_out) <= 99

    base = Fraction(balance_out - amount_out, balance_out)
    y = Fraction(weight_out, weight_in)
    mbase, my = to_mpf(base), to_mpf(y)

    if y.denominator == 1:
        p = base ** y.numerator  # exact rational
        num = balance_in * (p.denominator - p.numerator)
        amount_in = -(-num // p.numerator)  # ceil
        power_dec, power_q128 = nstr(to_mpf(p)), frac_qfmt(p)
    else:
        p = mp.power(mbase, my)
        amount_in = int(mp.ceil(balance_in * (1 / p - 1)))
        power_dec, power_q128 = nstr(p), qfmt(p)

    mpower = to_mpf(p) if isinstance(p, Fraction) else p
    # First-order payment sensitivities (wei per unit of input error):
    #   |d in / d base| = b_in · y / (p · base)
    #   |d in / d y|    = b_in · |ln base| / p
    #   |d in / d p|    = b_in / p²   (the kernel's own power error)
    sens_base = int(mp.ceil(balance_in * my / (mpower * mbase)))
    sens_exp = int(mp.ceil(balance_in * abs(mp.log(mbase)) / mpower))
    sens_pow = int(mp.ceil(balance_in / (mpower * mpower)))

    return {
        "zone": zone,
        "balance_in": str(balance_in),
        "amount_out": str(amount_out),
        "balance_out": str(balance_out),
        "weight_in": str(weight_in),
        "weight_out": str(weight_out),
        "base": nstr(mbase),
        "exponent": nstr(my),
        "power_exact": power_dec,
        "power_exact_q128": power_q128,
        "amount_in_ceil": str(amount_in),
        "sens_base_wei": str(sens_base),
        "sens_exp_wei": str(sens_exp),
        "sens_pow_wei": str(sens_pow),
    }


def in_given_out_cases():
    U127 = (1 << 127) - 1
    specs = [
        # Sale start: buying exact project tokens is the HIGH-exponent side
        # here (exponent = w_out/w_in = 99).
        (10**18, 1, 10**27, 99, 1, "sale_start"),
        (10**18, 1, 10**27, 99, 10**18, "sale_start"),
        (10**18, 1, 10**27, 99, 10**21, "sale_start"),
        (10**18, 1, 10**27, 99, 10**24, "sale_start"),
        (10**18, 1, 10**27, 99, 3 * 10**26, "sale_start"),  # 30% corner
        (25 * 10**18, 5, 4 * 10**26, 95, 10**23, "sale_start"),
        # Overflow edge: huge balances, payment near the top of u128.
        (15 * 10**37, 50, 3 * 10**38, 50, 9 * 10**37, "overflow_edge"),
        (U127, 95, U127, 5, 1 << 125, "overflow_edge"),
        (15 * 10**37, 5, 3 * 10**38, 95, 10**37, "overflow_edge"),
        # Dust.
        (10**27, 1, 10**27, 99, 1, "dust"),
        (10**18, 50, 10**18, 50, 1, "dust"),
        # General mid-sale shapes.
        (10**21, 50, 10**24, 50, 10**19, "general"),
        (10**21, 60, 10**24, 40, 10**20, "general"),
        (10**21, 40, 10**24, 60, 29 * 10**22, "general"),
        (777 * 10**18, 70, 25 * 10**22, 30, 3 * 10**21, "general"),
        (10**24, 30, 8 * 10**22, 70, 10**22, "general"),
        # Sale end: the low-exponent side (1/99).
        (10**27, 99, 10**20, 1, 10**19, "sale_end"),
        (10**27, 99, 10**20, 1, 3 * 10**19, "sale_end"),
        (10**24, 90, 10**21, 10, 10**20, "sale_end"),
    ]
    return [in_case(*s) for s in specs]


# ---------------------------------------------------------------------------
# arith.json — input pairs for the mul/div/complement wrappers. No truth
# needed: the harness recomputes the exact rational at double width.
# ---------------------------------------------------------------------------

def arith_cases():
    pairs = [
        (Fraction(0), Fraction(1, 2)),
        (Fraction(1), Fraction(1)),
        (Fraction(1, 1 << 40), Fraction(1, 1 << 40)),   # product underflows
        (Fraction(1), Fraction(1, 1 << 40)),
        (Fraction(7, 2), Fraction(9, 4)),
        (Fraction(1) - Fraction(1, 1 << 13), Fraction(1) + Fraction(1, 1 << 13)),
        (d(123456, 1000), d(789, 10**6)),
        (Fraction(1000), Fraction(1000)),
        (Fraction(99), d(1, 99)),
        (Fraction(2), Fraction(1, 2)),
        (Fraction(3, 2), Fraction(3, 2)),
        (d(1, 3), Fraction(3)),
    ]
    return [
        {"a": dyadic_str(a), "a_s40": s40(a), "b": dyadic_str(b), "b_s40": s40(b)}
        for a, b in pairs
    ]


# ---------------------------------------------------------------------------
# scales.json — quantization floor per candidate SCALE.
# ---------------------------------------------------------------------------

def scale_cases():
    return [
        {
            "scale": s,
            "one": str(1 << s),
            "ulp_decimal": dyadic_str(Fraction(1, 1 << s)),
            "ulp_q128": str(1 << (128 - s)),
        }
        for s in SCALES
    ]


# ---------------------------------------------------------------------------
# balancer_inputs.json — the shared pow inputs rounded onto Balancer's 1e18
# grid, with truth recomputed AT that grid (so the comparison isolates
# Balancer's own error, not the grid mismatch). Cases outside LogExpMath's
# domain (exp argument < -41) are marked skip.
# ---------------------------------------------------------------------------

def balancer_inputs(pcases):
    E18 = 10**18
    out = []
    for c in pcases:
        base = Fraction(int(c["base_s40"]), 1 << GRID_BITS)
        y = Fraction(int(c["exponent_s40"]), 1 << GRID_BITS)
        x18 = round(base * E18)
        y18 = round(y * E18)
        entry = {"x18": str(x18), "y18": str(y18)}
        if x18 == 0:
            entry["skip"] = True
            out.append(entry)
            continue
        bx = Fraction(x18, E18)
        by = Fraction(y18, E18)
        arg = to_mpf(by) * mp.log(to_mpf(bx))
        if arg < mpf(-41) + mpf("1e-9"):
            entry["skip"] = True
            out.append(entry)
            continue
        v = mp.power(to_mpf(bx), to_mpf(by))
        entry["skip"] = False
        entry["expected18_floor"] = str(int(mp.floor(v * E18)))
        out.append(entry)
    return out


def balancer_flat(entries):
    """Same data flattened into parallel arrays for the forge capture script."""
    return {
        "x18": [int(e["x18"]) for e in entries],
        "y18": [int(e["y18"]) for e in entries],
        "skip": [bool(e.get("skip")) for e in entries],
    }


def dump(name, data):
    path = FIXTURES / name
    path.write_text(json.dumps(data, indent=1) + "\n")
    n = len(data) if isinstance(data, list) else sum(len(v) for v in data.values())
    print(f"wrote {path.name}: {n} entries")


def main():
    FIXTURES.mkdir(exist_ok=True)
    pcases = pow_cases()
    dump("pow.json", pcases)
    dump("ln_exp.json", ln_exp_cases())
    dump("out_given_in.json", out_given_in_cases())
    dump("in_given_out.json", in_given_out_cases())
    dump("arith.json", arith_cases())
    dump("scales.json", scale_cases())
    bcases = balancer_inputs(pcases)
    dump("balancer_inputs.json", bcases)
    flat = FIXTURES.parent / "balancer-ref" / "inputs_flat.json"
    flat.write_text(json.dumps(balancer_flat(bcases)) + "\n")
    print(f"wrote {flat}")


if __name__ == "__main__":
    main()
