// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Delegation} from "./Delegation.sol";

abstract contract ProposalManager is Delegation {
    enum ProposalState { Draft, Voting, Queued, Executed, Cancelled }

    struct ProposalVersion {
        string description;
        address target;
        uint256 value;
        bytes data;
        uint64 createdAt;
    }

    struct Proposal {
        address proposer;
        ProposalType proposalType;
        ProposalState state;
        uint64 voteStart;
        uint64 voteEnd;
        uint64 queuedAt;
        uint256 forVotes;
        uint256 againstVotes;
        uint256 forQuadraticCost;
        uint256 againstQuadraticCost;
        uint256 participation;
        uint32 version;
    }

    mapping(uint256 => Proposal) internal proposals;
    mapping(uint256 => mapping(uint32 => ProposalVersion)) internal proposalVersions;
    uint256 internal nextProposalId = 1;

    event ProposalCreated(uint256 indexed proposalId, address indexed proposer, ProposalType proposalType, uint32 version);
    event ProposalAmended(uint256 indexed proposalId, uint32 version);
    event ProposalStateChanged(uint256 indexed proposalId, ProposalState state);

    modifier onlyProposer(uint256 proposalId) {
        require(proposals[proposalId].proposer == msg.sender, "not proposer");
        _;
    }

    function _createProposal(ProposalType proposalType, string calldata description, address target, uint256 value, bytes calldata data) internal returns (uint256 proposalId) {
        proposalId = nextProposalId++;
        Proposal storage proposal = proposals[proposalId];
        proposal.proposer = msg.sender;
        proposal.proposalType = proposalType;
        proposal.state = ProposalState.Draft;
        proposal.version = 1;
        proposalVersions[proposalId][1] = ProposalVersion(description, target, value, data, uint64(block.timestamp));
        emit ProposalCreated(proposalId, msg.sender, proposalType, 1);
    }

    function _amendProposal(uint256 proposalId, string calldata description, address target, uint256 value, bytes calldata data) internal onlyProposer(proposalId) {
        Proposal storage proposal = proposals[proposalId];
        require(proposal.state == ProposalState.Draft, "not draft");
        uint32 version = proposal.version + 1;
        proposal.version = version;
        proposalVersions[proposalId][version] = ProposalVersion(description, target, value, data, uint64(block.timestamp));
        emit ProposalAmended(proposalId, version);
    }

    function latestVersion(uint256 proposalId) public view returns (ProposalVersion memory) {
        Proposal storage proposal = proposals[proposalId];
        require(proposal.proposer != address(0), "missing proposal");
        return proposalVersions[proposalId][proposal.version];
    }
}
