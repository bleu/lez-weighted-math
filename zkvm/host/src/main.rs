//! Host runner: executes the weighted-math guest in the RISC0 executor over
//! the harness fixture set, checks every output bit-for-bit against a direct
//! host call into `weighted-math-core`, and reports cycle costs.
//!
//! The host harness remains the source of truth (the oracle gates live
//! there); this binary only establishes that the guest is the *same
//! function* — bit-identical — and what it costs in zkVM cycles.
//!
//! Exit code is nonzero on any parity mismatch.

use anyhow::{bail, Context, Result};
use risc0_zkvm::{default_executor, ExecutorEnv};
use serde::Deserialize;
use std::path::PathBuf;
use weighted_math_core::fixed::{Fixed, SCALE};
use weighted_math_core::{pow, weighted};
use zkvm_methods::WEIGHTED_MATH_GUEST_ELF;

/// Fixture inputs are dyadic multiples of 2^-40 (harness::S40_BITS).
const S40_BITS: u32 = 40;

const OP_NAMES: [&str; 10] = [
    "ln",
    "exp",
    "expm1",
    "pow",
    "pow_up",
    "pow_down",
    "calc_out_given_in",
    "calc_in_given_out",
    "u128_div",
    "baseline",
];

// ---------------------------------------------------------------------------
// Fixture loading (only the fields the guest needs; serde ignores the rest)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct PowCase {
    base_s40: String,
    exponent_s40: String,
}

#[derive(Deserialize)]
struct LnCase {
    x_s40: String,
}

#[derive(Deserialize)]
struct ExpCase {
    neg_x_s40: String,
}

#[derive(Deserialize)]
struct LnExpFixture {
    ln: Vec<LnCase>,
    exp: Vec<ExpCase>,
}

#[derive(Deserialize)]
struct OutGivenInCase {
    balance_in: String,
    amount_in: String,
    balance_out: String,
    weight_in: String,
    weight_out: String,
}

#[derive(Deserialize)]
struct InGivenOutCase {
    balance_in: String,
    amount_out: String,
    balance_out: String,
    weight_in: String,
    weight_out: String,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/harness/fixtures")
}

fn load<T: serde::de::DeserializeOwned>(name: &str) -> Result<T> {
    let path = fixtures_dir().join(name);
    let text = std::fs::read_to_string(&path).with_context(|| format!("reading {path:?}"))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {path:?}"))
}

fn s40(s: &str) -> u128 {
    let raw: i128 = s.parse().expect("s40 field");
    (raw << (SCALE - S40_BITS)) as u128
}

fn wei(s: &str) -> u128 {
    s.parse().expect("u128 field")
}

// ---------------------------------------------------------------------------
// Host reference: the same dispatch the guest runs, on host arithmetic
// ---------------------------------------------------------------------------

fn host_eval(op: u32, c: &[u128]) -> u128 {
    match op {
        0 => pow::ln(Fixed(c[0] as i128)).0 as u128,
        1 => pow::exp(Fixed(c[0] as i128)).0 as u128,
        2 => pow::expm1(Fixed(c[0] as i128)).0 as u128,
        3 => pow::pow(Fixed(c[0] as i128), Fixed(c[1] as i128)).0 as u128,
        4 => pow::pow_up(Fixed(c[0] as i128), Fixed(c[1] as i128)).0 as u128,
        5 => pow::pow_down(Fixed(c[0] as i128), Fixed(c[1] as i128)).0 as u128,
        6 => weighted::calc_out_given_in(c[0], c[1], c[2], c[3], c[4]),
        7 => weighted::calc_in_given_out(c[0], c[1], c[2], c[3], c[4]),
        8 => c[0] / c[1],
        9 => c[0],
        _ => unreachable!(),
    }
}

// ---------------------------------------------------------------------------
// Executor plumbing
// ---------------------------------------------------------------------------

struct BatchResult {
    outputs: Vec<u128>,
    cycles: Vec<u64>,
    user_cycles: u64,
    total_cycles: u64,
    segments: usize,
}

