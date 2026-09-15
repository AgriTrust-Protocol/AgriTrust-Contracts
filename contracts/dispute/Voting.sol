// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./IERC20.sol";
import "./JuryManager.sol";

/**
 * @title Voting
 * @notice Handles commit-reveal voting, stake escrow, slashing, and reward distribution for jurors.
 */
contract Voting {
    enum VoteChoice { None, PlaintiffWin, DefendantWin }

    uint256 public constant VOTE_STAKE = 10 ether;
    uint256 public constant VOTING_PERIOD = 72 hours;
    uint256 public constant MAJORITY_REWARD = 1 ether;
    uint256 public constant MINORITY_SLASH = 2 ether;

    IERC20 public immutable agriToken;
    JuryManager public immutable juryManager;
    address public immutable disputeContract;

    struct RoundVoting {
        uint256 startTime;
        uint256 endTime;
        uint256 plaintiffVotes;
        uint256 defendantVotes;
        bool tallied;
        VoteChoice winningChoice;
    }

    struct JurorVote {
        bytes32 commitment;
        bool committed;
        bool revealed;
        VoteChoice choice;
        bool settled;
    }

    // disputeId => round => RoundVoting
    mapping(uint256 => mapping(uint256 => RoundVoting)) public roundVotings;
    // disputeId => round => juror => JurorVote
    mapping(uint256 => mapping(uint256 => mapping(address => JurorVote))) public jurorVotes;
    // disputeId => round => list of revealed jurors
    mapping(uint256 => mapping(uint256 => address[])) internal revealedJurors;

    event VoteCommitted(uint256 indexed disputeId, uint256 indexed round, address indexed juror, bytes32 commitment);
    event VoteRevealed(uint256 indexed disputeId, uint256 indexed round, address indexed juror, VoteChoice choice);
    event VotingTallied(uint256 indexed disputeId, uint256 indexed round, VoteChoice winningChoice, uint256 plaintiffVotes, uint256 defendantVotes);
    event JurorRewardSettled(uint256 indexed disputeId, uint256 indexed round, address indexed juror, int256 netReward);

    modifier onlyDisputeContract() {
        require(msg.sender == disputeContract, "Voting: caller not dispute contract");
        _;
    }

    constructor(address _agriToken, address _juryManager, address _disputeContract) {
        require(_agriToken != address(0) && _juryManager != address(0), "Invalid inputs");
        agriToken = IERC20(_agriToken);
        juryManager = JuryManager(_juryManager);
        disputeContract = _disputeContract;
    }

    function startVotingRound(uint256 disputeId, uint256 round) external onlyDisputeContract {
        require(roundVotings[disputeId][round].startTime == 0, "Round already started");
        roundVotings[disputeId][round] = RoundVoting({
            startTime: block.timestamp,
            endTime: block.timestamp + VOTING_PERIOD,
            plaintiffVotes: 0,
            defendantVotes: 0,
            tallied: false,
            winningChoice: VoteChoice.None
        });
    }

    /**
     * @notice Juror commits their vote hash and stakes 10 AGRI.
     * @param commitment keccak256(abi.encodePacked(choice, salt, msg.sender))
     */
    function commitVote(uint256 disputeId, uint256 round, bytes32 commitment) external {
        RoundVoting storage rv = roundVotings[disputeId][round];
        require(rv.startTime > 0, "Voting round not active");
        require(block.timestamp <= rv.endTime, "Voting period ended");
        require(juryManager.isJurorSelected(disputeId, round, msg.sender), "Not selected for this jury");
        require(!jurorVotes[disputeId][round][msg.sender].committed, "Already committed");
        require(commitment != bytes32(0), "Invalid commitment");

        require(agriToken.transferFrom(msg.sender, address(this), VOTE_STAKE), "Stake transfer failed");

        jurorVotes[disputeId][round][msg.sender] = JurorVote({
            commitment: commitment,
            committed: true,
            revealed: false,
            choice: VoteChoice.None,
            settled: false
        });

        emit VoteCommitted(disputeId, round, msg.sender, commitment);
    }

    /**
     * @notice Juror reveals their vote using the original choice and salt.
     */
    function revealVote(uint256 disputeId, uint256 round, VoteChoice choice, bytes32 salt) external {
        RoundVoting storage rv = roundVotings[disputeId][round];
        require(rv.startTime > 0, "Voting round not active");
        require(choice == VoteChoice.PlaintiffWin || choice == VoteChoice.DefendantWin, "Invalid choice");

        JurorVote storage jv = jurorVotes[disputeId][round][msg.sender];
        require(jv.committed, "No commitment found");
        require(!jv.revealed, "Already revealed");

        bytes32 expectedHash = keccak256(abi.encodePacked(choice, salt, msg.sender));
        require(jv.commitment == expectedHash, "Hash mismatch with commitment");

        jv.revealed = true;
        jv.choice = choice;
        revealedJurors[disputeId][round].push(msg.sender);

        if (choice == VoteChoice.PlaintiffWin) {
            rv.plaintiffVotes++;
        } else {
            rv.defendantVotes++;
        }

        emit VoteRevealed(disputeId, round, msg.sender, choice);
    }

    /**
     * @notice Computes and finalizes the round tally.
     */
    function tallyVotes(uint256 disputeId, uint256 round) external returns (VoteChoice) {
        RoundVoting storage rv = roundVotings[disputeId][round];
        require(rv.startTime > 0, "Round not initialized");
        require(!rv.tallied, "Already tallied");

        rv.tallied = true;
        if (rv.plaintiffVotes > rv.defendantVotes) {
            rv.winningChoice = VoteChoice.PlaintiffWin;
        } else if (rv.defendantVotes > rv.plaintiffVotes) {
            rv.winningChoice = VoteChoice.DefendantWin;
        } else {
            rv.winningChoice = VoteChoice.None; // Tie
        }

        emit VotingTallied(disputeId, round, rv.winningChoice, rv.plaintiffVotes, rv.defendantVotes);
        return rv.winningChoice;
    }

    /**
     * @notice Settles rewards and slashing for all revealed jurors in a tallied round.
     */
    function settleJurorRewards(uint256 disputeId, uint256 round) external {
        RoundVoting storage rv = roundVotings[disputeId][round];
        require(rv.tallied, "Round not tallied yet");

        address[] storage jurors = revealedJurors[disputeId][round];
        uint256 len = jurors.length;

        for (uint256 i = 0; i < len; i++) {
            address juror = jurors[i];
            JurorVote storage jv = jurorVotes[disputeId][round][juror];
            if (jv.revealed && !jv.settled) {
                jv.settled = true;
                if (rv.winningChoice == VoteChoice.None) {
                    // Tie: return full 10 AGRI stake
                    agriToken.transfer(juror, VOTE_STAKE);
                    emit JurorRewardSettled(disputeId, round, juror, 0);
                } else if (jv.choice == rv.winningChoice) {
                    // Majority: return stake + 1 AGRI reward = 11 AGRI
                    agriToken.transfer(juror, VOTE_STAKE + MAJORITY_REWARD);
                    emit JurorRewardSettled(disputeId, round, juror, int256(MAJORITY_REWARD));
                } else {
                    // Minority: slashed 2 AGRI, return remaining 8 AGRI
                    agriToken.transfer(juror, VOTE_STAKE - MINORITY_SLASH);
                    emit JurorRewardSettled(disputeId, round, juror, -int256(MINORITY_SLASH));
                }
            }
        }
    }

    function getRoundVoting(uint256 disputeId, uint256 round) external view returns (
        uint256 startTime,
        uint256 endTime,
        uint256 plaintiffVotes,
        uint256 defendantVotes,
        bool tallied,
        VoteChoice winningChoice
    ) {
        RoundVoting storage rv = roundVotings[disputeId][round];
        return (rv.startTime, rv.endTime, rv.plaintiffVotes, rv.defendantVotes, rv.tallied, rv.winningChoice);
    }
}
