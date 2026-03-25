use soroban_sdk::token;
use soroban_sdk::{Address, Env, Symbol};
use crate::{
    ProposalStake, StakeStatus, Error, PROPOSAL_STAKE_AMOUNT, GrantContract,
    DataKey, LANDSLIDE_REJECTION_THRESHOLD, MIN_VOTING_PARTICIPATION_FOR_STAKE_BURN
};

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::{Address as TestAddress, Ledger as TestLedger};

    fn setup_contract(env: &Env) -> (Address, Address, Address) {
        let admin = Address::generate(env);
        let grant_token = Address::generate(env);
        let treasury = Address::generate(env);
        let oracle = Address::generate(env);
        let native_token = env.token_contract_address();

        GrantContract::initialize(
            env.clone(),
            admin.clone(),
            grant_token.clone(),
            treasury.clone(),
            oracle.clone(),
            native_token,
        ).unwrap();

        (admin, grant_token, treasury)
    }

    #[test]
    fn test_deposit_proposal_stake() {
        let env = Env::default();
        env.mock_all_auths();
        
        let (admin, grant_token, treasury) = setup_contract(&env);
        let staker = Address::generate(&env);
        let grant_id = 1u64;

        // Mint tokens to staker
        let token_client = token::Client::new(&env, &grant_token);
        token_client.mint(&staker, &(PROPOSAL_STAKE_AMOUNT * 2));

        // Test successful stake deposit
        GrantContract::deposit_proposal_stake(
            env.clone(),
            grant_id,
            staker.clone(),
            PROPOSAL_STAKE_AMOUNT,
        ).unwrap();

        // Verify stake was created
        let stake = GrantContract::get_proposal_stake(env.clone(), grant_id).unwrap();
        assert_eq!(stake.grant_id, grant_id);
        assert_eq!(stake.staker, staker);
        assert_eq!(stake.amount, PROPOSAL_STAKE_AMOUNT);
        assert_eq!(stake.status, StakeStatus::Deposited);

        // Verify escrow balance
        let balance = GrantContract::get_stake_escrow_balance(env.clone());
        assert_eq!(balance, PROPOSAL_STAKE_AMOUNT);

        // Verify staker balance decreased
        let staker_balance = token_client.balance(&staker);
        assert_eq!(staker_balance, PROPOSAL_STAKE_AMOUNT);
    }

    #[test]
    fn test_deposit_invalid_amount() {
        let env = Env::default();
        env.mock_all_auths();
        
        let (admin, grant_token, treasury) = setup_contract(&env);
        let staker = Address::generate(&env);
        let grant_id = 1u64;

        // Mint tokens to staker
        let token_client = token::Client::new(&env, &grant_token);
        token_client.mint(&staker, &(PROPOSAL_STAKE_AMOUNT * 2));

        // Test deposit with invalid amount
        let result = GrantContract::deposit_proposal_stake(
            env.clone(),
            grant_id,
            staker.clone(),
            PROPOSAL_STAKE_AMOUNT / 2, // Wrong amount
        );

        assert_eq!(result.unwrap_err(), Error::InvalidStakeAmount);
    }

    #[test]
    fn test_deposit_duplicate_stake() {
        let env = Env::default();
        env.mock_all_auths();
        
        let (admin, grant_token, treasury) = setup_contract(&env);
        let staker = Address::generate(&env);
        let grant_id = 1u64;

        // Mint tokens to staker
        let token_client = token::Client::new(&env, &grant_token);
        token_client.mint(&staker, &(PROPOSAL_STAKE_AMOUNT * 2));

        // First deposit should succeed
        GrantContract::deposit_proposal_stake(
            env.clone(),
            grant_id,
            staker.clone(),
            PROPOSAL_STAKE_AMOUNT,
        ).unwrap();

        // Second deposit should fail
        let result = GrantContract::deposit_proposal_stake(
            env.clone(),
            grant_id,
            staker.clone(),
            PROPOSAL_STAKE_AMOUNT,
        );

        assert_eq!(result.unwrap_err(), Error::StakeAlreadyDeposited);
    }

    #[test]
    fn test_return_proposal_stake() {
        let env = Env::default();
        env.mock_all_auths();
        
        let (admin, grant_token, treasury) = setup_contract(&env);
        let staker = Address::generate(&env);
        let grant_id = 1u64;

        // Mint tokens to staker
        let token_client = token::Client::new(&env, &grant_token);
        token_client.mint(&staker, &(PROPOSAL_STAKE_AMOUNT * 2));

        // Deposit stake
        GrantContract::deposit_proposal_stake(
            env.clone(),
            grant_id,
            staker.clone(),
            PROPOSAL_STAKE_AMOUNT,
        ).unwrap();

        let initial_balance = token_client.balance(&staker);

        // Return stake
        GrantContract::return_proposal_stake(
            env.clone(),
            admin.clone(),
            grant_id,
        ).unwrap();

        // Verify stake status
        let stake = GrantContract::get_proposal_stake(env.clone(), grant_id).unwrap();
        assert_eq!(stake.status, StakeStatus::Returned);
        assert!(stake.returned_at.is_some());

        // Verify escrow balance
        let balance = GrantContract::get_stake_escrow_balance(env.clone());
        assert_eq!(balance, 0);

        // Verify staker balance returned
        let final_balance = token_client.balance(&staker);
        assert_eq!(final_balance, initial_balance + PROPOSAL_STAKE_AMOUNT);
    }

    #[test]
    fn test_burn_proposal_stake() {
        let env = Env::default();
        env.mock_all_auths();
        
        let (admin, grant_token, treasury) = setup_contract(&env);
        let staker = Address::generate(&env);
        let grant_id = 1u64;

        // Mint tokens to staker
        let token_client = token::Client::new(&env, &grant_token);
        token_client.mint(&staker, &(PROPOSAL_STAKE_AMOUNT * 2));

        // Deposit stake
        GrantContract::deposit_proposal_stake(
            env.clone(),
            grant_id,
            staker.clone(),
            PROPOSAL_STAKE_AMOUNT,
        ).unwrap();

        let initial_treasury_balance = token_client.balance(&treasury);

        // Burn stake
        let burn_reason = "Proposal rejected by landslide vote".to_string();
        GrantContract::burn_proposal_stake(
            env.clone(),
            admin.clone(),
            grant_id,
            burn_reason.clone(),
        ).unwrap();

        // Verify stake status
        let stake = GrantContract::get_proposal_stake(env.clone(), grant_id).unwrap();
        assert_eq!(stake.status, StakeStatus::Burned);
        assert_eq!(stake.burn_reason, Some(burn_reason));

        // Verify escrow balance
        let balance = GrantContract::get_stake_escrow_balance(env.clone());
        assert_eq!(balance, 0);

        // Verify burned stakes total
        let burned_total = GrantContract::get_burned_stakes_total(env.clone());
        assert_eq!(burned_total, PROPOSAL_STAKE_AMOUNT);

        // Verify treasury received the burned stake
        let final_treasury_balance = token_client.balance(&treasury);
        assert_eq!(final_treasury_balance, initial_treasury_balance + PROPOSAL_STAKE_AMOUNT);

        // Verify staker did not get tokens back
        let staker_balance = token_client.balance(&staker);
        assert_eq!(staker_balance, PROPOSAL_STAKE_AMOUNT); // Only remaining tokens
    }

    #[test]
    fn test_has_valid_stake() {
        let env = Env::default();
        env.mock_all_auths();
        
        let (admin, grant_token, treasury) = setup_contract(&env);
        let staker = Address::generate(&env);
        let grant_id = 1u64;

        // Mint tokens to staker
        let token_client = token::Client::new(&env, &grant_token);
        token_client.mint(&staker, &(PROPOSAL_STAKE_AMOUNT * 2));

        // Before deposit, should not have valid stake
        assert!(!GrantContract::has_valid_stake(env.clone(), grant_id));

        // Deposit stake
        GrantContract::deposit_proposal_stake(
            env.clone(),
            grant_id,
            staker.clone(),
            PROPOSAL_STAKE_AMOUNT,
        ).unwrap();

        // Should have valid stake
        assert!(GrantContract::has_valid_stake(env.clone(), grant_id));

        // Return stake
        GrantContract::return_proposal_stake(
            env.clone(),
            admin.clone(),
            grant_id,
        ).unwrap();

        // Should not have valid stake anymore
        assert!(!GrantContract::has_valid_stake(env.clone(), grant_id));
    }

    #[test]
    fn test_should_burn_stake_logic() {
        // Test landslide rejection case
        let votes_for = 20i128;
        let votes_against = 80i128;
        let total_voting_power = 200i128;
        
        assert!(GrantContract::should_burn_stake(votes_for, votes_against, total_voting_power));

        // Test close rejection (not landslide)
        let votes_for = 45i128;
        let votes_against = 55i128;
        let total_voting_power = 200i128;
        
        assert!(!GrantContract::should_burn_stake(votes_for, votes_against, total_voting_power));

        // Test approval case
        let votes_for = 80i128;
        let votes_against = 20i128;
        let total_voting_power = 200i128;
        
        assert!(!GrantContract::should_burn_stake(votes_for, votes_against, total_voting_power));

        // Test insufficient participation
        let votes_for = 10i128;
        let votes_against = 40i128;
        let total_voting_power = 200i128; // Only 25% participation
        
        assert!(!GrantContract::should_burn_stake(votes_for, votes_against, total_voting_power));

        // Test zero voting power
        let votes_for = 10i128;
        let votes_against = 40i128;
        let total_voting_power = 0i128;
        
        assert!(!GrantContract::should_burn_stake(votes_for, votes_against, total_voting_power));
    }

    #[test]
    fn test_stake_already_returned_error() {
        let env = Env::default();
        env.mock_all_auths();
        
        let (admin, grant_token, treasury) = setup_contract(&env);
        let staker = Address::generate(&env);
        let grant_id = 1u64;

        // Mint tokens to staker
        let token_client = token::Client::new(&env, &grant_token);
        token_client.mint(&staker, &(PROPOSAL_STAKE_AMOUNT * 2));

        // Deposit stake
        GrantContract::deposit_proposal_stake(
            env.clone(),
            grant_id,
            staker.clone(),
            PROPOSAL_STAKE_AMOUNT,
        ).unwrap();

        // Return stake
        GrantContract::return_proposal_stake(
            env.clone(),
            admin.clone(),
            grant_id,
        ).unwrap();

        // Try to return again should fail
        let result = GrantContract::return_proposal_stake(
            env.clone(),
            admin.clone(),
            grant_id,
        );

        assert_eq!(result.unwrap_err(), Error::StakeAlreadyReturned);
    }

    #[test]
    fn test_stake_already_burned_error() {
        let env = Env::default();
        env.mock_all_auths();
        
        let (admin, grant_token, treasury) = setup_contract(&env);
        let staker = Address::generate(&env);
        let grant_id = 1u64;

        // Mint tokens to staker
        let token_client = token::Client::new(&env, &grant_token);
        token_client.mint(&staker, &(PROPOSAL_STAKE_AMOUNT * 2));

        // Deposit stake
        GrantContract::deposit_proposal_stake(
            env.clone(),
            grant_id,
            staker.clone(),
            PROPOSAL_STAKE_AMOUNT,
        ).unwrap();

        // Burn stake
        GrantContract::burn_proposal_stake(
            env.clone(),
            admin.clone(),
            grant_id,
            "Test burn".to_string(),
        ).unwrap();

        // Try to burn again should fail
        let result = GrantContract::burn_proposal_stake(
            env.clone(),
            admin.clone(),
            grant_id,
            "Test burn again".to_string(),
        );

        assert_eq!(result.unwrap_err(), Error::StakeAlreadyBurned);
    }
}
