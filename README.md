# lez-weighted-math

A fixed-point weighted-pool math kernel targeting the LEZ RISC0 zkVM: a
small, deterministic, `no_std` Rust implementation of Balancer-style
weighted-pool swap math — `calc_out_given_in` / `calc_in_given_out` and the
underlying fixed-point `pow(base, exponent)` — validated for accuracy
against a high-precision `mpmath` oracle.

```rust
use weighted_math_core::weighted::calc_out_given_in;

// Sell 1000 wei of token A into a 99/1 pool.
let out = calc_out_given_in(
    1_000_000_000, // balance_in  (raw u128 wei)
    99,            // weight_in
    500_000_000,   // balance_out
    1,             // weight_out
    1_000,         // amount_in
);
assert!(out > 0 && out < 500_000_000);
```

Balances, amounts, and weights are raw `u128` (the LEZ native token unit);
fixed point appears only internally, at 52 fractional bits (ADR 0004). Every
rounding favours the pool, and one `pow` costs exactly one hardware division
— division being the most expensive RISC0 primitive. `CONTEXT.md` has the
design overview; `docs/adr/` records each decision and its alternatives.

## Accuracy claim

Correct to a proven error bound against mpmath, not bit-identical to
Balancer. `pow_up`/`pow_down` stay within 4 ulps of the true value at
`2^-52` (measured worst case: 2, the deliberate directional pad), on the
pool-safe side only — a result on the fund-losing side of true fails the
suite regardless of magnitude. The sweep table behind the numbers is in
ADR 0004; the written error analysis is `docs/error-analysis.md`.

## How correctness is established

Each layer is independent of the ones above it:

- **Differential gates** — every `pow` and swap output graded against
  committed mpmath fixtures, with signed one-sided error bands
  (`crates/harness/tests/differential.rs`, fixtures from
  `crates/harness/oracle/`; architecture in ADR 0002).
- **Grader self-validation** — the same grader judges Balancer's captured
  `LogExpMath` outputs within Balancer's own documented accuracy, proving
  the machinery on an independent implementation
  (`crates/harness/balancer-ref/`).
- **Oracle-free properties** — bounds, monotonicity, and rounding
  self-consistency over randomized inputs
  (`crates/harness/tests/proptest_invariants.rs`).
- **Overflow safety** — a written proof (`docs/overflow-proof.md`) plus
  hammer tests that pin its envelope in release mode, where Rust wraps
  silently (`crates/harness/tests/overflow_envelope.rs`).
- **Curve invariant** — `b_in^w_in · b_out^w_out` never decreases across a
  trade, checked against an exact big-integer referee by both proptest and
  a coverage-guided fuzzer (ADR 0008).
- **zkVM parity** — the kernel compiled as a RISC0 guest is bit-identical
  to the host across the whole fixture set, with measured cycle costs
  (`zkvm/`, `docs/zkvm-cycles.md`).

## Layout

- `crates/weighted-math-core` — `no_std` math kernel (`fixed`, `pow`,
  `weighted`). No RISC0 dependencies; compiles unchanged as a zkVM guest.
- `crates/harness` — dev/test crate: the mpmath oracle fixtures, the
  differential grader, and the property tests.
- `zkvm` — separate workspace: RISC0 guest build, host-vs-guest parity,
  cycle measurement.
- `fuzz` — separate workspace: the coverage-guided curve-invariant fuzz
  target (local-only, see ADR 0008).

## Build & test

```sh
cargo build --workspace
cargo test --workspace
```

### Fuzzing

The curve-invariant fuzz target is not wired into CI; run it locally when
the kernel changes:

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
