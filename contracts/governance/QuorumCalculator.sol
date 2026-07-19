// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {VotingPower} from "./VotingPower.sol";

library QuorumCalculator {
    uint256 internal constant QUORUM_BPS = 400;
    uint256 internal constant BPS_DENOMINATOR = 10_000;

    function quorum(uint256 totalSupply) internal pure returns (uint256) {
        return (totalSupply * QUORUM_BPS) / BPS_DENOMINATOR;
    }

    function hasQuadraticQuorum(uint256 forQuadraticVotes, uint256 againstQuadraticVotes, uint256 totalSupply) internal pure returns (bool) {
        return forQuadraticVotes + againstQuadraticVotes >= quorum(totalSupply);
    }
}
