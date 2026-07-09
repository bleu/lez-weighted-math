#!/usr/bin/env bash
# Offline capture of Balancer LogExpMath.pow outputs into
# ../fixtures/balancer_pow.json. Requires foundry and network access.
# CI never runs this — it only reads the committed fixture.
set -euo pipefail
cd "$(dirname "$0")"

# Pinned Balancer v3 commit (see README.md). The source is fetched, not
# vendored; src-external/ is gitignored.
COMMIT=7861ea2785b96dd10681ff1b8dfe56b36cc202b6
URL="https://raw.githubusercontent.com/balancer/balancer-v3-monorepo/$COMMIT/pkg/solidity-utils/contracts/math/LogExpMath.sol"

mkdir -p src-external
curl -sSfL -o src-external/LogExpMath.sol "$URL"

forge script script/PowCapture.s.sol:PowCapture --sig "run()"
mv balancer_pow.json ../fixtures/balancer_pow.json
echo "wrote ../fixtures/balancer_pow.json"
