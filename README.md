# lez-weighted-math

A fixed-point weighted-pool `pow` math kernel targeting the LEZ RISC0 zkVM. The
goal is a small, deterministic, `no_std` Rust implementation of Balancer-style
weighted-pool swap math — `calcOutGivenIn` / `calcInGivenOut` and the
underlying fixed-point `pow(base, exponent)` — that produces results provable
inside a RISC-V zero-knowledge VM, validated for accuracy against a
high-precision `mpmath` oracle.

## Layout

- `crates/weighted-math-core` — `no_std` math kernel (`fixed`, `pow`,
  `weighted`). No RISC0 dependencies; compiles unchanged as a zkVM guest.
- `crates/harness` — dev/test crate for the mpmath oracle fixtures, the
  differential grader, and `proptest` properties.
- `zkvm` — separate workspace: the RISC0 guest build, host-vs-guest bit
  parity over the fixture set, and cycle measurement (see
  `docs/zkvm-cycles.md` for the measured cost table).
- `fuzz` — separate workspace: the coverage-guided curve-invariant fuzz
  target (local-only, see ADR 0008).
- `CONTEXT.md` — design brief.
- `docs/adr/` — architecture decision records.

## Build & test

```sh
cargo build --workspace
cargo test --workspace
```

### Fuzzing

The curve invariant (`b_in^w_in * b_out^w_out` never decreases across a
trade) is checked two ways: proptest properties in
`crates/harness/tests/proptest_invariants.rs`, which run with the normal
test suite, and a coverage-guided libFuzzer target that explores the whole
enforced envelope against an exact big-integer referee. The fuzz target is
not wired into CI; run it locally when the kernel changes:

```sh
rustup toolchain install nightly   # once
cargo install cargo-fuzz           # once
cargo +nightly fuzz run trade_invariant
```

It runs until interrupted or until it finds a counterexample (saved under
`fuzz/artifacts/`).

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any contribution
intentionally submitted for inclusion in this work by you, as defined in the
Apache-2.0 license, shall be dual-licensed as above, without any additional
terms or conditions.
