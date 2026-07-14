# Pow pipeline error analysis

The kernel computes `pow(base, y) = exp(y · ln base)` at `LN_SCALE = 62`
fractional bits and rounds once at the end. This is the ulp accounting
behind the `POW_PAD_ULPS = 2` directional pad in `pow.rs`; the harness
sweep (ADR 0004) confirms it case by case.

All bounds below are at `LN_SCALE`, where `ulp = 2^-62`.

## ln

`t` carries a half-ulp from its one division. The 13-term Horner loop adds
at most 13 half-ulp roundings, damped by `u <= 0.03`. Reconstruction via
`k·ln2` adds `k` half-ulps of the constant. Total:
`|δ(ln x)| <~ 4 ulp + k·2^-63`.

## The argument product

`y · ln x` amplifies the ln error by `y`, but only bounded products matter:
results below `2^-SCALE` underflow to the padded floor, so the reachable
worst case is `y·k·ln2 <= ~44`. That gives
`y·|δ(ln x)| + |δ(arg)| <= ~2^-54`.

## exp

Truncation after 20 alternating terms is `< 2^-66`. The 19 Horner roundings
add `< 20 ulp`. `exp(x) <= 1`, so the downstream error is at most the
argument error.

## Total

Pipeline error `< ~2^-53.5` — under half an ulp at any `SCALE <= 56`.
Nearest-rounding to `SCALE` adds another half ulp, so `POW_PAD_ULPS = 2`
covers both with margin. The swap paths keep results at 62 bits and use
`PAD_62 = 512` (`2^-53`) instead: twice the pipeline bound.

Measured margins per fixture case:
`cargo test -p harness --test differential -- --ignored --nocapture`.
