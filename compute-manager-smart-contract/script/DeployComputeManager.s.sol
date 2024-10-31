// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script, console} from "forge-std/Script.sol";
import {ComputeManager} from "../src/ComputeManager.sol";

contract ComputeManagerScript is Script {

    function run() public {
        // Load environment variables (such as private key, etc.)
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");

        // Start broadcasting the transaction
        vm.startBroadcast(deployerPrivateKey);

        address[] memory submitters = new address[](3);
        submitters[0] = address(bytes20(bytes(hex"b79aafc95c8866e65ed51a7856e75587feb481ff")));
        submitters[1] = address(bytes20(bytes(hex"4d40f98096410aa4235292343cc5fa559afcc780")));
        submitters[2] = address(bytes20(bytes(hex"4A2C2501d26ff3aC30bd0AAbfb48471B411852A1")));

        address[] memory computers = new address[](2);
        computers[0] = address(bytes20(bytes(hex"77248e33d7f82973aa31ff3c7cd61237dce78cd1")));
        computers[1] = address(bytes20(bytes(hex"e1ab19bdcd7293cd47493c6a935fa48690fad222")));

        address[] memory verifiers = new address[](2);
        verifiers[0] = address(bytes20(bytes(hex"e62ace9c2512b2ad4ff50959b7a6a327e8befc93")));
        verifiers[1] = address(bytes20(bytes(hex"40a28b7ca8509395b93165bea28ba614b4a6bdd9")));

        ComputeManager computeManager = new ComputeManager(submitters, computers, verifiers);

        // Print the contract address
        console.log("ComputeManager contract deployed at:", address(computeManager));

        vm.stopBroadcast();
    }
}
