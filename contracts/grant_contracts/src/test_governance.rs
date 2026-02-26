#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{ testutils::Address as _, Address, Env, String };

    fn create_test_env() -> (Env, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let governance_token = Address::generate(&env);
        let stake_token = env.register_stellar_asset_contract(admin.clone());

        (env, admin, governance_token, stake_token)
    }

    fn setup_governance(env: &Env, governance_token: &Address, stake_token: &Address) {
        GovernanceContract::initialize(
            env.clone(),
            governance_token.clone(),
            1000, // voting_threshold
            500, // quorum_threshold
            stake_token.clone(),
            100_000_000 // stake_amount: 10 XLM (assuming 7 decimals)
        ).unwrap();
    }

    #[test]
    fn test_initialize_governance() {
        let (env, _admin, governance_token, stake_token) = create_test_env();

        let result = GovernanceContract::initialize(
            env.clone(),
            governance_token.clone(),
            1000,
            500,
            stake_token.clone(),
            100_000_000
        );

        assert!(result.is_ok());

        // Test duplicate initialization
        let result = GovernanceContract::initialize(env.clone(), governance_token, 1000, 500, stake_token, 100_000_000);

        assert!(matches!(result, Err(GovernanceError::AlreadyInitialized)));
    }

    #[test]
    fn test_create_proposal() {
        let (env, admin, governance_token, stake_token) = create_test_env();
        setup_governance(&env, &governance_token, &stake_token);
        
        // Mint XLM to proposer so they can stake
        let token_client = token::StellarAssetClient::new(&env, &stake_token);
        token_client.mint(&admin, &100_000_000);

        let title = soroban_sdk::String::from_str(&env, "Test Proposal");
        let description = soroban_sdk::String::from_str(&env, "Test Description");
        let voting_period = 86400; // 1 day

        let proposal_id = GovernanceContract::create_proposal(
            env.clone(),
            admin.clone(),
            title.clone(),
            description.clone(),
            voting_period
        ).unwrap();

        assert_eq!(proposal_id, 0);

        let proposal = GovernanceContract::get_proposal_info(env.clone(), proposal_id).unwrap();
        assert_eq!(proposal.id, proposal_id);
        assert_eq!(proposal.proposer, admin);
        assert_eq!(proposal.title, title);
        assert_eq!(proposal.description, description);
        assert_eq!(proposal.status, ProposalStatus::Active);
        assert_eq!(proposal.yes_votes, 0);
        assert_eq!(proposal.no_votes, 0);
    }

    #[test]
    fn test_quadratic_voting_power_calculation() {
        let (env, _admin, governance_token, stake_token) = create_test_env();
        setup_governance(&env, &governance_token, &stake_token);

        let voter = Address::generate(&env);

        // Mock token balance - in real implementation this would come from token contract
        // For testing, we'll test the sqrt calculation directly
        let test_cases = vec![
            (0, 0), // sqrt(0) = 0
            (1, 1), // sqrt(1) = 1
            (4, 2), // sqrt(4) = 2
            (9, 3), // sqrt(9) = 3
            (16, 4), // sqrt(16) = 4
            (25, 5), // sqrt(25) = 5
            (100, 10), // sqrt(100) = 10
            (1000, 31) // sqrt(1000) ≈ 31.62 -> 31
        ];

        for (token_balance, expected_voting_power) in test_cases {
            let calculated_power = GovernanceContract::integer_sqrt(token_balance);
            assert_eq!(calculated_power, expected_voting_power);
        }
    }

    #[test]
    fn test_quadratic_vote() {
        let (env, admin, governance_token, stake_token) = create_test_env();
        setup_governance(&env, &governance_token, &stake_token);
        
        let token_client = token::StellarAssetClient::new(&env, &stake_token);
        token_client.mint(&admin, &100_000_000);

        let voter = Address::generate(&env);
        let title = soroban_sdk::String::from_str(&env, "Test Proposal");
        let description = soroban_sdk::String::from_str(&env, "Test Description");
        let voting_period = 86400;

        let proposal_id = GovernanceContract::create_proposal(
            env.clone(),
            admin.clone(),
            title,
            description,
            voting_period
        ).unwrap();

        // Test voting with weight 1 (quadratic: 1^2 = 1)
        let result = GovernanceContract::quadratic_vote(
            env.clone(),
            voter.clone(),
            proposal_id,
            1 // weight
        );

        assert!(result.is_ok());

        let proposal = GovernanceContract::get_proposal_info(env.clone(), proposal_id).unwrap();
        assert_eq!(proposal.yes_votes, 1); // 1^2 = 1

        // Test voting with weight 3 (quadratic: 3^2 = 9)
        let voter2 = Address::generate(&env);
        let result = GovernanceContract::quadratic_vote(
            env.clone(),
            voter2.clone(),
            proposal_id,
            3 // weight
        );

        assert!(result.is_ok());

        let proposal = GovernanceContract::get_proposal_info(env.clone(), proposal_id).unwrap();
        assert_eq!(proposal.yes_votes, 10); // 1^2 + 3^2 = 1 + 9 = 10
    }

    #[test]
    fn test_double_voting_prevention() {
        let (env, admin, governance_token, stake_token) = create_test_env();
        setup_governance(&env, &governance_token, &stake_token);

        let token_client = token::StellarAssetClient::new(&env, &stake_token);
        token_client.mint(&admin, &100_000_000);

        let voter = Address::generate(&env);
        let title = soroban_sdk::String::from_str(&env, "Test Proposal");
        let description = soroban_sdk::String::from_str(&env, "Test Description");
        let voting_period = 86400;

        let proposal_id = GovernanceContract::create_proposal(
            env.clone(),
            admin.clone(),
            title,
            description,
            voting_period
        ).unwrap();

        // First vote should succeed
        let result = GovernanceContract::quadratic_vote(env.clone(), voter.clone(), proposal_id, 1);
        assert!(result.is_ok());

        // Second vote should fail
        let result = GovernanceContract::quadratic_vote(env.clone(), voter.clone(), proposal_id, 2);
        assert!(matches!(result, Err(GovernanceError::AlreadyVoted)));
    }

    #[test]
    fn test_invalid_weight() {
        let (env, admin, governance_token, stake_token) = create_test_env();
        setup_governance(&env, &governance_token, &stake_token);

        let token_client = token::StellarAssetClient::new(&env, &stake_token);
        token_client.mint(&admin, &100_000_000);

        let voter = Address::generate(&env);
        let title = soroban_sdk::String::from_str(&env, "Test Proposal");
        let description = soroban_sdk::String::from_str(&env, "Test Description");
        let voting_period = 86400;

        let proposal_id = GovernanceContract::create_proposal(
            env.clone(),
            admin.clone(),
            title,
            description,
            voting_period
        ).unwrap();

        // Test with zero weight
        let result = GovernanceContract::quadratic_vote(env.clone(), voter.clone(), proposal_id, 0);
        assert!(matches!(result, Err(GovernanceError::InvalidWeight)));

        // Test with negative weight
        let result = GovernanceContract::quadratic_vote(
            env.clone(),
            voter.clone(),
            proposal_id,
            -1
        );
        assert!(matches!(result, Err(GovernanceError::InvalidWeight)));
    }

    #[test]
    fn test_proposal_execution() {
        let (env, admin, governance_token, stake_token) = create_test_env();
        setup_governance(&env, &governance_token, &stake_token);

        let token_client = token::StellarAssetClient::new(&env, &stake_token);
        token_client.mint(&admin, &100_000_000);

        let voter1 = Address::generate(&env);
        let voter2 = Address::generate(&env);
        let title = soroban_sdk::String::from_str(&env, "Test Proposal");
        let description = soroban_sdk::String::from_str(&env, "Test Description");
        let voting_period = 86400;

        let proposal_id = GovernanceContract::create_proposal(
            env.clone(),
            admin.clone(),
            title,
            description,
            voting_period
        ).unwrap();

        // Add votes to meet threshold
        GovernanceContract::quadratic_vote(env.clone(), voter1, proposal_id, 10).unwrap(); // 10^2 = 100
        GovernanceContract::quadratic_vote(env.clone(), voter2, proposal_id, 10).unwrap(); // 10^2 = 100

        // Try to execute before voting deadline
        let result = GovernanceContract::execute_proposal(env.clone(), proposal_id);
        assert!(matches!(result, Err(GovernanceError::VotingEnded)));

        // Advance time past voting deadline
        env.ledger().set_timestamp(env.ledger().timestamp() + voting_period + 1);

        // Execute proposal
        let result = GovernanceContract::execute_proposal(env.clone(), proposal_id);
        assert!(result.is_ok());

        let proposal = GovernanceContract::get_proposal_info(env.clone(), proposal_id).unwrap();
        assert_eq!(proposal.status, ProposalStatus::Executed);
    }

    #[test]
    fn test_quorum_not_met() {
        let (env, admin, governance_token, stake_token) = create_test_env();
        setup_governance(&env, &governance_token, &stake_token);

        let token_client = token::StellarAssetClient::new(&env, &stake_token);
        token_client.mint(&admin, &100_000_000);

        let voter = Address::generate(&env);
        let title = soroban_sdk::String::from_str(&env, "Test Proposal");
        let description = soroban_sdk::String::from_str(&env, "Test Description");
        let voting_period = 86400;

        let proposal_id = GovernanceContract::create_proposal(
            env.clone(),
            admin.clone(),
            title,
            description,
            voting_period
        ).unwrap();

        // Add small vote that doesn't meet quorum
        GovernanceContract::quadratic_vote(env.clone(), voter, proposal_id, 1).unwrap(); // 1^2 = 1

        // Advance time past voting deadline
        env.ledger().set_timestamp(env.ledger().timestamp() + voting_period + 1);

        // Try to execute - should fail due to quorum not met
        let result = GovernanceContract::execute_proposal(env.clone(), proposal_id);
        assert!(matches!(result, Err(GovernanceError::QuorumNotMet)));

        let proposal = GovernanceContract::get_proposal_info(env.clone(), proposal_id).unwrap();
        assert_eq!(proposal.status, ProposalStatus::Rejected);
    }

    #[test]
    fn test_voting_power_caching() {
        let (env, _admin, governance_token, stake_token) = create_test_env();
        setup_governance(&env, &governance_token, &stake_token);

        let voter = Address::generate(&env);

        // Calculate voting power
        let power1 = GovernanceContract::get_voter_power(env.clone(), voter.clone()).unwrap();

        // Check if voting power is cached
        let cached_power = env
            .storage()
            .instance()
            .get::<VotingPower>(&GovernanceDataKey::VotingPower(voter))
            .unwrap();

        assert_eq!(cached_power.voting_power, power1);
        assert_eq!(cached_power.address, voter);
    }

    #[test]
    fn test_get_vote_info() {
        let (env, admin, governance_token, stake_token) = create_test_env();
        setup_governance(&env, &governance_token, &stake_token);

        let token_client = token::StellarAssetClient::new(&env, &stake_token);
        token_client.mint(&admin, &100_000_000);

        let voter = Address::generate(&env);
        let title = soroban_sdk::String::from_str(&env, "Test Proposal");
        let description = soroban_sdk::String::from_str(&env, "Test Description");
        let voting_period = 86400;

        let proposal_id = GovernanceContract::create_proposal(
            env.clone(),
            admin.clone(),
            title,
            description,
            voting_period
        ).unwrap();

        // Cast a vote
        let weight = 5;
        GovernanceContract::quadratic_vote(
            env.clone(),
            voter.clone(),
            proposal_id,
            weight
        ).unwrap();

        // Get vote info
        let vote_info = GovernanceContract::get_vote_info(
            env.clone(),
            voter.clone(),
            proposal_id
        ).unwrap();

        assert_eq!(vote_info.voter, voter);
        assert_eq!(vote_info.proposal_id, proposal_id);
        assert_eq!(vote_info.weight, weight);
        assert!(vote_info.voting_power > 0);
    }

    #[test]
    fn test_get_nonexistent_vote_info() {
        let (env, admin, governance_token, stake_token) = create_test_env();
        setup_governance(&env, &governance_token, &stake_token);

        let token_client = token::StellarAssetClient::new(&env, &stake_token);
        token_client.mint(&admin, &100_000_000);

        let voter = Address::generate(&env);
        let title = soroban_sdk::String::from_str(&env, "Test Proposal");
        let description = soroban_sdk::String::from_str(&env, "Test Description");
        let voting_period = 86400;

        let proposal_id = GovernanceContract::create_proposal(
            env.clone(),
            admin.clone(),
            title,
            description,
            voting_period
        ).unwrap();

        // Try to get vote info for a vote that doesn't exist
        let result = GovernanceContract::get_vote_info(env.clone(), voter.clone(), proposal_id);
        assert!(matches!(result, Err(GovernanceError::ProposalNotFound)));
    }

    #[test]
    fn test_propose_grant_staking() {
        let (env, admin, governance_token, stake_token) = create_test_env();
        setup_governance(&env, &governance_token, &stake_token);

        let token_client = token::StellarAssetClient::new(&env, &stake_token);
        token_client.mint(&admin, &100_000_000);

        let title = soroban_sdk::String::from_str(&env, "Test Proposal");
        let description = soroban_sdk::String::from_str(&env, "Test Description");
        let voting_period = 86400;

        let proposal_id = GovernanceContract::create_proposal(
            env.clone(),
            admin.clone(),
            title,
            description,
            voting_period
        ).unwrap();

        // Admin staked 100_000_000, balance should be 0 since it was 100_000_000
        let admin_balance = token::Client::new(&env, &stake_token).balance(&admin);
        assert_eq!(admin_balance, 0);

        let contract_balance = token::Client::new(&env, &stake_token).balance(&env.current_contract_address());
        assert_eq!(contract_balance, 100_000_000);

        let proposal = GovernanceContract::get_proposal_info(env.clone(), proposal_id).unwrap();
        assert_eq!(proposal.stake_amount, 100_000_000);
        assert_eq!(proposal.stake_returned, false);
    }

    #[test]
    fn test_refund_stake() {
        let (env, admin, governance_token, stake_token) = create_test_env();
        setup_governance(&env, &governance_token, &stake_token);

        let token_client = token::StellarAssetClient::new(&env, &stake_token);
        token_client.mint(&admin, &100_000_000);

        let voter1 = Address::generate(&env);
        let proposal_id = GovernanceContract::create_proposal(
            env.clone(),
            admin.clone(),
            soroban_sdk::String::from_str(&env, "Test"),
            soroban_sdk::String::from_str(&env, "Desc"),
            86400
        ).unwrap();

        GovernanceContract::quadratic_vote(env.clone(), admin.clone(), proposal_id, 10).unwrap();

        env.ledger().set_timestamp(env.ledger().timestamp() + 86400 + 1);
        GovernanceContract::execute_proposal(env.clone(), proposal_id).unwrap();

        // Attempt Refund
        GovernanceContract::refund_stake(env.clone(), proposal_id).unwrap();

        let admin_balance = token::Client::new(&env, &stake_token).balance(&admin);
        assert_eq!(admin_balance, 100_000_000);
        
        let contract_balance = token::Client::new(&env, &stake_token).balance(&env.current_contract_address());
        assert_eq!(contract_balance, 0);
        
        let proposal = GovernanceContract::get_proposal_info(env.clone(), proposal_id).unwrap();
        assert_eq!(proposal.stake_returned, true);
    }
    
    #[test]
    fn test_slash_stake() {
        let (env, admin, governance_token, stake_token) = create_test_env();
        setup_governance(&env, &governance_token, &stake_token);

        let token_client = token::StellarAssetClient::new(&env, &stake_token);
        token_client.mint(&admin, &100_000_000);

        let treasury = Address::generate(&env);
        let proposal_id = GovernanceContract::create_proposal(
            env.clone(),
            admin.clone(),
            soroban_sdk::String::from_str(&env, "Spam Proposal"),
            soroban_sdk::String::from_str(&env, "Desc"),
            86400
        ).unwrap();

        GovernanceContract::slash_stake(env.clone(), admin.clone(), treasury.clone(), proposal_id).unwrap();

        let treasury_balance = token::Client::new(&env, &stake_token).balance(&treasury);
        assert_eq!(treasury_balance, 100_000_000);
        
        let contract_balance = token::Client::new(&env, &stake_token).balance(&env.current_contract_address());
        assert_eq!(contract_balance, 0);

        let proposal = GovernanceContract::get_proposal_info(env.clone(), proposal_id).unwrap();
        assert_eq!(proposal.stake_returned, true);
    }
}
