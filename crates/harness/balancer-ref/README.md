# Balancer reference capture

Offline only. CI reads the committed `../fixtures/balancer_pow.json` and
never runs Foundry.

Balancer's `LogExpMath` is the secondary comparator (ADR 0001): the grader
grades Balancer's real outputs against the mpmath truth within Balancer's
*own* documented accuracy (1e-14 relative on its 1e18 grid, plus 2 wei of
rounding). Passing here validates the grader machinery on an independent,
production implementation of this math — a grader bug would fail Balancer,
not just the kernel.

## Pinned source

- Repo: `balancer/balancer-v3-monorepo`
- Commit: `7861ea2785b96dd10681ff1b8dfe56b36cc202b6`
- File: `pkg/solidity-utils/contracts/math/LogExpMath.sol`

The source is fetched at capture time into `src-external/` (gitignored),
not vendored. Note: the v3 file is MIT-licensed — the GPL-3.0 concern in
ADR 0001 applies to Balancer v2 — but keeping the source external keeps the
provenance unambiguous either way. Only the captured numbers (data) and our
script live in this repo.

## Regenerate

```sh
./capture.sh   # needs foundry + network; writes ../fixtures/balancer_pow.json
```

`inputs_flat.json` (committed) is emitted by `../oracle/gen.py` from the
shared pow fixture; the script reads it, runs `LogExpMath.pow` per case
(try/catch for the 11 cases outside Balancer's exp domain, recorded as
`null`), and writes the results.
