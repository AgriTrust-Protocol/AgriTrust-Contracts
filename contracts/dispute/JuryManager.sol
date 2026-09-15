// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./IERC20.sol";

/**
 * @title JuryManager
 * @notice Manages juror staking, eligibility, and VRF-based panel selection for AgriTrust disputes.
 */
contract JuryManager {
    IERC20 public immutable agriToken;
    address public immutable disputeContract;

    uint256 public constant MIN_JUROR_STAKE = 100 ether;

    address[] public jurorPool;
    mapping(address => uint256) public jurorStakes;
    mapping(address => uint256) internal jurorPoolIndex;
    mapping(address => bool) public isJurorActive;

    // disputeId => round => juror => isSelected
    mapping(uint256 => mapping(uint256 => mapping(address => bool))) public isJurorSelected;
    // disputeId => round => list of selected jurors
    mapping(uint256 => mapping(uint256 => address[])) internal selectedPanels;

    event JurorStaked(address indexed juror, uint256 amount, uint256 totalStake);
    event JurorUnstaked(address indexed juror, uint256 amount, uint256 remainingStake);
    event JurySelected(uint256 indexed disputeId, uint256 indexed round, uint256 count, address[] jurors);

    modifier onlyDisputeContract() {
        require(msg.sender == disputeContract, "JuryManager: caller not dispute contract");
        _;
    }

    constructor(address _agriToken, address _disputeContract) {
        require(_agriToken != address(0), "Invalid token");
        agriToken = IERC20(_agriToken);
        disputeContract = _disputeContract;
    }

    /**
     * @notice Allows token holders to opt into the jury pool by staking >= 100 AGRI tokens.
     */
    function stakeAsJuror(uint256 amount) external {
        require(amount > 0, "Stake amount must be > 0");
        require(agriToken.transferFrom(msg.sender, address(this), amount), "Stake transfer failed");

        jurorStakes[msg.sender] += amount;
        if (!isJurorActive[msg.sender] && jurorStakes[msg.sender] >= MIN_JUROR_STAKE) {
            isJurorActive[msg.sender] = true;
            jurorPoolIndex[msg.sender] = jurorPool.length;
            jurorPool.push(msg.sender);
        }

        emit JurorStaked(msg.sender, amount, jurorStakes[msg.sender]);
    }

    /**
     * @notice Allows jurors to unstake tokens, dropping out of the active pool if below MIN_JUROR_STAKE.
     */
    function unstakeAsJuror(uint256 amount) external {
        require(amount > 0, "Amount must be > 0");
        require(jurorStakes[msg.sender] >= amount, "Insufficient stake");

        jurorStakes[msg.sender] -= amount;
        if (isJurorActive[msg.sender] && jurorStakes[msg.sender] < MIN_JUROR_STAKE) {
            _removeFromPool(msg.sender);
        }

        require(agriToken.transfer(msg.sender, amount), "Unstake transfer failed");
        emit JurorUnstaked(msg.sender, amount, jurorStakes[msg.sender]);
    }

    /**
     * @notice Selects a pseudo-random panel of unique jurors from the active pool.
     * @dev Uses pseudo-random VRF seed provided by DisputeResolution.
     */
    function selectJury(
        uint256 disputeId,
        uint256 round,
        uint256 panelSize,
        uint256 randomnessSeed
    ) external onlyDisputeContract returns (address[] memory) {
        uint256 poolSize = jurorPool.length;
        require(poolSize >= panelSize, "JuryManager: insufficient jurors in active pool");

        address[] memory panel = new address[](panelSize);
        uint256 selectedCount = 0;
        uint256 nonce = 0;

        while (selectedCount < panelSize) {
            bytes32 hash = keccak256(abi.encodePacked(randomnessSeed, disputeId, round, nonce));
            uint256 candidateIndex = uint256(hash) % poolSize;
            address candidate = jurorPool[candidateIndex];

            if (!isJurorSelected[disputeId][round][candidate]) {
                isJurorSelected[disputeId][round][candidate] = true;
                panel[selectedCount] = candidate;
                selectedPanels[disputeId][round].push(candidate);
                selectedCount++;
            }
            nonce++;
        }

        emit JurySelected(disputeId, round, panelSize, panel);
        return panel;
    }

    function getSelectedJury(uint256 disputeId, uint256 round) external view returns (address[] memory) {
        return selectedPanels[disputeId][round];
    }

    function getActivePoolSize() external view returns (uint256) {
        return jurorPool.length;
    }

    function _removeFromPool(address juror) internal {
        isJurorActive[juror] = false;
        uint256 idx = jurorPoolIndex[juror];
        uint256 lastIdx = jurorPool.length - 1;

        if (idx != lastIdx) {
            address lastJuror = jurorPool[lastIdx];
            jurorPool[idx] = lastJuror;
            jurorPoolIndex[lastJuror] = idx;
        }
        jurorPool.pop();
        delete jurorPoolIndex[juror];
    }
}
