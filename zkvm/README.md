# zkvm — RISC0 guest build and cycle measurement

This is a separate cargo workspace. The root workspace (kernel + harness)
stays on its pinned rustc 1.73 and never compiles a risc0 crate; default CI
is untouched by anything in here.

The host harness remains the source of truth for correctness (the mpmath
oracle gates live in `crates/harness`). This workspace establishes two
narrower facts:

1. the guest is the *same function* — `weighted-math-core` compiled unchanged
   for the zkVM produces bit-identical output to the host across the whole
   fixture set;
2. what the kernel costs in zkVM cycles (see `docs/zkvm-cycles.md` for the
   measured table and analysis).

## Layout

- `methods/guest` — the guest program. A thin word-protocol wrapper around
  `weighted-math-core` (path dependency, no code changes): reads raw u32
  words, dispatches to the kernel, commits each result plus its
  `env::cycle_count()` delta to the journal. Built with
  `overflow-checks = true`, same as the root workspace, so the cycle numbers
  include that cost.
- `methods` — embeds the compiled guest ELF via `risc0-build`.
- `host` — loads `crates/harness/fixtures/*.json`, runs the guest in the
  RISC0 executor (no proving), checks every output bit-for-bit against a
  direct host call, and prints per-call and whole-session cycle counts.
  Exits nonzero on any parity mismatch.

## Prerequisites

```sh
curl -L https://risczero.com/install | bash
rzup install            # cargo-risczero, r0vm, and the risc0 rust toolchain
```

Pinned against risc0 3.0.5 (`rzup install` as of 2026-07 installs exactly
that; if the default moves, `rzup install cargo-risczero 3.0.5` etc.).

## Run

```sh
cd zkvm
cargo run --release
```

Executor only — no proofs are generated, so the run takes seconds. Cycle
counts are deterministic: two runs produce byte-identical output.
