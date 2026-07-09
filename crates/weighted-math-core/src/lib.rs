//! `weighted-math-core` — fixed-point weighted-pool math kernel.
//!
//! A Balancer-style LBP `pow` kernel: `calc_out_given_in` over
//! `tokensOut = balanceOut · (1 − (balanceIn/(balanceIn+amountIn))^(wIn/wOut))`,
//! correct to a proven error bound against an mpmath oracle (see
//! `crates/harness`) and division-frugal for the RISC0 zkVM. Design brief in
//! `CONTEXT.md`; decisions in `docs/adr/`.
//!
//! The crate is `#![no_std]` with no dependencies, so it builds unchanged
//! for a RISC0 guest.
#![no_std]

pub mod fixed;
pub mod pow;
pub mod weighted;
pub(crate) mod wide;
