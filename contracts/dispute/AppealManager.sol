// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./IERC20.sol";

/**
 * @title AppealManager
 * @notice Manages appeals, escalating jury panels (11 -> 21 -> 41), and appeal fee staking.
 */
contract AppealManager {
    uint256 public constant APPEAL_WINDOW = 7 days;
    uint256 public constant MAX_APPEALS = 2; // 3 levels total (round 0, 1, 2)

    IERC20 public immutable agriToken;
    address public immutable disputeContract;

    struct AppealState {
        uint256 appealCount;
        uint256 rulingTimestamp;
        bool appealActive;
        address appellant;
        uint256 appealFeePaid;
    }

    // disputeId => AppealState
    mapping(uint256 => AppealState) public disputeAppeals;

    event RulingRecorded(uint256 indexed disputeId, uint256 round, uint256 timestamp, uint256 appealDeadline);
    event AppealFiled(uint256 indexed disputeId, uint256 newRound, address indexed appellant, uint256 feePaid, uint256 newJurySize);

    modifier onlyDisputeContract() {
        require(msg.sender == disputeContract, "AppealManager: caller not dispute contract");
        _;
    }

    constructor(address _agriToken, address _disputeContract) {
        require(_agriToken != address(0), "Invalid token");
        agriToken = IERC20(_agriToken);
        disputeContract = _disputeContract;
    }

    /**
     * @notice Records when a round's ruling was published to begin the 7-day appeal countdown.
     */
    function recordRuling(uint256 disputeId, uint256 round) external onlyDisputeContract {
        AppealState storage state = disputeAppeals[disputeId];
        state.rulingTimestamp = block.timestamp;
        state.appealActive = false;

        emit RulingRecorded(disputeId, round, block.timestamp, block.timestamp + APPEAL_WINDOW);
    }

    /**
     * @notice Checks whether an appeal can be filed.
     */
    function canAppeal(uint256 disputeId) external view returns (bool) {
        AppealState storage state = disputeAppeals[disputeId];
        if (state.rulingTimestamp == 0) return false;
        if (state.appealCount >= MAX_APPEALS) return false;
        if (block.timestamp > state.rulingTimestamp + APPEAL_WINDOW) return false;
        return true;
    }

    /**
     * @notice Processes an appeal, collects 2x case stake fee, and returns the next jury size.
     */
    function processAppeal(
        uint256 disputeId,
        address appellant,
        uint256 caseStake
    ) external onlyDisputeContract returns (uint256 newRound, uint256 newJurySize) {
        AppealState storage state = disputeAppeals[disputeId];
        require(state.rulingTimestamp > 0, "No ruling to appeal");
        require(state.appealCount < MAX_APPEALS, "Max appeals reached (3 levels total)");
        require(block.timestamp <= state.rulingTimestamp + APPEAL_WINDOW, "Appeal window expired");

        uint256 appealFee = caseStake * 2;
        require(agriToken.transferFrom(appellant, address(this), appealFee), "Appeal fee transfer failed");

        state.appealCount++;
        state.appealActive = true;
        state.appellant = appellant;
        state.appealFeePaid += appealFee;
        state.rulingTimestamp = 0; // reset until next ruling

        newRound = state.appealCount;
        newJurySize = getJurySizeForRound(newRound);

        emit AppealFiled(disputeId, newRound, appellant, appealFee, newJurySize);
        return (newRound, newJurySize);
    }

    function getJurySizeForRound(uint256 round) public pure returns (uint256) {
        if (round == 0) return 11;
        if (round == 1) return 21;
        if (round == 2) return 41;
        return 41;
    }

    function getAppealDetails(uint256 disputeId) external view returns (
        uint256 appealCount,
        uint256 rulingTimestamp,
        bool appealActive,
        address appellant,
        uint256 appealFeePaid
    ) {
        AppealState storage s = disputeAppeals[disputeId];
        return (s.appealCount, s.rulingTimestamp, s.appealActive, s.appellant, s.appealFeePaid);
    }
}
