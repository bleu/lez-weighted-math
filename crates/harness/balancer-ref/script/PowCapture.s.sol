// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.24;

import {LogExpMath} from "../src-external/LogExpMath.sol";

/// Minimal cheatcode surface, declared by hand so this script needs no
/// forge-std checkout. The address is forge's well-known cheatcode address.
interface Vm {
    function readFile(string calldata path) external view returns (string memory);

    function parseJsonUintArray(
        string calldata json,
        string calldata key
    ) external pure returns (uint256[] memory);

    function parseJsonBoolArray(
        string calldata json,
        string calldata key
    ) external pure returns (bool[] memory);

    function writeFile(string calldata path, string calldata data) external;

    function toString(uint256 value) external pure returns (string memory);
}

/// Thin deployable wrapper so the capture loop can try/catch the domain
/// reverts LogExpMath raises on out-of-range inputs.
contract PowRunner {
    function pow(uint256 x, uint256 y) external pure returns (uint256) {
        return LogExpMath.pow(x, y);
    }
}

/// Runs the pinned Balancer `LogExpMath.pow` over the shared fixture inputs
/// and freezes the outputs into `fixtures/balancer_pow.json`. Offline only —
/// CI never runs this. See README.md for the pinned commit and regen steps.
contract PowCapture {
    Vm internal constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    function run() external {
        PowRunner runner = new PowRunner();
        string memory json = vm.readFile("inputs_flat.json");
        uint256[] memory xs = vm.parseJsonUintArray(json, ".x18");
        uint256[] memory ys = vm.parseJsonUintArray(json, ".y18");
        bool[] memory skip = vm.parseJsonBoolArray(json, ".skip");
        require(xs.length == ys.length && xs.length == skip.length, "length mismatch");

        string memory out = "[\n";
        for (uint256 i = 0; i < xs.length; i++) {
            string memory entry;
            if (skip[i]) {
                entry = "null";
            } else {
                try runner.pow(xs[i], ys[i]) returns (uint256 r) {
                    entry = string.concat("\"", vm.toString(r), "\"");
                } catch {
                    entry = "null";
                }
            }
            out = string.concat(out, " ", entry, i + 1 < xs.length ? ",\n" : "\n");
        }
        out = string.concat(out, "]\n");
        // forge only permits writes inside the project root; capture.sh moves
        // this into ../fixtures/.
        vm.writeFile("balancer_pow.json", out);
    }
}
