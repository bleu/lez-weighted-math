# Implementation plan: the test harness (the judge before the kernel)

Status: **READY TO IMPLEMENT** (via TDD). Decisions below settled in the Phase 1
grill; see `CONTEXT.md` and `docs/adr/0001-open-decisions.md`.

This plan builds the harness that will judge the `pow` kernel. The kernel stays
`todo!()`. The point is to have a trustworthy grader *first*, so the later TDD
loop has a red target to drive to green. Nothing here implements kernel math.

## Why build the judge first

This is a fund-loss-sensitive kernel. The harness is the safety margin, so it
has to be trustworthy before there is anything to judge. The risk in
"tests-first" is that a test failing against a blank kernel is indistinguishable
from a test failing because the *grader itself* is broken — both just show red.
So the plan deliberately arranges for one path to be **green from day one**
(the grader correctly judging a real implementation, Balancer) and one path to
be **red** (the blank kernel). Green-on-Balancer proves the machinery; red-on-
kernel is the TDD target.

---

## Settled decisions

These resolve the open questions in `CONTEXT.md` and ADR 0001. Fold #1, #5, #6
into ADR 0001 as RESOLVED, and add an ADR 0002 for the harness architecture, as
the first implementation step.

1. **The bound is parametric, not a magic number.** The grader is parameterised
   over `(SCALE, BUDGET)`. The oracle emits the unavoidable representation floor
   per candidate scale; the pass/fail line is `BOUND = quant_floor(SCALE) +
   BUDGET`. Tests are red because the kernel panics, never because the number is
   wrong. When the scale sweep runs post-kernel, `BUDGET`/`SCALE` are the only
   knobs that move — the harness structure does not change.

2. **Two hard gates plus a diagnostic.** The final payout (`tokens_out`, in
   token wei) is the economic gate. The intermediate `pow` is its own hard gate
   at the ulp level, because this is a reusable `pow` package and the primitive
   must stand alone. `1 - power`, computed via the oracle's `expm1` (never by
   subtraction), is a diagnostic that localises sale-start cancellation.

3. **Fixtures store high-precision text; Rust rounds.** Answer keys are ~70–80
   digit decimal strings plus a machine `q128` form (`value * 2^128`, floored).
   Rust rescales to its compiled-in `SCALE` at test time. Re-sweeping the scale
   is a one-line change with zero fixture regeneration. (Pow output is in (0,1],
   so `q128 < 2^128` fits a `u128` exactly, keeping the Rust side native.)

4. **Balancer validates the grader.** The grader grades Balancer's real outputs
   against Balancer's *own* known accuracy — green from day one — proving the
   machinery works while the kernel path stays red. Grading Balancer against our
   (possibly stricter) kernel bound would produce a confusing false-red, so the
   self-check uses a bound appropriate to what it is grading.

5. **Balancer numbers captured offline, committed.** Canonical Solidity
   `LogExpMath` at a pinned commit, run through a committed `forge` print
   script, outputs frozen into a fixture. CI needs neither Foundry nor Python.
   The Balancer *source* stays external — it is GPL-3.0 and cannot be vendored
   into this MIT/Apache repo; only the captured numbers (data) and our script
   are committed.

6. **Signed one-sided error band.** Error must live in `[0, BOUND]`: never on
   the wrong side (a fund leak), never further than the allowance on the right
   side (a precision nit). Direction and magnitude are separate failure
   categories — a wei of overpayment fails louder than "slightly less accurate."

7. **Full wrapper set, each direction-checked.** `pow_up`/`pow_down`,
   `mul_up`/`mul_down`, `div_up`/`div_down`, each with its own invariant
   (`_up ≥ true`, `_down ≤ true`, within `BOUND` on that side). Rule of thumb:
   if `pow_up` produces a value *smaller* than the reference, that is an error
   regardless of magnitude. `calc_out_given_in` rounds **down** (floored payout).

8. **Both test tiers.** Per-wrapper isolated fixtures (fast failure
   localisation; also proves the fixed-point bricks before `pow` is trusted)
   *and* composed `calc_out_given_in` fixtures (the end-to-end economic gate).

9. **proptest owns oracle-free invariants.** CI has no Python, so property tests
   cannot check accuracy on random inputs (there is no answer key for a
   just-invented input). They check what holds by logic: never panics / never
   overflows, output bounds, monotonicity, rounding self-consistency
   (`_up ≥ _down`), across the whole domain. Fixtures own exact accuracy.

10. **Concrete input domain.** Exponent `0.0101 → 99` (from normalised weights
    swinging `0.01 → 0.99`, exponent `w/(1-w)`). Realistic balances `~1e18–1e27`
    wei, pushed toward `2^128` for the overflow-stress band; trades from 1 wei
    to the full reserve. Two named danger zones, both over-sampled:
    - *Sale start:* exponent ∈ [0.0101, 0.05], base ∈ [0.999, 1) — `power ≈ 1`,
      catastrophic cancellation in `1 - power`.
    - *Overflow edge:* balances near `2^128`, exponent at 99 — the
      `balance_out · (1 - power)` widening multiply at its largest.

11. **Balances are raw `u128`.** The scaffold's `Fixed` balance params are wrong:
    a `1e27` reserve times `2^52` overflows `i128`. `Fixed` appears only for the
    internal ratio `base = balance_in/(balance_in+amount_in)` and the exponent
    `w_in/w_out`. Correcting the `weighted.rs` signatures (`Fixed → u128`
    balances, bodies stay `todo!()`) is part of this work.

### Out of scope for this harness

