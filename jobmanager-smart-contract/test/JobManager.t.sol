// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import {JobManager} from "../src/JobManager.sol";

contract JobManagerTest is Test {
    JobManager public jobManager;
    
    address public verifier;

    function setUp() public {
        // Use the address which are the same as Rust tests.
        // (See [project root]/common/src/txs.rs)
        bytes20 pk_bytes = hex"cd2a3d9f938e13cd947ec05abc7fe734df8dd826";
        verifier = address(pk_bytes);
        address[] memory _verifiers = new address[](1);
        _verifiers[0] = verifier;

        jobManager = new JobManager(_verifiers);
    }

    function test_submitCreateCommitment() public {
        bytes32 txHash = hex"2a7f69e1c5cc5f11272fa5a2632f8c47c8039f1e19dcf739ad99adad9130fe15";
        bytes32 computeRootHash = hex"dac8c2a3d60d7511b008fdc854b8e8156954ff7670991151ae67c303dbc7e28e";

        // Call the function
        jobManager.submitCreateCommitment(txHash, computeRootHash);

        // Check that the transaction was stored in storage
        bytes32 returnedHash = jobManager.computeRootHashes(txHash);
        assert(computeRootHash == returnedHash);

        bool exists = jobManager.hasTxHash(txHash);
        assert(exists);
    }


    function test_submitJobVerification() public {
        bytes32 txHash = hex"042a89a8fa63d2af0dbb5248e72c0094b640285d78ef262931ab1550e6e1a4d0";
        JobManager.Signature memory signature = JobManager.Signature({
            s: hex"75f3cab53d46d1eb00ceaee6525bbece17878ca9ed8caf6796b969d78329cc92",
            r: hex"f293b710791ceb69d1317ebc0d8952005fc186a2a363bc74004771f183d1d8d5",
            r_id: 1
        });
        
        // Call the function
        jobManager.submitJobVerification(txHash, signature);

        // Assert the expected behavior
        bool exists = jobManager.hasTxHash(hex"042a89a8fa63d2af0dbb5248e72c0094b640285d78ef262931ab1550e6e1a4d0");
        assert(exists);
    }
}
