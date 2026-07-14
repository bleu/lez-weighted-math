//! Fixed-point weighted-pool math kernel (Balancer-style LBP).
//!
//! Correct to a proven error bound against an mpmath oracle (see
//! `crates/harness`), one division per `pow` for the RISC0 zkVM. Design
//! overview in `CONTEXT.md`; decisions in `docs/adr/`.
//!
//! `#![no_std]`, no dependencies: compiles unchanged as a RISC0 guest.
#![no_std]

pub mod fixed;
pub mod pow;
pub mod weighted;
pub(crate) mod wide;
