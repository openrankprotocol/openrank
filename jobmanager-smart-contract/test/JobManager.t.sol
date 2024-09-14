// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import {JobManager} from "../src/JobManager.sol";

contract JobManagerTest is Test {
    JobManager public jobManager;
    
    address public blockBuilder;    
    address public computer;
    address public verifier;

    function setUp() public {
        // Get the addresses from the default test wallets
        bytes20 pk_bytes = hex"a94f5374fce5edbc8e2a8697c15331677e6ebf0b";
        blockBuilder = address(pk_bytes);
        address[] memory _blockBuilders = new address[](1);
        _blockBuilders[0] = blockBuilder;

        pk_bytes = hex"13978aee95f38490e9769c39b2773ed763d9cd5f";
        computer = address(pk_bytes);
        address[] memory _computers = new address[](1);
        _computers[0] = computer;

        pk_bytes = hex"cd2a3d9f938e13cd947ec05abc7fe734df8dd826";
        verifier = address(pk_bytes);
        address[] memory _verifiers = new address[](1);
        _verifiers[0] = verifier;

        jobManager = new JobManager(_blockBuilders, _computers, _verifiers);
    }

    function test_decodeJobRunRequest() public view {
        // JobRunRequest::new(DomainHash::from(1), 2, Hash::default());
        bytes memory data = hex"e5c10102e1a00000000000000000000000000000000000000000000000000000000000000000";

        JobManager.JobRunRequest memory jobRunRequest = jobManager.decodeJobRunRequest(data);

        assertEq(jobRunRequest.domain_id, 1);
        assertEq(jobRunRequest.block_height, 2);
        assertEq(jobRunRequest.job_id, hex"0000000000000000000000000000000000000000000000000000000000000000");
    }

    function test_decodeJobRunAssignment() public view {
        // JobRunAssignment::new(TxHash::default(), Address::default(), Address::default());
        bytes memory data = hex"f84ee1a00000000000000000000000000000000000000000000000000000000000000000d5940000000000000000000000000000000000000000d5940000000000000000000000000000000000000000";

        JobManager.JobRunAssignment memory jobRunAssignment = jobManager.decodeJobRunAssignment(data);

        assertEq(jobRunAssignment.job_run_request_tx_hash, hex"0000000000000000000000000000000000000000000000000000000000000000");
        assertEq(jobRunAssignment.assigned_compute_node, hex"0000000000000000000000000000000000000000");
        assertEq(jobRunAssignment.assigned_verifier_node, hex"0000000000000000000000000000000000000000");
    }

    function test_decodeCreateCommitment() public view {
        // CreateCommitment::default();
        bytes memory data = hex"f869e1a00000000000000000000000000000000000000000000000000000000000000000e1a00000000000000000000000000000000000000000000000000000000000000000e1a00000000000000000000000000000000000000000000000000000000000000000c0c0c0";

        JobManager.CreateCommitment memory createCommitment = jobManager.decodeCreateCommitment(data);

        assertEq(createCommitment.job_run_assignment_tx_hash, hex"0000000000000000000000000000000000000000000000000000000000000000");
        assertEq(createCommitment.lt_root_hash, hex"0000000000000000000000000000000000000000000000000000000000000000");
        assertEq(createCommitment.compute_root_hash, hex"0000000000000000000000000000000000000000000000000000000000000000");

        assertEq(createCommitment.scores_tx_hashes.length, 0);
        assertEq(createCommitment.new_trust_tx_hashes.length, 0);
        assertEq(createCommitment.new_seed_tx_hashes.length, 0);

        // CreateCommitment::default_with(TxHash::default(), Hash::default(), Hash::default(), vec![TxHash::default(), TxHash::default()]);
        bytes memory data1 = hex"f8aee1a00000000000000000000000000000000000000000000000000000000000000000e1a00000000000000000000000000000000000000000000000000000000000000000e1a00000000000000000000000000000000000000000000000000000000000000000f844e1a00000000000000000000000000000000000000000000000000000000000000000e1a00000000000000000000000000000000000000000000000000000000000000000c0c0";

        JobManager.CreateCommitment memory createCommitment1 = jobManager.decodeCreateCommitment(data1);

        assertEq(createCommitment1.job_run_assignment_tx_hash, hex"0000000000000000000000000000000000000000000000000000000000000000");
        assertEq(createCommitment1.lt_root_hash, hex"0000000000000000000000000000000000000000000000000000000000000000");
        assertEq(createCommitment1.compute_root_hash, hex"0000000000000000000000000000000000000000000000000000000000000000");

        assertEq(createCommitment1.scores_tx_hashes.length, 2);
        assertEq(createCommitment1.new_trust_tx_hashes.length, 0);
        assertEq(createCommitment1.new_seed_tx_hashes.length, 0);
    }

    function test_decodeJobVerification() public view {
        // JobVerification::default();
        bytes memory data = hex"e3e1a0000000000000000000000000000000000000000000000000000000000000000001";

        JobManager.JobVerification memory jobVerification = jobManager.decodeJobVerification(data);

        assertEq(jobVerification.job_run_assignment_tx_hash, hex"0000000000000000000000000000000000000000000000000000000000000000");
        assertEq(jobVerification.verification_result, true);
    }

    function test_sendJobRunRequest() public {
        // Call the function 
        JobManager.OpenrankTx memory transaction = JobManager.OpenrankTx({
            nonce: 0,
            from: hex"0000000000000000000000000000000000000000",
            to: hex"0000000000000000000000000000000000000000",
            kind: JobManager.TxKind.JobRunRequest,
            body: hex"e5c18080e1a00000000000000000000000000000000000000000000000000000000000000000",
            signature: JobManager.Signature({
                s: hex"187e6de032ceea6cc1c879fc7a52ec274c93b2be19b6d4fb11bcb806db2f3f4c",
                r: hex"02ca805e1940a90fc3df7b51f66abc4d55cc3866a751b6110e9f2e3c962182b3",
                r_id: 1
            }),
            sequence_number: 0
        });

        jobManager.sendJobRunRequest(transaction);

        // Check that the transaction was stored in storage
    }

    function test_submitJobRunAssignment() public {
        // Send JobRunRequest 
        JobManager.OpenrankTx memory t0 = JobManager.OpenrankTx({
            nonce: 0,
            from: hex"0000000000000000000000000000000000000000",
            to: hex"0000000000000000000000000000000000000000",
            kind: JobManager.TxKind.JobRunRequest,
            body: hex"e5c18080e1a00000000000000000000000000000000000000000000000000000000000000000",
            signature: JobManager.Signature({
                s: hex"187e6de032ceea6cc1c879fc7a52ec274c93b2be19b6d4fb11bcb806db2f3f4c",
                r: hex"02ca805e1940a90fc3df7b51f66abc4d55cc3866a751b6110e9f2e3c962182b3",
                r_id: 1
            }),
            sequence_number: 0
        });
        jobManager.sendJobRunRequest(t0);

        // Call the function
        JobManager.OpenrankTx memory transaction = JobManager.OpenrankTx({
            nonce: 0,
            from: hex"0000000000000000000000000000000000000000",
            to: hex"0000000000000000000000000000000000000000",
            kind: JobManager.TxKind.JobRunAssignment,
            body: hex"f84ee1a0159232adb52f1e32121d4b111339a6959caccfd86ecf2cc7ab8ef4388d314646d59413978aee95f38490e9769c39b2773ed763d9cd5fd594cd2a3d9f938e13cd947ec05abc7fe734df8dd826",
            signature: JobManager.Signature({
                s: hex"1de2b58056c12cceae2229750a3975ec10cb56ac1c592bc91422c82fd540bcf0",
                r: hex"7320fdb17e892a361d979f074cb9f9949764796fb68de9f20b7aff45bafe0b8a",
                r_id: 1
            }),
            sequence_number: 0
        });

        jobManager.submitJobRunAssignment(transaction);

        // Check that the transaction was stored in storage
    }


    function test_submitCreateCommitment() public {
        // Send JobRunRequest 
        JobManager.OpenrankTx memory t0 = JobManager.OpenrankTx({
            nonce: 0,
            from: hex"0000000000000000000000000000000000000000",
            to: hex"0000000000000000000000000000000000000000",
            kind: JobManager.TxKind.JobRunRequest,
            body: hex"e5c18080e1a00000000000000000000000000000000000000000000000000000000000000000",
            signature: JobManager.Signature({
                s: hex"187e6de032ceea6cc1c879fc7a52ec274c93b2be19b6d4fb11bcb806db2f3f4c",
                r: hex"02ca805e1940a90fc3df7b51f66abc4d55cc3866a751b6110e9f2e3c962182b3",
                r_id: 1
            }),
            sequence_number: 0
        });
        jobManager.sendJobRunRequest(t0);

        // Send JobRunAssignment
        JobManager.OpenrankTx memory t1 = JobManager.OpenrankTx({
            nonce: 0,
            from: hex"0000000000000000000000000000000000000000",
            to: hex"0000000000000000000000000000000000000000",
            kind: JobManager.TxKind.JobRunAssignment,
            body: hex"f84ee1a0159232adb52f1e32121d4b111339a6959caccfd86ecf2cc7ab8ef4388d314646d59413978aee95f38490e9769c39b2773ed763d9cd5fd594cd2a3d9f938e13cd947ec05abc7fe734df8dd826",
            signature: JobManager.Signature({
                s: hex"1de2b58056c12cceae2229750a3975ec10cb56ac1c592bc91422c82fd540bcf0",
                r: hex"7320fdb17e892a361d979f074cb9f9949764796fb68de9f20b7aff45bafe0b8a",
                r_id: 1
            }),
            sequence_number: 0
        });

        jobManager.submitJobRunAssignment(t1);


        // Call the function
        JobManager.OpenrankTx memory transaction = JobManager.OpenrankTx({
            nonce: 0,
            from: hex"0000000000000000000000000000000000000000",
            to: hex"0000000000000000000000000000000000000000",
            kind: JobManager.TxKind.CreateCommitment,
            body: hex"f869e1a043924aa0eb3f5df644b1d3b7d755190840d44d7b89f1df471280d4f1d957c819e1a00000000000000000000000000000000000000000000000000000000000000000e1a00000000000000000000000000000000000000000000000000000000000000000c0c0c0",
            signature: JobManager.Signature({
                s: hex"2a7f69e1c5cc5f11272fa5a2632f8c47c8039f1e19dcf739ad99adad9130fe15",
                r: hex"dac8c2a3d60d7511b008fdc854b8e8156954ff7670991151ae67c303dbc7e28e",
                r_id: 1
            }),
            sequence_number: 0
        });
        jobManager.submitCreateCommitment(transaction);

        // Check that the transaction was stored in storage
    }


    function test_submitJobVerification() public {
         // Send JobRunRequest 
        JobManager.OpenrankTx memory t0 = JobManager.OpenrankTx({
            nonce: 0,
            from: hex"0000000000000000000000000000000000000000",
            to: hex"0000000000000000000000000000000000000000",
            kind: JobManager.TxKind.JobRunRequest,
            body: hex"e5c18080e1a00000000000000000000000000000000000000000000000000000000000000000",
            signature: JobManager.Signature({
                s: hex"187e6de032ceea6cc1c879fc7a52ec274c93b2be19b6d4fb11bcb806db2f3f4c",
                r: hex"02ca805e1940a90fc3df7b51f66abc4d55cc3866a751b6110e9f2e3c962182b3",
                r_id: 1
            }),
            sequence_number: 0
        });
        jobManager.sendJobRunRequest(t0);

        // Send JobRunAssignment
        JobManager.OpenrankTx memory t1 = JobManager.OpenrankTx({
            nonce: 0,
            from: hex"0000000000000000000000000000000000000000",
            to: hex"0000000000000000000000000000000000000000",
            kind: JobManager.TxKind.JobRunAssignment,
            body: hex"f84ee1a0159232adb52f1e32121d4b111339a6959caccfd86ecf2cc7ab8ef4388d314646d59413978aee95f38490e9769c39b2773ed763d9cd5fd594cd2a3d9f938e13cd947ec05abc7fe734df8dd826",
            signature: JobManager.Signature({
                s: hex"1de2b58056c12cceae2229750a3975ec10cb56ac1c592bc91422c82fd540bcf0",
                r: hex"7320fdb17e892a361d979f074cb9f9949764796fb68de9f20b7aff45bafe0b8a",
                r_id: 1
            }),
            sequence_number: 0
        });

        jobManager.submitJobRunAssignment(t1);


        // Send CreateCommitment
        JobManager.OpenrankTx memory t2 = JobManager.OpenrankTx({
            nonce: 0,
            from: hex"0000000000000000000000000000000000000000",
            to: hex"0000000000000000000000000000000000000000",
            kind: JobManager.TxKind.CreateCommitment,
            body: hex"f869e1a043924aa0eb3f5df644b1d3b7d755190840d44d7b89f1df471280d4f1d957c819e1a00000000000000000000000000000000000000000000000000000000000000000e1a00000000000000000000000000000000000000000000000000000000000000000c0c0c0",
            signature: JobManager.Signature({
                s: hex"2a7f69e1c5cc5f11272fa5a2632f8c47c8039f1e19dcf739ad99adad9130fe15",
                r: hex"dac8c2a3d60d7511b008fdc854b8e8156954ff7670991151ae67c303dbc7e28e",
                r_id: 1
            }),
            sequence_number: 0
        });
        jobManager.submitCreateCommitment(t2);

        // Call the function
        JobManager.OpenrankTx memory transaction = JobManager.OpenrankTx({
            nonce: 0,
            from: hex"0000000000000000000000000000000000000000",
            to: hex"0000000000000000000000000000000000000000",
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

    }
}
