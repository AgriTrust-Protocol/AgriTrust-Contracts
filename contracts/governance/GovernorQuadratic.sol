// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ProposalManager} from "./ProposalManager.sol";
import {VotingPower} from "./VotingPower.sol";
import {QuorumCalculator} from "./QuorumCalculator.sol";
import {TimeLock} from "./TimeLock.sol";

interface IERC20Governance {
    function balanceOf(address account) external view returns (uint256);
    function totalSupply() external view returns (uint256);
    function transferFrom(address from, address to, uint256 amount) external returns (bool);
    function transfer(address to, uint256 amount) external returns (bool);
}

contract GovernorQuadratic is ProposalManager, TimeLock {
    using VotingPower for uint256;

    IERC20Governance public immutable token;
    uint64 public immutable votingPeriod;

    mapping(uint256 => mapping(address => bool)) public hasVoted;
    mapping(uint256 => mapping(address => uint256)) public lockedCost;
    mapping(uint256 => address[]) private voters;

    event VoteCast(uint256 indexed proposalId, address indexed voter, bool support, uint256 votes, uint256 cost, uint256 votingPower);

    constructor(IERC20Governance votingToken, uint64 votingPeriodSeconds) {
        require(address(votingToken) != address(0), "zero token");
        token = votingToken;
        votingPeriod = votingPeriodSeconds;
    }

    receive() external payable {}

    function createProposal(ProposalType proposalType, string calldata description, address target, uint256 value, bytes calldata data) external returns (uint256) {
        return _createProposal(proposalType, description, target, value, data);
    }

    function amendProposal(uint256 proposalId, string calldata description, address target, uint256 value, bytes calldata data) external {
        _amendProposal(proposalId, description, target, value, data);
    }

    function startVoting(uint256 proposalId) external onlyProposer(proposalId) {
        Proposal storage proposal = proposals[proposalId];
        require(proposal.state == ProposalState.Draft, "not draft");
        proposal.state = ProposalState.Voting;
        proposal.voteStart = uint64(block.timestamp);
        proposal.voteEnd = uint64(block.timestamp + votingPeriod);
        emit ProposalStateChanged(proposalId, ProposalState.Voting);
    }

    function castVote(uint256 proposalId, bool support, uint256 votes, uint256 lockDurationWeeks) external {
        require(votes > 0, "zero votes");
        Proposal storage proposal = proposals[proposalId];
        require(proposal.state == ProposalState.Voting, "not voting");
        require(block.timestamp <= proposal.voteEnd, "voting ended");
        address countedVoter = delegateOf(msg.sender, proposal.proposalType);
        require(!hasVoted[proposalId][countedVoter], "already voted");

        uint256 weightedTokens = VotingPower.timeWeightedTokens(token.balanceOf(msg.sender), lockDurationWeeks);
        require(votes <= VotingPower.maxQuadraticVotes(weightedTokens), "insufficient power");
        uint256 cost = VotingPower.voteCost(votes);
        require(token.transferFrom(msg.sender, address(this), cost), "lock failed");

        hasVoted[proposalId][countedVoter] = true;
        lockedCost[proposalId][msg.sender] = cost;
        voters[proposalId].push(msg.sender);
        proposal.participation += votes;
        if (support) {
            proposal.forVotes += votes;
            proposal.forQuadraticCost += cost;
        } else {
            proposal.againstVotes += votes;
            proposal.againstQuadraticCost += cost;
        }

        emit VoteCast(proposalId, countedVoter, support, votes, cost, weightedTokens);
    }

    function queue(uint256 proposalId) external {
        Proposal storage proposal = proposals[proposalId];
        require(proposal.state == ProposalState.Voting, "not voting");
        require(block.timestamp > proposal.voteEnd, "voting active");
        require(QuorumCalculator.hasQuadraticQuorum(proposal.forVotes, proposal.againstVotes, token.totalSupply()), "quorum not met");
        require(proposal.forVotes > proposal.againstVotes, "proposal defeated");
        proposal.state = ProposalState.Queued;
        proposal.queuedAt = uint64(block.timestamp);
        _refundVoters(proposalId);
        emit ProposalQueued(proposalId, uint64(block.timestamp + TIMELOCK_DELAY));
        emit ProposalStateChanged(proposalId, ProposalState.Queued);
    }

    function execute(uint256 proposalId) external payable {
        Proposal storage proposal = proposals[proposalId];
        require(proposal.state == ProposalState.Queued, "not queued");
        require(block.timestamp >= proposal.queuedAt + TIMELOCK_DELAY, "timelocked");
        ProposalVersion memory version = latestVersion(proposalId);
        proposal.state = ProposalState.Executed;
        _executeCall(version.target, version.value, version.data);
        emit ProposalExecuted(proposalId);
        emit ProposalStateChanged(proposalId, ProposalState.Executed);
    }

    function cancel(uint256 proposalId) external onlyProposer(proposalId) {
        Proposal storage proposal = proposals[proposalId];
        require(proposal.state == ProposalState.Queued, "not queued");
        require(block.timestamp < proposal.queuedAt + TIMELOCK_DELAY, "delay elapsed");
        proposal.state = ProposalState.Cancelled;
        emit ProposalCancelled(proposalId);
        emit ProposalStateChanged(proposalId, ProposalState.Cancelled);
    }

    function state(uint256 proposalId) external view returns (ProposalState) { return proposals[proposalId].state; }

    function _refundVoters(uint256 proposalId) private {
        address[] storage proposalVoters = voters[proposalId];
        for (uint256 i; i < proposalVoters.length; ++i) {
            address voter = proposalVoters[i];
            uint256 amount = lockedCost[proposalId][voter];
            if (amount != 0) {
                lockedCost[proposalId][voter] = 0;
                require(token.transfer(voter, amount), "refund failed");
            }
        }
    }
}
