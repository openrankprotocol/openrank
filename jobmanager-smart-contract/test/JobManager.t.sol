// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import {JobManager} from "../src/JobManager.sol";

contract JobManagerTest is Test {
    JobManager public jobManager;
    
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

        jobManager = new JobManager(_computers, _verifiers);
    }

    function test_submitCreateCommitment() public {
        bytes32 jobRunAssignTxHash = hex"43924aa0eb3f5df644b1d3b7d755190840d44d7b89f1df471280d4f1d957c819";
        bytes32 createCommitTxHash = hex"9949143b1cabba1079b3f15b000fcb7c030d0fdbfcfff704be1f8917d88582ef";
        bytes32 computeRootHash = hex"0000000000000000000000000000000000000000000000000000000000000000";

        JobManager.Signature memory signature = JobManager.Signature({
            s: hex"2a7f69e1c5cc5f11272fa5a2632f8c47c8039f1e19dcf739ad99adad9130fe15",
            r: hex"dac8c2a3d60d7511b008fdc854b8e8156954ff7670991151ae67c303dbc7e28e",
            r_id: 1
        });

        // Call the function
        jobManager.submitCreateCommitment(jobRunAssignTxHash, createCommitTxHash, computeRootHash, signature);

        // Check that the transaction was stored in storage
        bytes32 returnedHash = jobManager.computeRootHashes(jobRunAssignTxHash, createCommitTxHash);
        assert(computeRootHash == returnedHash);

        bool exists = jobManager.hasTx(jobRunAssignTxHash);
        assert(exists);

        exists = jobManager.hasTx(createCommitTxHash);
        assert(exists);
    }


    function test_submitJobVerification() public {
        bytes20 data = hex"0000000000000000000000000000000000000000";
        address from = address(data);
        address to = address(data);

        // Send the CreateCommitment transaction for testing purposes
        bytes32 jobRunAssignTxHash = hex"43924aa0eb3f5df644b1d3b7d755190840d44d7b89f1df471280d4f1d957c819";
        bytes32 createCommitTxHash = hex"9949143b1cabba1079b3f15b000fcb7c030d0fdbfcfff704be1f8917d88582ef";
        bytes32 computeRootHash = hex"0000000000000000000000000000000000000000000000000000000000000000";

        JobManager.Signature memory signature = JobManager.Signature({
            s: hex"2a7f69e1c5cc5f11272fa5a2632f8c47c8039f1e19dcf739ad99adad9130fe15",
            r: hex"dac8c2a3d60d7511b008fdc854b8e8156954ff7670991151ae67c303dbc7e28e",
            r_id: 1
        });
        jobManager.submitCreateCommitment(jobRunAssignTxHash, createCommitTxHash, computeRootHash, signature);
        
        // Call the function
        JobManager.OpenrankTx memory transaction = JobManager.OpenrankTx({
            nonce: 0,
            from: from,
            to: to,
            kind: JobManager.TxKind.JobVerification,
            body: hex"e3e1a043924aa0eb3f5df644b1d3b7d755190840d44d7b89f1df471280d4f1d957c81901",
            signature: JobManager.Signature({
                s: hex"75f3cab53d46d1eb00ceaee6525bbece17878ca9ed8caf6796b969d78329cc92",
                r: hex"f293b710791ceb69d1317ebc0d8952005fc186a2a363bc74004771f183d1d8d5",
                r_id: 1
            }),
            sequence_number: 0
        });
        jobManager.submitJobVerification(transaction);

        // Assert the expected behavior
        bool exists = jobManager.hasTx(hex"042a89a8fa63d2af0dbb5248e72c0094b640285d78ef262931ab1550e6e1a4d0");
        assert(exists);
    }
}