fn run_guest(op: u32, cases: &[Vec<u128>]) -> Result<BatchResult> {
    let mut input: Vec<u32> = vec![op, cases.len() as u32];
    for case in cases {
        for &v in case {
            for limb in 0..4 {
                input.push((v >> (32 * limb)) as u32);
            }
        }
    }
    let env = ExecutorEnv::builder()
        .write_slice(&input)
        .build()
        .context("building ExecutorEnv")?;
    let info = default_executor()
        .execute(env, WEIGHTED_MATH_GUEST_ELF)
        .with_context(|| format!("executing guest for op {}", OP_NAMES[op as usize]))?;

    let bytes = &info.journal.bytes;
    if bytes.len() != cases.len() * 24 {
        bail!(
            "journal length {} != expected {} for op {}",
            bytes.len(),
            cases.len() * 24,
            OP_NAMES[op as usize]
        );
    }
    let mut outputs = Vec::with_capacity(cases.len());
    let mut cycles = Vec::with_capacity(cases.len());
    for rec in bytes.chunks_exact(24) {
        outputs.push(u128::from_le_bytes(rec[..16].try_into().unwrap()));
        cycles.push(u64::from_le_bytes(rec[16..24].try_into().unwrap()));
    }
    Ok(BatchResult {
        outputs,
        cycles,
        // user cycles, no continuation/padding overhead
        user_cycles: info.cycles(),
        // what proving actually pays: segments are padded to 2^po2
        total_cycles: info.segments.iter().map(|s| 1u64 << s.po2).sum(),
        segments: info.segments.len(),
    })
}

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

struct CycleStats {
    min: u64,
    median: u64,
    max: u64,
}

fn stats(cycles: &[u64]) -> CycleStats {
    let mut sorted = cycles.to_vec();
    sorted.sort_unstable();
    CycleStats {
        min: sorted[0],
        median: sorted[sorted.len() / 2],
        max: sorted[sorted.len() - 1],
    }
}

