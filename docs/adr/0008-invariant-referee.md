# ADR 0008: Curve-invariant property with an exact big-integer referee

Status: **ACCEPTED**. Adds the end-to-end fund-safety property on top of the
per-step directional gates from ADR 0002.

## Context

Every rounding in the kernel is chosen to favour the pool (ADRs 0003, 0007),
and the fixture gates check each step's direction against the mpmath oracle.
What no test stated directly was the property those choices exist to
guarantee: across any trade, the curve value `b_in^w_in * b_out^w_out` must
not decrease — a decrease is the pool paying out more value than it takes in.
The fixtures imply it at their 19-odd points; nothing asserted it across the
random domain.

Checking it needs a referee, and the candidates all had problems:

- The kernel's own `pow` is circular — a directional bug would vouch for
  itself.
- `f64` logarithms carry ~40 bits of relative error into a comparison whose
  interesting failures are one-ulp fund leaks; any epsilon either masks real
  leaks or fires spuriously, and a failure can't be told apart from a
  referee artifact.

## Decision

**Compare the invariant exactly, in big-integer arithmetic.** With the
generator weights capped at 99, both sides are products of u128 values
raised to small integer exponents — at most ~12.7k bits — so
`(b_in + in)^w_in * (b_out - out)^w_out >= b_in^w_in * b_out^w_out` is a
finite exact computation with no rounding of its own. A `false` verdict is
always a real leak.

Supporting choices:

- **`num-bigint` (pinned, builds on rustc 1.73) rather than a hand-rolled
  limb bignum.** The referee should not be more of our own arithmetic that
  could share a blind spot with the kernel. Dependency of `harness` only;
  the kernel and the zkVM guest are untouched.
- **The assertion is `>=`, not strict `>`.** Zero-fee math holds the curve
  exactly constant, so an exactly-representable trade can legitimately land
  on equality; strict would be a false claim about the math.
- **Both trade functions are covered.** `calc_out_given_in` over the
  `any_swap` domain and `calc_in_given_out` over the `exact_out_swap`
  domain — the latter guards the ADR 0007 inversion, where every rounding
  direction flips once.
- **Two vehicles share one referee** (`harness::invariant_preserved`):
  proptest properties in the normal suite, plus a coverage-guided
  cargo-fuzz target (`fuzz/`, its own workspace, nightly-only like `zkvm/`)
  that shapes arbitrary bytes into the enforced envelope before calling the
  kernel — outside it the asserts panic by design, and a raw target would
  drown in intentional crashes. The fuzz target additionally uses arbitrary
  weight pairs in 1..=99 (not just `w / 100-w`), reaching every exponent
  quantization `p/q` with `p, q <= 99`.
- **Fuzzing is local-only.** No CI job; the run command is in the README.
  The proptest properties carry the regression burden in CI.

## Consequences

- A fund leak anywhere in the pipeline — a flipped rounding, a pad in the
  wrong direction, a wide-multiply bug — now fails a property test with a
  shrunk counterexample, independent of the oracle fixtures.
- The referee only works because weights stay small integers. If the weight
  envelope ever widens beyond ratios expressible in small units, the exact
  comparison stops being cheap and this decision needs revisiting.
- `fuzz/Cargo.lock` pins `jobserver` so `cargo check` inside `fuzz/` still
  works on the workspace's stable 1.73; the fuzzer itself needs nightly and
  `cargo-fuzz`, neither of which CI installs.
