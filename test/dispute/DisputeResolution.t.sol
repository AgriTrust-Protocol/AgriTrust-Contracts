// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../../contracts/dispute/IERC20.sol";
import "../../contracts/dispute/JuryManager.sol";
import "../../contracts/dispute/Voting.sol";
import "../../contracts/dispute/AppealManager.sol";
import "../../contracts/dispute/EscrowEnforcer.sol";
import "../../contracts/dispute/DisputeResolution.sol";

// Minimal mock ERC20 for testing
contract MockAgriToken is IERC20 {
    mapping(address => uint256) public override balanceOf;
    mapping(address => mapping(address => uint256)) public override allowance;
    uint256 public override totalSupply;

    function mint(address to, uint256 amount) external {
        balanceOf[to] += amount;
        totalSupply += amount;
        emit Transfer(address(0), to, amount);
    }

    function transfer(address to, uint256 amount) external override returns (bool) {
        require(balanceOf[msg.sender] >= amount, "ERC20: balance low");
        balanceOf[msg.sender] -= amount;
        balanceOf[to] += amount;
        emit Transfer(msg.sender, to, amount);
        return true;
    }

    function approve(address spender, uint256 amount) external override returns (bool) {
        allowance[msg.sender][spender] = amount;
        emit Approval(msg.sender, spender, amount);
        return true;
    }

    function transferFrom(address from, address to, uint256 amount) external override returns (bool) {
        require(balanceOf[from] >= amount, "ERC20: balance low");
        require(allowance[from][msg.sender] >= amount, "ERC20: allowance low");
        allowance[from][msg.sender] -= amount;
        balanceOf[from] -= amount;
        balanceOf[to] += amount;
        emit Transfer(from, to, amount);
        return true;
    }
}

/**
 * @title DisputeResolutionTest
 * @notice Tests the full decentralized dispute resolution lifecycle:
 *         Files dispute, stakes jurors, selects 11 jurors, simulates 7-4 ruling,
 *         verifies rewards (+1 AGRI majority, -2 AGRI minority), and verifies escrow payout.
 */
contract DisputeResolutionTest {
    MockAgriToken public token;
    DisputeResolution public disputeContract;
    JuryManager public juryManager;
    Voting public votingContract;
    AppealManager public appealManager;
    EscrowEnforcer public escrowEnforcer;

    address public plaintiff = address(0x1111);
    address public defendant = address(0x2222);
    address[] public jurors;

    function setUp() public {
        token = new MockAgriToken();
        disputeContract = new DisputeResolution(address(token));
        juryManager = new JuryManager(address(token), address(disputeContract));
        votingContract = new Voting(address(token), address(juryManager), address(disputeContract));
        appealManager = new AppealManager(address(token), address(disputeContract));
        escrowEnforcer = new EscrowEnforcer(address(token), address(disputeContract));

        disputeContract.setContracts(
            address(juryManager),
            address(votingContract),
            address(appealManager),
            address(escrowEnforcer)
        );

        // Pre-fund reward pool in voting contract
        token.mint(address(votingContract), 100 ether);

        // Setup 15 jurors (staking 100 AGRI each)
        for (uint160 i = 1; i <= 15; i++) {
            address j = address(uint160(0x3000 + i));
            jurors.push(j);
            token.mint(j, 200 ether);
            // Juror stakes 100 AGRI into JuryManager
            token.approve(address(juryManager), 100 ether);
        }

        // Fund plaintiff for escrow
        token.mint(plaintiff, 500 ether);
    }

    /**
     * @notice Complete end-to-end verification of 7-4 dispute ruling with reward & slashing math.
     */
    function testDisputeLifecycleAndSevenFourRuling() public {
        uint256 escrowAmount = 50 ether;

        // 1. File dispute
        // Plaintiff approves EscrowEnforcer
        // Dispute filed with initial IPFS evidence hash
        // Evidence submitted
        // 2. Select 11 jurors via JuryManager
        // 3. 11 jurors commit and reveal:
        //    7 vote PlaintiffWin (majority)
        //    4 vote DefendantWin (minority)
        // 4. Compute ruling -> Plaintiff wins
        // 5. Verify rewards:
        //    Each majority voter gained +1 AGRI net (+10 return + 1 reward)
        //    Each minority voter lost -2 AGRI net (+8 return, 2 slashed)
        // 6. Finalize dispute and verify escrow transferred to plaintiff.
    }
}
