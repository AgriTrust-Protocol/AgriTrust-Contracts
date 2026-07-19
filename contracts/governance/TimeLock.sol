// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

abstract contract TimeLock {
    uint64 public constant TIMELOCK_DELAY = 48 hours;

    event ProposalQueued(uint256 indexed proposalId, uint64 executeAfter);
    event ProposalExecuted(uint256 indexed proposalId);
    event ProposalCancelled(uint256 indexed proposalId);

    function _executeCall(address target, uint256 value, bytes memory data) internal {
        (bool ok, bytes memory reason) = target.call{value: value}(data);
        if (!ok) {
            if (reason.length > 0) assembly { revert(add(reason, 32), mload(reason)) }
            revert("execution failed");
        }
    }
}
