// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script, console} from "forge-std/Script.sol";
import {JobManager} from "../src/JobManager.sol";

contract JobManagerScript is Script {
    JobManager public jobManager;

    address public blockBuilder1;
    address public blockBuilder2;
    address public blockBuilder3;
    
    address public computer1;
    address public computer2;
    address public computer3;

    address public verifier1;
    address public verifier2;
    address public verifier3;

    function setUp() public {
        // Get the addresses from the default test wallets
        blockBuilder1 = vm.addr(1);
        blockBuilder2 = vm.addr(2);
        blockBuilder3 = vm.addr(3);

        computer1 = vm.addr(4);
        computer2 = vm.addr(5);
        computer3 = vm.addr(6);

        verifier1 = vm.addr(7);
        verifier2 = vm.addr(8);
        verifier3 = vm.addr(9);
    }

    function run() public {
        vm.startBroadcast();

        address[] memory _blockBuilders = new address[](3);
        _blockBuilders[0] = blockBuilder1;
        _blockBuilders[1] = blockBuilder2;
        _blockBuilders[2] = blockBuilder3;

        address[] memory _computers = new address[](3);
        _computers[0] = computer1;
        _computers[1] = computer2;
        _computers[2] = computer3;

        address[] memory _verifiers = new address[](3);
        _verifiers[0] = verifier1;
        _verifiers[1] = verifier2;
        _verifiers[2] = verifier3;
        jobManager = new JobManager(_blockBuilders, _computers, _verifiers);

        vm.stopBroadcast();
    }
}
