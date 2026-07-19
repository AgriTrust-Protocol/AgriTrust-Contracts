// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

abstract contract Delegation {
    enum ProposalType { TreasurySpend, ParameterChange, ContractUpgrade, TextProposal }

    mapping(address => mapping(ProposalType => address)) private _delegates;

    event DelegateChanged(address indexed delegator, ProposalType indexed proposalType, address indexed delegatee);

    function delegate(ProposalType proposalType, address delegatee) external {
        require(delegatee != msg.sender, "self delegation");
        _delegates[msg.sender][proposalType] = delegatee;
        emit DelegateChanged(msg.sender, proposalType, delegatee);
    }

    function delegateOf(address voter, ProposalType proposalType) public view returns (address) {
        address delegatee = _delegates[voter][proposalType];
        return delegatee == address(0) ? voter : delegatee;
    }
}
