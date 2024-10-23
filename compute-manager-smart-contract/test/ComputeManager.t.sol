// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import {ComputeManager} from "../src/ComputeManager.sol";

contract ComputeManagerTest is Test {
    ComputeManager public computeManager;

    address public submitter;
    address public computer;
    address public verifier;

    function setUp() public {
        // Use the address which are the same as Rust tests.
        // (See [project root]/common/src/txs.rs)
        bytes20 pk_bytes = hex"13978aee95f38490e9769c39b2773ed763d9cd5f";
        computer = address(pk_bytes);
        address[] memory _computers = new address[](1);
        _computers[0] = computer;

        pk_bytes = hex"cd2a3d9f938e13cd947ec05abc7fe734df8dd826";
        verifier = address(pk_bytes);
        address[] memory _verifiers = new address[](1);
        _verifiers[0] = verifier;

        submitter = verifier;
        address[] memory _submitters = new address[](1);
        _submitters[0] = submitter;

        computeManager = new ComputeManager(_submitters, _computers, _verifiers);
    }

    function test_SubmitterAccess() public {
        bytes32 computeAssignTxHash = hex"43924aa0eb3f5df644b1d3b7d755190840d44d7b89f1df471280d4f1d957c819";
        bytes32 computeCommitTxHash = hex"9949143b1cabba1079b3f15b000fcb7c030d0fdbfcfff704be1f8917d88582ef";
        bytes32 computeRootHash = hex"0000000000000000000000000000000000000000000000000000000000000000";
        ComputeManager.Signature memory signature = ComputeManager.Signature({
            s: hex"2a7f69e1c5cc5f11272fa5a2632f8c47c8039f1e19dcf739ad99adad9130fe15",
            r: hex"dac8c2a3d60d7511b008fdc854b8e8156954ff7670991151ae67c303dbc7e28e",
            r_id: 1
        });

        // Emulate the submitter calling the function
        vm.prank(submitter);
        computeManager.submitComputeCommitment(computeAssignTxHash, computeCommitTxHash, computeRootHash, signature);
        
        // Emulate a different address calling the function (should revert)
        vm.prank(computer);
        vm.expectRevert("Only submitters can call this function");
        computeManager.submitComputeCommitment(computeAssignTxHash, computeCommitTxHash, computeRootHash, signature);
    }

    function test_submitComputeCommitment() public {
        vm.prank(submitter);

        bytes32 computeAssignTxHash = hex"43924aa0eb3f5df644b1d3b7d755190840d44d7b89f1df471280d4f1d957c819";
        bytes32 computeCommitTxHash = hex"9949143b1cabba1079b3f15b000fcb7c030d0fdbfcfff704be1f8917d88582ef";
        bytes32 computeRootHash = hex"0000000000000000000000000000000000000000000000000000000000000000";
        ComputeManager.Signature memory signature = ComputeManager.Signature({
            s: hex"2a7f69e1c5cc5f11272fa5a2632f8c47c8039f1e19dcf739ad99adad9130fe15",
            r: hex"dac8c2a3d60d7511b008fdc854b8e8156954ff7670991151ae67c303dbc7e28e",
            r_id: 1
        });

        // Call the function
        computeManager.submitComputeCommitment(
            computeAssignTxHash,
            computeCommitTxHash,
            computeRootHash,
            signature
        );

        // Check that the transaction was stored in storage
        bytes32 returnedHash = computeManager.computeRootHashes(
            computeAssignTxHash,
            computeCommitTxHash
        );
        assert(computeRootHash == returnedHash);

        bool exists = computeManager.hasTx(computeAssignTxHash);
        assert(exists);

        exists = computeManager.hasTx(computeCommitTxHash);
        assert(exists);
    }

    function test_submitComputeVerification() public {
        // Send the ComputeCommitment transaction for testing purposes
        vm.prank(submitter);
        bytes32 computeAssignTxHash = hex"43924aa0eb3f5df644b1d3b7d755190840d44d7b89f1df471280d4f1d957c819";
        bytes32 computeCommitTxHash = hex"9949143b1cabba1079b3f15b000fcb7c030d0fdbfcfff704be1f8917d88582ef";
        bytes32 computeRootHash = hex"0000000000000000000000000000000000000000000000000000000000000000";

        ComputeManager.Signature memory sig0 = ComputeManager.Signature({
            s: hex"2a7f69e1c5cc5f11272fa5a2632f8c47c8039f1e19dcf739ad99adad9130fe15",
            r: hex"dac8c2a3d60d7511b008fdc854b8e8156954ff7670991151ae67c303dbc7e28e",
            r_id: 1
        });
        computeManager.submitComputeCommitment(
            computeAssignTxHash,
            computeCommitTxHash,
            computeRootHash,
            sig0
        );

        // Check that the transaction was stored in storage
        bytes32 returnedHash = computeManager.computeRootHashes(computeAssignTxHash, computeCommitTxHash);
        assert(computeRootHash == returnedHash);

        // Call the function
        vm.prank(submitter);
        bytes32 computeVerifyTxHash = hex"042a89a8fa63d2af0dbb5248e72c0094b640285d78ef262931ab1550e6e1a4d0";
        ComputeManager.Signature memory sig1 = ComputeManager.Signature({
            s: hex"75f3cab53d46d1eb00ceaee6525bbece17878ca9ed8caf6796b969d78329cc92",
            r: hex"f293b710791ceb69d1317ebc0d8952005fc186a2a363bc74004771f183d1d8d5",
            r_id: 1
        });
        computeManager.submitComputeVerification(computeVerifyTxHash, computeAssignTxHash, sig1);

        // Assert the expected behavior
        bool exists = computeManager.hasTx(
            hex"042a89a8fa63d2af0dbb5248e72c0094b640285d78ef262931ab1550e6e1a4d0"
        );
        assert(exists);
    }
}