fn main() -> Result<()> {
    let mut ops: Vec<(u32, Vec<Vec<u128>>)> = Vec::new();

    let pow_cases: Vec<PowCase> = load("pow.json")?;
    let pow_inputs: Vec<Vec<u128>> = pow_cases
        .iter()
        .map(|c| vec![s40(&c.base_s40), s40(&c.exponent_s40)])
        .collect();
    let ln_exp: LnExpFixture = load("ln_exp.json")?;
    let ln_inputs: Vec<Vec<u128>> = ln_exp.ln.iter().map(|c| vec![s40(&c.x_s40)]).collect();
    let exp_inputs: Vec<Vec<u128>> = ln_exp
        .exp
        .iter()
        .map(|c| vec![(-(s40(&c.neg_x_s40) as i128)) as u128])
        .collect();
    let out_cases: Vec<OutGivenInCase> = load("out_given_in.json")?;
    let out_inputs: Vec<Vec<u128>> = out_cases
        .iter()
        .map(|c| {
            vec![
                wei(&c.balance_in),
                wei(&c.weight_in),
                wei(&c.balance_out),
                wei(&c.weight_out),
                wei(&c.amount_in),
            ]
        })
        .collect();
    let in_cases: Vec<InGivenOutCase> = load("in_given_out.json")?;
    let in_inputs: Vec<Vec<u128>> = in_cases
        .iter()
        .map(|c| {
            vec![
                wei(&c.balance_in),
                wei(&c.weight_in),
                wei(&c.balance_out),
                wei(&c.weight_out),
                wei(&c.amount_out),
            ]
        })
        .collect();
    // Division microbench: operand magnitudes matching the kernel's actual
    // divisions — ln's t-division (~2^123 / ~2^63), the exponent formation
    // (~2^116 / ~2^64), ratio_up (~2^126 / ~2^74), and smaller shapes for
    // contrast.
    let div_inputs: Vec<Vec<u128>> = vec![
        vec![(1u128 << 123) + 12345, (1u128 << 63) + 987],
        vec![(1u128 << 116) + 5, (1u128 << 64) - 59],
        vec![(1u128 << 126) + 7, (1u128 << 74) + 3],
        vec![(1u128 << 90) + 1, (1u128 << 60) + 1],
        vec![(1u128 << 50) + 9, (1u128 << 30) + 2],
        vec![u128::MAX / 3, 1u128 << 64],
    ];
    let baseline_inputs: Vec<Vec<u128>> = (0..16).map(|i| vec![i as u128]).collect();

    ops.push((0, ln_inputs));
    ops.push((1, exp_inputs.clone()));
    ops.push((2, exp_inputs));
    ops.push((3, pow_inputs.clone()));
    ops.push((4, pow_inputs.clone()));
    ops.push((5, pow_inputs.clone()));
    ops.push((6, out_inputs));
    ops.push((7, in_inputs));
    ops.push((8, div_inputs));
    ops.push((9, baseline_inputs));

    // ---- Parity + per-call cycles over every batch --------------------
    let mut mismatches = 0usize;
    let mut results: Vec<(u32, usize, CycleStats)> = Vec::new();
    let mut baseline_median = 0u64;
    // per op: the case whose cycle delta is the batch median (a
    // representative full-pipeline case for the single-call sessions)
    let mut median_case: Vec<Vec<u128>> = vec![Vec::new(); OP_NAMES.len()];
    let mut div_bench: Vec<(u32, u32, u64)> = Vec::new();
    for (op, inputs) in &ops {
        let batch = run_guest(*op, inputs)?;
        let st_median = stats(&batch.cycles).median;
        if let Some(i) = batch.cycles.iter().position(|&c| c == st_median) {
            median_case[*op as usize] = inputs[i].clone();
        }
        for (i, (case, guest_out)) in inputs.iter().zip(&batch.outputs).enumerate() {
            let host_out = host_eval(*op, case);
            if host_out != *guest_out {
                mismatches += 1;
                eprintln!(
                    "PARITY MISMATCH op={} case={i}: host={host_out:#034x} guest={guest_out:#034x} inputs={case:?}",
                    OP_NAMES[*op as usize]
                );
            }
        }
        let st = stats(&batch.cycles);
        if *op == 9 {
            baseline_median = st.median;
        }
        if *op == 8 {
            div_bench = inputs
                .iter()
                .zip(&batch.cycles)
                .map(|(c, &cy)| (128 - c[0].leading_zeros(), 128 - c[1].leading_zeros(), cy))
                .collect();
        }
        results.push((*op, inputs.len(), st));
    }

    println!("host-vs-guest parity: {} ops, SCALE={SCALE}", ops.len());
    println!();
    println!(
        "{:<19} {:>6} {:>9} {:>9} {:>9} {:>11}",
        "op", "cases", "min", "median", "max", "net median"
    );
    for (op, n, st) in &results {
        let net = st.median.saturating_sub(baseline_median);
        println!(
            "{:<19} {:>6} {:>9} {:>9} {:>9} {:>11}",
            OP_NAMES[*op as usize],
            n,
            st.min,
            st.median,
            st.max,
            if *op == 9 { st.median } else { net },
        );
    }

    println!();
    println!("u128_div by operand width (numerator bits / denominator bits -> net cycles):");
    for (nb, db, cy) in &div_bench {
        println!(
            "  {nb:>3} / {db:<3} -> {:>6}",
            cy.saturating_sub(baseline_median)
        );
    }

    // ---- Whole-session cost for single-call programs -------------------
    // Runs the batch-median case of each op alone, so the number is the
    // whole-program cost of one representative full-pipeline call.
    println!();
    println!("single-call sessions (whole-program cost incl. setup/paging):");
    for op in [3u32, 6, 7] {
        let case = &median_case[op as usize];
        let batch = run_guest(op, std::slice::from_ref(case))?;
        println!(
            "  {:<19} user_cycles={:>8} total_cycles={:>8} segments={}",
            OP_NAMES[op as usize], batch.user_cycles, batch.total_cycles, batch.segments,
        );
    }

    if mismatches > 0 {
        bail!("{mismatches} parity mismatches");
    }
    println!();
    println!("all outputs bit-identical host vs guest");
    Ok(())
}
