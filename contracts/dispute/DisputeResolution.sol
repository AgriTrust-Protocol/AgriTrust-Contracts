// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./IERC20.sol";
import "./JuryManager.sol";
import "./Voting.sol";
import "./AppealManager.sol";
import "./EscrowEnforcer.sol";

/**
 * @title DisputeResolution
 * @notice Master dispute resolution orchestrator implementing the full lifecycle:
 *         Filed -> Evidence -> JurySelection -> Voting -> Ruling -> Appeal -> Final
 */
contract DisputeResolution {
    enum DisputeState {
        Filed,
        Evidence,
        JurySelection,
        Voting,
        Ruling,
        Appeal,
        Final
    }

    struct DisputeCase {
        uint256 disputeId;
        address plaintiff;
        address defendant;
        uint256 escrowAmount;
        DisputeState state;
        uint256 currentRound;
        Voting.VoteChoice currentRuling;
        address finalWinner;
        string[] evidenceList;
    }

    IERC20 public immutable agriToken;
    JuryManager public juryManager;
    Voting public votingContract;
    AppealManager public appealManager;
    EscrowEnforcer public escrowEnforcer;

    uint256 public disputeCounter;
    mapping(uint256 => DisputeCase) public disputes;

    event DisputeFiled(uint256 indexed disputeId, address indexed plaintiff, address indexed defendant, uint256 escrowAmount);
    event EvidenceSubmitted(uint256 indexed disputeId, address indexed submitter, string evidenceHash);
    event DisputeStateChanged(uint256 indexed disputeId, DisputeState oldState, DisputeState newState);
    event RulingDecided(uint256 indexed disputeId, uint256 round, Voting.VoteChoice ruling, address leadingWinner);
    event DisputeFinalized(uint256 indexed disputeId, address indexed finalWinner, uint256 escrowAmount);

    constructor(address _agriToken) {
        require(_agriToken != address(0), "Invalid token");
        agriToken = IERC20(_agriToken);
    }

    /**
     * @notice Sets auxiliary contract addresses (called by deployer).
     */
    function setContracts(
        address _juryManager,
        address _votingContract,
        address _appealManager,
        address _escrowEnforcer
    ) external {
        require(address(juryManager) == address(0), "Contracts already set");
        juryManager = JuryManager(_juryManager);
        votingContract = Voting(_votingContract);
        appealManager = AppealManager(_appealManager);
        escrowEnforcer = EscrowEnforcer(_escrowEnforcer);
    }

    /**
     * @notice Files a new supply chain dispute with an initial IPFS evidence hash and locked escrow.
     */
    function fileDispute(
        address defendant,
        uint256 escrowAmount,
        string calldata evidenceHash
    ) external returns (uint256) {
        require(defendant != address(0) && defendant != msg.sender, "Invalid defendant");
        require(escrowAmount > 0, "Escrow amount must be > 0");

        disputeCounter++;
        uint256 disputeId = disputeCounter;

        DisputeCase storage c = disputes[disputeId];
        c.disputeId = disputeId;
        c.plaintiff = msg.sender;
        c.defendant = defendant;
        c.escrowAmount = escrowAmount;
        c.state = DisputeState.Filed;
        c.currentRound = 0;
        c.currentRuling = Voting.VoteChoice.None;
        c.evidenceList.push(evidenceHash);

        escrowEnforcer.lockEscrow(disputeId, msg.sender, defendant, escrowAmount);

        emit DisputeFiled(disputeId, msg.sender, defendant, escrowAmount);
        emit EvidenceSubmitted(disputeId, msg.sender, evidenceHash);

        // Move to Evidence phase
        c.state = DisputeState.Evidence;
        emit DisputeStateChanged(disputeId, DisputeState.Filed, DisputeState.Evidence);

        return disputeId;
    }

    /**
     * @notice Submits additional IPFS evidence before jury selection.
     */
    function submitEvidence(uint256 disputeId, string calldata evidenceHash) external {
        DisputeCase storage c = disputes[disputeId];
        require(c.state == DisputeState.Evidence, "Not in Evidence state");
        require(msg.sender == c.plaintiff || msg.sender == c.defendant, "Only parties can submit evidence");

        c.evidenceList.push(evidenceHash);
        emit EvidenceSubmitted(disputeId, msg.sender, evidenceHash);
    }

    /**
     * @notice Selects the panel of jurors (11 for round 0, 21 for round 1, 41 for round 2).
     */
    function selectJury(uint256 disputeId, uint256 randomnessSeed) external returns (address[] memory) {
        DisputeCase storage c = disputes[disputeId];
        require(c.state == DisputeState.Evidence || c.state == DisputeState.JurySelection, "Invalid state for jury selection");

        c.state = DisputeState.JurySelection;
        uint256 panelSize = appealManager.getJurySizeForRound(c.currentRound);

        address[] memory panel = juryManager.selectJury(disputeId, c.currentRound, panelSize, randomnessSeed);

        // Transition to Voting phase
        c.state = DisputeState.Voting;
        votingContract.startVotingRound(disputeId, c.currentRound);

        emit DisputeStateChanged(disputeId, DisputeState.JurySelection, DisputeState.Voting);
        return panel;
    }

    /**
     * @notice Tallies the vote, settles juror rewards, records ruling, and opens the 7-day appeal period.
     */
    function computeRuling(uint256 disputeId) external returns (Voting.VoteChoice) {
        DisputeCase storage c = disputes[disputeId];
        require(c.state == DisputeState.Voting, "Dispute not in Voting state");

        Voting.VoteChoice ruling = votingContract.tallyVotes(disputeId, c.currentRound);
        votingContract.settleJurorRewards(disputeId, c.currentRound);

        c.currentRuling = ruling;
        c.state = DisputeState.Ruling;
        emit DisputeStateChanged(disputeId, DisputeState.Voting, DisputeState.Ruling);

        address leadingWinner = address(0);
        if (ruling == Voting.VoteChoice.PlaintiffWin) {
            leadingWinner = c.plaintiff;
        } else if (ruling == Voting.VoteChoice.DefendantWin) {
            leadingWinner = c.defendant;
        }

        appealManager.recordRuling(disputeId, c.currentRound);
        c.state = DisputeState.Appeal;
        emit DisputeStateChanged(disputeId, DisputeState.Ruling, DisputeState.Appeal);
        emit RulingDecided(disputeId, c.currentRound, ruling, leadingWinner);

        return ruling;
    }

    /**
     * @notice Files an appeal within 7 days, paying 2x case stake and escalating jury size.
     */
    function appeal(uint256 disputeId) external {
        DisputeCase storage c = disputes[disputeId];
        require(c.state == DisputeState.Appeal, "Dispute not in Appeal state");
        require(msg.sender == c.plaintiff || msg.sender == c.defendant, "Only plaintiff or defendant can appeal");

        (uint256 newRound, ) = appealManager.processAppeal(disputeId, msg.sender, c.escrowAmount);
        c.currentRound = newRound;

        // Reset to Evidence/JurySelection for next round
        c.state = DisputeState.Evidence;
        emit DisputeStateChanged(disputeId, DisputeState.Appeal, DisputeState.Evidence);
    }

    /**
     * @notice Finalizes dispute after appeal window expires or max appeals reached, executing escrow payout.
     */
    function finalizeDispute(uint256 disputeId) external {
        DisputeCase storage c = disputes[disputeId];
        require(c.state == DisputeState.Appeal, "Dispute not ready for finalization");

        (, uint256 rulingTimestamp, , , ) = appealManager.getAppealDetails(disputeId);
        require(
            block.timestamp > rulingTimestamp + appealManager.APPEAL_WINDOW() || c.currentRound >= appealManager.MAX_APPEALS(),
            "Appeal window still open"
        );

        c.state = DisputeState.Final;
        emit DisputeStateChanged(disputeId, DisputeState.Appeal, DisputeState.Final);

        address winner;
        if (c.currentRuling == Voting.VoteChoice.PlaintiffWin) {
            winner = c.plaintiff;
        } else if (c.currentRuling == Voting.VoteChoice.DefendantWin) {
            winner = c.defendant;
        } else {
            // In case of tie, refund plaintiff
            winner = c.plaintiff;
        }

        c.finalWinner = winner;
        escrowEnforcer.enforceRuling(disputeId, winner);

        emit DisputeFinalized(disputeId, winner, c.escrowAmount);
    }

    function getDispute(uint256 disputeId) external view returns (
        address plaintiff,
        address defendant,
        uint256 escrowAmount,
        DisputeState state,
        uint256 currentRound,
        Voting.VoteChoice currentRuling,
        address finalWinner
    ) {
        DisputeCase storage c = disputes[disputeId];
        return (c.plaintiff, c.defendant, c.escrowAmount, c.state, c.currentRound, c.currentRuling, c.finalWinner);
    }

    function getEvidenceList(uint256 disputeId) external view returns (string[] memory) {
        return disputes[disputeId].evidenceList;
    }
}
