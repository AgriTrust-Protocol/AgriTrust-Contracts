// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import {GovernorQuadratic, IERC20Governance} from "../contracts/governance/GovernorQuadratic.sol";
import {VotingPower} from "../contracts/governance/VotingPower.sol";

contract MockToken is IERC20Governance {
    string public name = "AgriTrust Governance";
    string public symbol = "AGT";
    uint8 public decimals = 18;
    uint256 public override totalSupply;
    mapping(address => uint256) public override balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    function mint(address to, uint256 amount) external {
        balanceOf[to] += amount;
        totalSupply += amount;
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        return true;
    }

    function transfer(address to, uint256 amount) external override returns (bool) {
        balanceOf[msg.sender] -= amount;
        balanceOf[to] += amount;
        return true;
    }

    function transferFrom(address from, address to, uint256 amount) external override returns (bool) {
        uint256 allowed = allowance[from][msg.sender];
        if (allowed != type(uint256).max) allowance[from][msg.sender] = allowed - amount;
        balanceOf[from] -= amount;
        balanceOf[to] += amount;
        return true;
    }
}

contract QuadraticHarness {
    function voteCost(uint256 votes) external pure returns (uint256) {
        return VotingPower.voteCost(votes);
    }

    function maxVotes(uint256 balance) external pure returns (uint256) {
        return VotingPower.maxQuadraticVotes(balance);
    }
}

contract GovernorQuadraticTest is Test {
    MockToken token;
    GovernorQuadratic governor;
    QuadraticHarness harness;

    address proposer = address(0xA11CE);
    address voterOne = address(0xB0B);
    address voterTwo = address(0xCAFE);
    address voterThree = address(0xDAD);

    function setUp() public {
        token = new MockToken();
        governor = new GovernorQuadratic(token, 7 days);
        harness = new QuadraticHarness();
        token.mint(voterOne, 100);
        token.mint(voterTwo, 400);
        token.mint(voterThree, 900);
    }

    function testQuadraticCostCurveForThreeVoters() public {
        assertEq(harness.voteCost(10), 100);
        assertEq(harness.voteCost(20), 400);
        assertEq(harness.voteCost(30), 900);
        assertEq(harness.maxVotes(token.balanceOf(voterOne)), 10);
        assertEq(harness.maxVotes(token.balanceOf(voterTwo)), 20);
        assertEq(harness.maxVotes(token.balanceOf(voterThree)), 30);
    }

    function testCreateAmendAndVoteLatestVersion() public {
        vm.prank(proposer);
        uint256 proposalId = governor.createProposal(GovernorQuadratic.ProposalType.TextProposal, "v1", address(0), 0, "");
        vm.prank(proposer);
        governor.amendProposal(proposalId, "v2", address(0), 0, "");
        vm.prank(proposer);
        governor.startVoting(proposalId);

        vm.startPrank(voterTwo);
        token.approve(address(governor), 400);
        governor.castVote(proposalId, true, 20, 52);
        vm.stopPrank();

        assertTrue(governor.hasVoted(proposalId, voterTwo));
        assertEq(uint8(governor.state(proposalId)), uint8(GovernorQuadratic.ProposalState.Voting));
    }
}
