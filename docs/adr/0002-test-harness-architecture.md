# ADR 0002: Test harness architecture

Status: **ACCEPTED** (Phase 1 grill). Resolves ADR 0001 decisions 1, 5, 6 and
the scale *method* part of decision 2. Implementation plan:
`docs/plans/0001-test-harness.md`.

## Context

The kernel is fund-loss-sensitive. Its correctness claim is "correct to a proven
error bound versus mpmath," not "bit-identical to Balancer." The harness is the
mechanism that makes that claim checkable, so it is built before the kernel —
the kernel bodies stay `todo!()` while the grader is developed.

Building the judge first carries one hazard: a test failing against a blank
kernel looks the same as a test failing because the grader is broken. Both show
red. A grader never observed giving a correct *pass* cannot be trusted, and here
the grader is the entire safety margin. The architecture below is shaped around
neutralising that hazard.

## Decision

### The error bound is parametric (ADR 0001 #1)

No hand-picked pass/fail constant. The bound is
`BOUND = quant_floor(SCALE) + BUDGET`:

- `quant_floor(SCALE)` is the unavoidable representation error on a `2^-SCALE`
  grid — one ulp under directional rounding. The oracle computes it per
  candidate scale and emits it as data (`scales.json`).
- `BUDGET` is the algorithmic allowance the kernel is permitted on top of the
  representation floor.

The kernel tests are red because the kernel panics, never because the number is
wrong. When the post-kernel scale sweep runs, `SCALE` and `BUDGET` are the only
things that move; the harness structure is unaffected.

### Two hard gates plus a diagnostic (ADR 0001 #1)

- **Economic gate:** the final payout `tokens_out`, in token wei. This is the
  fund-loss surface and the natural integer unit.
- **Primitive gate:** the intermediate `pow`, at the ulp level. This crate ships
  a *reusable* `pow`, so the primitive must be correct independently of the LBP
  wrapper.
- **Diagnostic:** `1 - power`, computed by the oracle via `expm1` (never by
  subtraction), localises catastrophic cancellation near the sale start. It
  informs failures; it is not the headline gate.

### Signed, one-sided error band (ADR 0001 #5)

Rounding must always favour the pool, so accuracy is not a symmetric tolerance.
The error must live in `[0, BOUND]`:

- Wrong side (e.g. an `_up` wrapper landing below the reference, or a payout a
  wei too high) is a **direction violation** — a fund leak — and fails
  regardless of magnitude.
- Right side but beyond `BOUND` is a **magnitude violation** — a precision nit.

The two are distinct failure categories, reported differently. `calc_out_given_in`
rounds down (floored payout).

### Full wrapper set, each direction-checked (ADR 0001 #5)

`pow_up`/`pow_down`, `mul_up`/`mul_down`, `div_up`/`div_down`, and `complement`.
Invariant per wrapper: `_up ≥ true` (within `BOUND` above), `_down ≤ true`
(within `BOUND` below). Tested at two tiers: each wrapper in isolation (fast
failure localisation; proves the fixed-point bricks before `pow` is trusted),
and composed through `calc_out_given_in` (the end-to-end economic gate).

### Fixtures are scale-independent (ADR 0001 #2, method)

The oracle stores truth as a high-precision decimal string (human) and a `q128`
integer, `floor(value · 2^128)` (machine). Rust rescales `q128` to its
compiled-in `SCALE` at test time. Pow output is in (0,1], so `q128 < 2^128` fits
a `u128` — the Rust side stays native, no bignum. Re-sweeping the scale never
regenerates a fixture. Balances/amounts are raw `u128` wei strings (ADR 0003);
`tokens_out_floor` is the correctly rounded-down payout, so the gate is a pure
integer comparison.

### Balancer validates the grader (ADR 0001 #6)

Balancer's `LogExpMath` is a real, working implementation of this math, so the
grader can judge *it* today. The differential runner grades Balancer's captured
outputs against the mpmath truth within Balancer's *own* known accuracy — this
path is green from day one and proves the grader machinery is sound while the
kernel path stays red. Grading Balancer against the kernel's (possibly stricter)
bound would produce a confusing false-red, so the self-check uses a bound
appropriate to what it grades.

Source: canonical Solidity `LogExpMath` at a pinned commit, captured offline via
a committed `forge` script into `balancer_pow.json`. CI reads only the fixture —
no Foundry, no Python. The Balancer source is GPL-3.0 and is **not** vendored
into this MIT/Apache repo; only the captured numbers (data) and our script live
here, with a documented regeneration command.

### proptest owns oracle-free invariants (ADR 0001, testing strategy)

CI has no Python, so for a randomly generated input there is no answer key and
property tests cannot check accuracy. They check what holds by logic across the
whole domain: never panics / never overflows (an empirical companion to the
written overflow proof), output bounds (`pow ∈ (0,1]`, `tokens_out ≤
balance_out`), monotonicity, and rounding self-consistency (`_up ≥ _down`).
Exact accuracy stays with the fixture tests. Generators carry the danger-zone
weighting (ADR 0001 #9 domain).

### Scope boundaries

- Kernel-internal decisions (series/term count, `expm1` threshold) are *not*
  harness concerns — it grades results, not method. The `expm1` region is
  covered only as fixture weighting.
- Criterion benchmarks are deferred: nothing to benchmark until the kernel
  exists, and it adds a heavy dependency for no signal now.

## Consequences

- The committed harness has a green path (grader vs. Balancer, generator
  sanity) and a red path (grader vs. blank kernel). The green path is the proof
  the grader works; the red path is the TDD target.
- The scale sweep becomes a knob-turn, not a rebuild.
- CI stays dependency-free (no Python, no Foundry) — fixtures are the only input.
- A wrong-side rounding bug fails loudly and distinctly from a precision
  shortfall, matching the fund-safety priority.
- The kernel API must take raw `u128` balances (ADR 0003), so the `weighted.rs`
  scaffold signatures change as part of implementation.
