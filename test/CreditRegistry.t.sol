// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "../contracts/carbon/CarbonOracle.sol";
import "../contracts/carbon/CreditRegistry.sol";

contract CreditRegistryTest {
    function testMintRetireAndSerialTracking() public {
        CarbonOracle oracle = new CarbonOracle(address(this));
        bytes32 projectId = keccak256("project-1");
        bytes32 reportHash = keccak256("verification-report");
        bytes32 attestationId = oracle.attest(CarbonOracle.Registry.Verra, projectId, reportHash, "bafy-report");

        CreditRegistry registry = new CreditRegistry(oracle);
        uint256 id = registry.mint(address(this), projectId, "VM0042", 2026, "US-IA", 1000, "bafy-report", attestationId, block.timestamp + 3650 days);

        require(registry.balanceOf(address(this), id) == 1000, "mint balance");
        uint256 certificateId = registry.retire(id, 500, "AgriCo retirement 2026");

        require(registry.balanceOf(address(this), id) == 500, "remaining balance");
        require(registry.isSerialRetired(bytes32(id), 0), "first serial retired");
        require(registry.isSerialRetired(bytes32(id), 499), "last serial retired");
        require(!registry.isSerialRetired(bytes32(id), 500), "next serial available");
        require(registry.retirementCertificate().ownerOf(certificateId) == address(this), "certificate owner");
    }
}
