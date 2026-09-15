// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./IERC20.sol";

/**
 * @title EscrowEnforcer
 * @notice Holds escrowed dispute funds and releases them automatically based on the final ruling.
 */
contract EscrowEnforcer {
    IERC20 public immutable paymentToken;
    address public immutable disputeContract;

    struct EscrowDeposit {
        address plaintiff;
        address defendant;
        uint256 amount;
        bool locked;
        bool released;
        address beneficiary;
    }

    // disputeId => EscrowDeposit
    mapping(uint256 => EscrowDeposit) public escrows;

    event EscrowLocked(uint256 indexed disputeId, address indexed plaintiff, address indexed defendant, uint256 amount);
    event EscrowReleased(uint256 indexed disputeId, address indexed beneficiary, uint256 amount);

    modifier onlyDisputeContract() {
        require(msg.sender == disputeContract, "EscrowEnforcer: caller not dispute contract");
        _;
    }

    constructor(address _paymentToken, address _disputeContract) {
        require(_paymentToken != address(0), "Invalid token");
        paymentToken = IERC20(_paymentToken);
        disputeContract = _disputeContract;
    }

    /**
     * @notice Locks funds in escrow when a dispute is filed.
     */
    function lockEscrow(
        uint256 disputeId,
        address plaintiff,
        address defendant,
        uint256 amount
    ) external onlyDisputeContract {
        require(amount > 0, "Amount must be > 0");
        require(!escrows[disputeId].locked, "Escrow already locked");

        require(paymentToken.transferFrom(plaintiff, address(this), amount), "Escrow transfer failed");

        escrows[disputeId] = EscrowDeposit({
            plaintiff: plaintiff,
            defendant: defendant,
            amount: amount,
            locked: true,
            released: false,
            beneficiary: address(0)
        });

        emit EscrowLocked(disputeId, plaintiff, defendant, amount);
    }

    /**
     * @notice Automatically releases escrow funds to the winner upon a final, binding ruling.
     */
    function enforceRuling(uint256 disputeId, address winner) external onlyDisputeContract {
        EscrowDeposit storage deposit = escrows[disputeId];
        require(deposit.locked, "No escrow locked");
        require(!deposit.released, "Escrow already released");
        require(winner == deposit.plaintiff || winner == deposit.defendant, "Winner must be dispute party");

        deposit.released = true;
        deposit.beneficiary = winner;

        require(paymentToken.transfer(winner, deposit.amount), "Escrow release transfer failed");

        emit EscrowReleased(disputeId, winner, deposit.amount);
    }

    function getEscrowDetails(uint256 disputeId) external view returns (
        address plaintiff,
        address defendant,
        uint256 amount,
        bool locked,
        bool released,
        address beneficiary
    ) {
        EscrowDeposit storage d = escrows[disputeId];
        return (d.plaintiff, d.defendant, d.amount, d.locked, d.released, d.beneficiary);
    }
}
