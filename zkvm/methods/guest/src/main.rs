//! Minimal RISC0 guest around `weighted-math-core`.
//!
//! The kernel crate is compiled UNCHANGED for the zkVM target; this wrapper
//! only moves raw words in and out. Protocol (all u32 little-endian words,
//! u128 values as 4 limbs low-to-high):
//!
//! input:  [op, n_cases] then per case `words_for(op)` input words
//! journal: per case 6 words — the u128/i128 result bit pattern (4 words)
//!          followed by the guest cycle delta for the call (u64, 2 words)
//!
//! Ops: 0 ln, 1 exp, 2 expm1, 3 pow, 4 pow_up, 5 pow_down,
//!      6 calc_out_given_in, 7 calc_in_given_out,
//!      8 u128 division (hotspot microbench), 9 baseline (empty measured
//!      region: the cost of the two `cycle_count` reads themselves).
//!
//! Cycle deltas are measured with `env::cycle_count()` around the call, with
//! `black_box` fencing inputs and result so the compiler can neither hoist
//! nor sink the work across the measurement points. Subtract the op-10
//! baseline to get the pure call cost.

use core::hint::black_box;

use risc0_zkvm::guest::env;
use weighted_math_core::fixed::Fixed;
use weighted_math_core::{pow, weighted};

fn words_for(op: u32) -> usize {
    match op {
        0..=2 | 9 => 4,
        3..=5 | 8 => 8,
        6 | 7 => 20,
        _ => panic!("unknown op {op}"),
    }
}

fn u128_at(words: &[u32], i: usize) -> u128 {
    let mut v = 0u128;
    for limb in 0..4 {
        v |= (words[i * 4 + limb] as u128) << (32 * limb);
    }
    v
}

fn main() {
    let mut header = [0u32; 2];
    env::read_slice(&mut header);
    let (op, n) = (header[0], header[1]);
    let words = words_for(op);

    let mut buf = [0u32; 20];
    for _ in 0..n {
        let case = &mut buf[..words];
        env::read_slice(case);
        let a = |i: usize| black_box(u128_at(case, i));

        let start = env::cycle_count();
        let result: u128 = match op {
            0 => pow::ln(Fixed(a(0) as i128)).0 as u128,
            1 => pow::exp(Fixed(a(0) as i128)).0 as u128,
            2 => pow::expm1(Fixed(a(0) as i128)).0 as u128,
            3 => pow::pow(Fixed(a(0) as i128), Fixed(a(1) as i128)).0 as u128,
            4 => pow::pow_up(Fixed(a(0) as i128), Fixed(a(1) as i128)).0 as u128,
            5 => pow::pow_down(Fixed(a(0) as i128), Fixed(a(1) as i128)).0 as u128,
            6 => weighted::calc_out_given_in(a(0), a(1), a(2), a(3), a(4)),
            7 => weighted::calc_in_given_out(a(0), a(1), a(2), a(3), a(4)),
            8 => a(0) / a(1),
            9 => a(0),
            _ => unreachable!(),
        };
        let cycles = env::cycle_count() - start;
        let result = black_box(result);

        let mut out = [0u32; 6];
        for limb in 0..4 {
            out[limb] = (result >> (32 * limb)) as u32;
        }
        out[4] = cycles as u32;
        out[5] = (cycles >> 32) as u32;
        env::commit_slice(&out);
    }
}
