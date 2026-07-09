//! `weighted-math-core` — fixed-point weighted-pool math kernel.
//!
//! Scaffold only: module stubs with signatures and `todo!()` bodies. No math
//! is implemented yet, and there are no RISC0 dependencies. See `CONTEXT.md`
//! and `docs/adr/0001-open-decisions.md` at the workspace root.
//!
//! The crate is `#![no_std]` so it can eventually build for a RISC0 guest.
#![no_std]

pub mod fixed;
pub mod pow;
pub mod weighted;
