# ADR 0006: calc_in_given_out by base inversion; 30% out-ratio cap

Status: **ACCEPTED**. Extends the kernel API beyond the M1 `calc_out_given_in`
scope without reopening the base ∈ (0,1) design decision.

## Context

The initial API sketch's `calc_in_given_out` was dropped during the kernel
build because its textbook form needs `pow(base > 1)` — the regime whose
deletion is what makes this kernel small (no large-argument `exp`, output
pinned to (0,1]). Restoring it raised the question of whether that decision
had to be reopened.

## Decision

It does not. The function is served by the existing pipeline:

- **calc_in_given_out** is algebraically inverted into the native domain:
  `(b/(b-a))^y − 1 = (1−p)/p` with `p = ((b−a)/b)^y`, where
  `(b−a)/b ∈ (0,1)`. The kernel computes `p` at the internal 62-bit scale
  padded *down* (an understated power overstates the payment), forms
  `(1−p)/p` with one ceiling division (numerator ≤ 2^124, no widening
  needed), and finishes with the widened `balance_in · r` multiply rounded
  up. Every rounding overstates the payment: base' down, exponent up,
  power padded down, final multiply ceiled. Cost: 3 hardware divisions,
  same as `calc_out_given_in` plus the `(1−p)/p` division.

- **amount_out is capped at 30% of the reserve** (Balancer's
  `MAX_OUT_RATIO` parity). Beyond the precedent, the cap does real work
  here: it bounds `p >= 0.7^99 ≈ 2^-50.6`, four times the internal pad
  floor, so the `(1−p)/p` denominator can never collapse. Deep-drain
  exact-out trades revert (assert) instead of returning a garbage price.
  The payment must also fit `u128`; the widened multiply's fit assertion
  is that envelope (a start-of-sale purchase of 30% of the tokens can
  genuinely price beyond 2^128 wei — the pool refuses rather than wraps).

## Consequences

- `in_given_out.json` joins the fixture set (19 cases, sale-start first —
  note the danger zone flips: buying exact tokens at sale start is the
  *high*-exponent side, w_out/w_in = 99). The gate mirrors the out gate:
  direction absolute (never undercharge), magnitude bounded by first-order
  sensitivities plus a `sens_pow` term for the kernel's own power error.
- Accuracy degrades as `p` approaches the pad floor (the 30% corner at
  exponent 99 is direction-only in the fixture, its sensitivity exceeding
  u128); for realistic trades the overcharge is bounded by the same
  parametric formula as the payout gate.
- The u128-fit envelope is input-dependent, unlike `calc_out_given_in`
  whose result is always ≤ the reserve. Callers that must not panic should
  pre-check trade size; the property-test generator documents a sufficient
  condition (`exponent · drain_fraction <= 1/4`).