- **Series choice / term count (ADR #3)** and the **`expm1` boundary threshold
  (ADR #4)** are *kernel* decisions. The harness grades results, not the method,
  so it stays agnostic. The `expm1` danger zone shows up only as fixture
  weighting (decision #10).
- **Criterion benchmarks.** Deferred — nothing to benchmark until the kernel
  exists, and it drags in a heavy dependency for zero signal today.

---

## Deliverables and layout

```
crates/harness/
  oracle/                 OFFLINE, never in CI
    gen.py                mpmath ground-truth generator
    requirements.txt      pin mpmath
    README.md             regen command + what each fixture means
  balancer-ref/           OFFLINE, never in CI
    print_pow.s.sol       forge script: reads shared inputs, prints LogExpMath(pow)
    README.md             pinned commit + how to fetch LogExpMath (GPL, external)
  fixtures/               committed JSON, the only thing CI reads
    pow.json              pow(base,exp) cases: decimal + q128 truth
    out_given_in.json     full swap cases + floored payout
    arith.json            input pairs for mul/div/complement wrapper checks
    scales.json           quant floor per candidate SCALE
    balancer_pow.json     Balancer's pow outputs for the shared inputs
  src/
    lib.rs                fixture structs (serde), quantizer, band checker, config
  tests/
    differential.rs       red on kernel, green grading Balancer
    proptest_invariants.rs  oracle-free invariants + green generator sanity
  Cargo.toml              dev-deps: serde, serde_json, proptest
```

## Fixture schema (the contract between oracle and Rust)

Scale-independent. Truth stored as decimal string (human) and `q128` integer
(machine, `floor(value · 2^128)`), which Rust rescales to `SCALE`.

`pow.json` case:
```json
{ "base": "0.999…", "exponent": "0.0101…",
  "pow_exact": "0.9999…", "pow_exact_q128": "34027…", "zone": "sale_start" }
```

`out_given_in.json` case (balances/amounts are raw u128 wei strings):
```json
{ "balance_in": "…", "amount_in": "…", "balance_out": "…",
  "weight_in": "1", "weight_out": "99",
  "base": "0.…", "exponent": "0.0101…",
  "power_exact": "0.…", "power_exact_q128": "…",
  "one_minus_power_exact": "…", "tokens_out_floor": "…", "zone": "sale_start" }
```
`tokens_out_floor` is the correctly rounded-down (pool-favouring) payout, so the
Rust gate is a pure integer comparison.

`scales.json` case: `{ "scale": 52, "one": "…", "ulp_decimal": "…", "ulp_q128": "…" }`.

`arith.json`: representative `(a, b)` input pairs for the mul/div wrappers. No
expected value needed — the true product/quotient of the *quantised* inputs is
exact rational arithmetic the Rust harness recomputes at double width (e.g. via
a widened intermediate), then checks the wrapper rounded to the correct side.

## The grader (src/lib.rs)

- `SCALE: u32` and `BUDGET` constants — the two knobs. `BOUND` derived from
  `scales.json`'s ulp for `SCALE` plus `BUDGET`.
- Quantiser: decimal/`q128` string → `Fixed` at compile-time `SCALE`, rounding
  as specified (nearest for reference inputs; the kernel supplies its own
  directional rounding).
- Band checker: given `(reference, actual, direction)` returns one of
  `Ok`, `WrongSide` (direction violation — the loud fund-leak category), or
  `TooFar` (magnitude violation). Distinct types, not a single `assert`.

## Test behaviour to expect (the red/green split)

- `differential.rs`
  - Kernel paths (`pow`, wrappers, `calc_out_given_in`) → **RED**: the kernel is
    `todo!()`, so these panic. That is the TDD target.
  - Balancer path → **GREEN**: grader reads `balancer_pow.json`, checks each
    against `pow.json`'s truth within Balancer's known accuracy. Proves the
    grader works today.
- `proptest_invariants.rs`
  - Kernel invariants (overflow-safety, bounds, monotonicity, `_up ≥ _down`) →
    **RED** on first sample (panics at `todo!()`).
  - Generator sanity ("every generated sample lands in-domain, danger zones
    hit") → **GREEN** now; validates the generators before the kernel exists.

## Build order for the TDD phase

1. Record decisions: update ADR 0001 (#1/#5/#6 → RESOLVED), add ADR 0002
   (harness architecture). Note #11.
2. Correct `weighted.rs` signatures: `Fixed → u128` balances; bodies stay
   `todo!()`. Confirm workspace still builds.
3. Oracle: write `gen.py` + `requirements.txt` + README. Generate and commit
   `pow.json`, `out_given_in.json`, `arith.json`, `scales.json`.
4. Balancer capture: pin a `LogExpMath` commit, write the `forge` print script
   reading the shared inputs, capture `balancer_pow.json`, commit it plus the
   script and a regen README. Source stays external.
5. Harness core (`src/lib.rs`): structs, quantiser, band checker, `(SCALE,
   BUDGET)` config. Add dev-deps.
6. Test suites: `differential.rs`, `proptest_invariants.rs` with danger-zone-
   weighted generators.
7. Verify the split: `cargo test` shows kernel paths red (todo panic), Balancer
   + generator-sanity green. Commit the harness (exclude `docs/handoffs/`).

## Verification checklist

- [ ] `cargo build --workspace` clean after the signature fix.
- [ ] `cargo test`: Balancer differential test and generator-sanity test pass.
- [ ] `cargo test`: kernel differential + kernel proptest fail by panicking at
      `todo!()` (not by a grader error) — confirm the panic message is the
      `todo!()`, proving the grader ran and reached the kernel.
- [ ] CI (`fmt`/`clippy`/`test`) needs no Python and no Foundry.
- [ ] Changing `SCALE` recompiles and re-runs with no fixture regeneration.
