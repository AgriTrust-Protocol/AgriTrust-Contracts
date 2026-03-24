#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token, Address, Env, Symbol,
    };

    fn set_timestamp(env: &Env, timestamp: u64) {
        env.ledger().with_mut(|li| {
            li.timestamp = timestamp;
        });
    }

    #[test]
    fn test_propose_slashing_basic() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let recipient = Address::generate(&env);
        let grant_token = Address::generate(&env);
        let treasury = Address::generate(&env);
        let native_token = Address::generate(&env);

        let contract_id = env.register(GrantContract, ());
        let client = GrantContractClient::new(&env, &contract_id);

        // Initialize contract
        client.mock_all_auths().initialize(
            &admin,
            &grant_token,
            &treasury,
            &native_token,
        );

        let grant_id: u64 = 1;
        let total_amount: i128 = 1000;
        let flow_rate: i128 = 10;
        let stake_percentage: i128 = 2000; // 20% stake

        // Create grant with staking requirement
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &total_amount,
            &flow_rate,
            &0,
            &stake_percentage,
            &grant_token,
        );

        // Post stake
        let stake_amount = total_amount * stake_percentage / 10000; // 200
        let token_client = token::Client::new(&env, &grant_token);
        token_client.mint(&recipient, &stake_amount);
        token_client.approve(&recipient, &contract_id, &stake_amount, &1000);
        client.mock_all_auths().post_stake(&grant_id, &stake_amount);

        // Propose slashing
        let reason = String::from_str(&env, "Fraudulent project activities detected");
        let evidence = String::from_str(&env, "Evidence documents hash...");
        let proposal_id = client.mock_all_auths().propose_slashing(&grant_id, &reason, &evidence).unwrap();

        // Verify proposal structure
        let proposal = client.get_slashing_proposal(&proposal_id).unwrap();
        assert_eq!(proposal.proposal_id, proposal_id);
        assert_eq!(proposal.grant_id, grant_id);
        assert_eq!(proposal.status, SlashingProposalStatus::Proposed);
        assert_eq!(proposal.reason, reason);
        assert_eq!(proposal.votes_for, 0);
        assert_eq!(proposal.votes_against, 0);
        assert_eq!(proposal.voting_deadline, 1_000 + (7 * 24 * 60 * 60)); // 7 days from now
    }

    #[test]
    fn test_propose_slashing_no_stake_fails() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let recipient = Address::generate(&env);
        let grant_token = Address::generate(&env);
        let treasury = Address::generate(&env);
        let native_token = Address::generate(&env);

        let contract_id = env.register(GrantContract, ());
        let client = GrantContractClient::new(&env, &contract_id);

        // Initialize contract
        client.mock_all_auths().initialize(
            &admin,
            &grant_token,
            &treasury,
            &native_token,
        );

        let grant_id: u64 = 1;

        // Create grant without staking requirement
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &1000,
            &10,
            &0,
        );

        // Try to propose slashing - should fail
        let reason = String::from_str(&env, "Test reason");
        let evidence = String::from_str(&env, "Test evidence");
        let result = client.mock_all_auths().try_propose_slashing(&grant_id, &reason, &evidence);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), Error::NoStakeToSlash);
    }

    #[test]
    fn test_propose_slashing_duplicate_fails() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let recipient = Address::generate(&env);
        let grant_token = Address::generate(&env);
        let treasury = Address::generate(&env);
        let native_token = Address::generate(&env);

        let contract_id = env.register(GrantContract, ());
        let client = GrantContractClient::new(&env, &contract_id);

        // Initialize contract
        client.mock_all_auths().initialize(
            &admin,
            &grant_token,
            &treasury,
            &native_token,
        );

        let grant_id: u64 = 1;
        let stake_percentage: i128 = 2000;

        // Create grant with staking requirement
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &1000,
            &10,
            &0,
            &stake_percentage,
            &grant_token,
        );

        // Post stake
        let stake_amount = 200;
        let token_client = token::Client::new(&env, &grant_token);
        token_client.mint(&recipient, &stake_amount);
        token_client.approve(&recipient, &contract_id, &stake_amount, &1000);
        client.mock_all_auths().post_stake(&grant_id, &stake_amount);

        // Create first proposal
        let reason = String::from_str(&env, "First proposal");
        let evidence = String::from_str(&env, "First evidence");
        client.mock_all_auths().propose_slashing(&grant_id, &reason, &evidence).unwrap();

        // Try to create second proposal - should fail
        let reason2 = String::from_str(&env, "Second proposal");
        let evidence2 = String::from_str(&env, "Second evidence");
        let result = client.mock_all_auths().try_propose_slashing(&grant_id, &reason2, &evidence2);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), Error::ProposalAlreadyExists);
    }

    #[test]
    fn test_vote_on_slashing() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let recipient = Address::generate(&env);
        let voter = Address::generate(&env);
        let grant_token = Address::generate(&env);
        let treasury = Address::generate(&env);
        let native_token = Address::generate(&env);

        let contract_id = env.register(GrantContract, ());
        let client = GrantContractClient::new(&env, &contract_id);

        // Initialize contract
        client.mock_all_auths().initialize(
            &admin,
            &grant_token,
            &treasury,
            &native_token,
        );

        let grant_id: u64 = 1;
        let stake_percentage: i128 = 2000;

        // Create grant with staking requirement
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &1000,
            &10,
            &0,
            &stake_percentage,
            &grant_token,
        );

        // Post stake
        let stake_amount = 200;
        let token_client = token::Client::new(&env, &grant_token);
        token_client.mint(&recipient, &stake_amount);
        token_client.approve(&recipient, &contract_id, &stake_amount, &1000);
        client.mock_all_auths().post_stake(&grant_id, &stake_amount);

        // Create proposal
        let reason = String::from_str(&env, "Test proposal");
        let evidence = String::from_str(&env, "Test evidence");
        let proposal_id = client.mock_all_auths().propose_slashing(&grant_id, &reason, &evidence).unwrap();

        // Set voting power for voter
        let voting_power = 1000;
        client.mock_all_auths().set_voting_power(&voter, &voting_power);

        // Vote on proposal
        client.mock_all_auths().vote_on_slashing(&proposal_id, &true); // Vote in favor

        // Verify vote was recorded
        let proposal = client.get_slashing_proposal(&proposal_id).unwrap();
        assert_eq!(proposal.votes_for, voting_power);
        assert_eq!(proposal.votes_against, 0);
    }

    #[test]
    fn test_vote_twice_fails() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let recipient = Address::generate(&env);
        let voter = Address::generate(&env);
        let grant_token = Address::generate(&env);
        let treasury = Address::generate(&env);
        let native_token = Address::generate(&env);

        let contract_id = env.register(GrantContract, ());
        let client = GrantContractClient::new(&env, &contract_id);

        // Initialize contract
        client.mock_all_auths().initialize(
            &admin,
            &grant_token,
            &treasury,
            &native_token,
        );

        let grant_id: u64 = 1;
        let stake_percentage: i128 = 2000;

        // Create grant with staking requirement
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &1000,
            &10,
            &0,
            &stake_percentage,
            &grant_token,
        );

        // Post stake
        let stake_amount = 200;
        let token_client = token::Client::new(&env, &grant_token);
        token_client.mint(&recipient, &stake_amount);
        token_client.approve(&recipient, &contract_id, &stake_amount, &1000);
        client.mock_all_auths().post_stake(&grant_id, &stake_amount);

        // Create proposal
        let reason = String::from_str(&env, "Test proposal");
        let evidence = String::from_str(&env, "Test evidence");
        let proposal_id = client.mock_all_auths().propose_slashing(&grant_id, &reason, &evidence).unwrap();

        // Set voting power for voter
        client.mock_all_auths().set_voting_power(&voter, &1000);

        // Vote once
        client.mock_all_auths().vote_on_slashing(&proposal_id, &true);

        // Try to vote again - should fail
        let result = client.mock_all_auths().try_vote_on_slashing(&proposal_id, &false);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), Error::AlreadyVoted);
    }

    #[test]
    fn test_execute_slashing_success() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let recipient = Address::generate(&env);
        let voter1 = Address::generate(&env);
        let voter2 = Address::generate(&env);
        let grant_token = Address::generate(&env);
        let treasury = Address::generate(&env);
        let native_token = Address::generate(&env);

        let contract_id = env.register(GrantContract, ());
        let client = GrantContractClient::new(&env, &contract_id);

        // Initialize contract
        client.mock_all_auths().initialize(
            &admin,
            &grant_token,
            &treasury,
            &native_token,
        );

        let grant_id: u64 = 1;
        let stake_percentage: i128 = 2000;

        // Create grant with staking requirement
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &1000,
            &10,
            &0,
            &stake_percentage,
            &grant_token,
        );

        // Post stake
        let stake_amount = 200;
        let token_client = token::Client::new(&env, &grant_token);
        token_client.mint(&recipient, &stake_amount);
        token_client.approve(&recipient, &contract_id, &stake_amount, &1000);
        client.mock_all_auths().post_stake(&grant_id, &stake_amount);

        // Create proposal
        let reason = String::from_str(&env, "Fraudulent activities");
        let evidence = String::from_str(&env, "Evidence documents");
        let proposal_id = client.mock_all_auths().propose_slashing(&grant_id, &reason, &evidence).unwrap();

        // Set voting power and vote
        client.mock_all_auths().set_voting_power(&voter1, &1000);
        client.mock_all_auths().set_voting_power(&voter2, &500);

        client.mock_all_auths().vote_on_slashing(&proposal_id, &true); // voter1 in favor
        client.mock_all_auths().vote_on_slashing(&proposal_id, &true); // voter2 in favor

        // Fast forward past voting deadline
        set_timestamp(&env, 1_000 + (7 * 24 * 60 * 60) + 1);

        // Execute slashing
        client.mock_all_auths().execute_slashing(&proposal_id).unwrap();

        // Verify grant was slashed
        let grant = client.get_grant(&grant_id).unwrap();
        assert_eq!(grant.status, GrantStatus::Slashed);
        assert_eq!(grant.staked_amount, 0);
        assert_eq!(grant.slash_reason, Some(reason));

        // Verify stake was transferred to treasury
        assert_eq!(token_client.balance(&treasury), stake_amount);

        // Verify proposal status
        let proposal = client.get_slashing_proposal(&proposal_id).unwrap();
        assert_eq!(proposal.status, SlashingProposalStatus::Executed);
        assert!(proposal.executed_at.is_some());
    }

    #[test]
    fn test_execute_slashing_insufficient_approval_fails() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let recipient = Address::generate(&env);
        let voter1 = Address::generate(&env);
        let voter2 = Address::generate(&env);
        let grant_token = Address::generate(&env);
        let treasury = Address::generate(&env);
        let native_token = Address::generate(&env);

        let contract_id = env.register(GrantContract, ());
        let client = GrantContractClient::new(&env, &contract_id);

        // Initialize contract
        client.mock_all_auths().initialize(
            &admin,
            &grant_token,
            &treasury,
            &native_token,
        );

        let grant_id: u64 = 1;
        let stake_percentage: i128 = 2000;

        // Create grant with staking requirement
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &1000,
            &10,
            &0,
            &stake_percentage,
            &grant_token,
        );

        // Post stake
        let stake_amount = 200;
        let token_client = token::Client::new(&env, &grant_token);
        token_client.mint(&recipient, &stake_amount);
        token_client.approve(&recipient, &contract_id, &stake_amount, &1000);
        client.mock_all_auths().post_stake(&grant_id, &stake_amount);

        // Create proposal
        let reason = String::from_str(&env, "Test proposal");
        let evidence = String::from_str(&env, "Test evidence");
        let proposal_id = client.mock_all_auths().propose_slashing(&grant_id, &reason, &evidence).unwrap();

        // Set voting power and vote against
        client.mock_all_auths().set_voting_power(&voter1, &1000);
        client.mock_all_auths().set_voting_power(&voter2, &500);

        client.mock_all_auths().vote_on_slashing(&proposal_id, &false); // voter1 against
        client.mock_all_auths().vote_on_slashing(&proposal_id, &false); // voter2 against

        // Fast forward past voting deadline
        set_timestamp(&env, 1_000 + (7 * 24 * 60 * 60) + 1);

        // Try to execute slashing - should fail
        let result = client.mock_all_auths().try_execute_slashing(&proposal_id);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), Error::ApprovalThresholdNotMet);

        // Verify proposal was rejected
        let proposal = client.get_slashing_proposal(&proposal_id).unwrap();
        assert_eq!(proposal.status, SlashingProposalStatus::Rejected);
    }

    #[test]
    fn test_execute_slashing_voting_period_active_fails() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let recipient = Address::generate(&env);
        let grant_token = Address::generate(&env);
        let treasury = Address::generate(&env);
        let native_token = Address::generate(&env);

        let contract_id = env.register(GrantContract, ());
        let client = GrantContractClient::new(&env, &contract_id);

        // Initialize contract
        client.mock_all_auths().initialize(
            &admin,
            &grant_token,
            &treasury,
            &native_token,
        );

        let grant_id: u64 = 1;
        let stake_percentage: i128 = 2000;

        // Create grant with staking requirement
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &1000,
            &10,
            &0,
            &stake_percentage,
            &grant_token,
        );

        // Post stake
        let stake_amount = 200;
        let token_client = token::Client::new(&env, &grant_token);
        token_client.mint(&recipient, &stake_amount);
        token_client.approve(&recipient, &contract_id, &stake_amount, &1000);
        client.mock_all_auths().post_stake(&grant_id, &stake_amount);

        // Create proposal
        let reason = String::from_str(&env, "Test proposal");
        let evidence = String::from_str(&env, "Test evidence");
        let proposal_id = client.mock_all_auths().propose_slashing(&grant_id, &reason, &evidence).unwrap();

        // Try to execute before voting deadline - should fail
        let result = client.mock_all_auths().try_execute_slashing(&proposal_id);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), Error::VotingPeriodActive);
    }

    #[test]
    fn test_get_grant_slashing_proposals() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let recipient = Address::generate(&env);
        let grant_token = Address::generate(&env);
        let treasury = Address::generate(&env);
        let native_token = Address::generate(&env);

        let contract_id = env.register(GrantContract, ());
        let client = GrantContractClient::new(&env, &contract_id);

        // Initialize contract
        client.mock_all_auths().initialize(
            &admin,
            &grant_token,
            &treasury,
            &native_token,
        );

        let grant_id: u64 = 1;
        let stake_percentage: i128 = 2000;

        // Create grant with staking requirement
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &1000,
            &10,
            &0,
            &stake_percentage,
            &grant_token,
        );

        // Post stake
        let stake_amount = 200;
        let token_client = token::Client::new(&env, &grant_token);
        token_client.mint(&recipient, &stake_amount);
        token_client.approve(&recipient, &contract_id, &stake_amount, &1000);
        client.mock_all_auths().post_stake(&grant_id, &stake_amount);

        // Initially no proposals
        let proposals = client.get_grant_slashing_proposals(&grant_id);
        assert_eq!(proposals.len(), 0);

        // Create proposal
        let reason = String::from_str(&env, "Test proposal");
        let evidence = String::from_str(&env, "Test evidence");
        let proposal_id = client.mock_all_auths().propose_slashing(&grant_id, &reason, &evidence).unwrap();

        // Should have one proposal
        let proposals = client.get_grant_slashing_proposals(&grant_id);
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals.get(0).unwrap(), proposal_id);
    }

    #[test]
    fn test_slashing_events_emission() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let recipient = Address::generate(&env);
        let voter = Address::generate(&env);
        let grant_token = Address::generate(&env);
        let treasury = Address::generate(&env);
        let native_token = Address::generate(&env);

        let contract_id = env.register(GrantContract, ());
        let client = GrantContractClient::new(&env, &contract_id);

        // Initialize contract
        client.mock_all_auths().initialize(
            &admin,
            &grant_token,
            &treasury,
            &native_token,
        );

        let grant_id: u64 = 1;
        let stake_percentage: i128 = 2000;

        // Create grant with staking requirement
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &1000,
            &10,
            &0,
            &stake_percentage,
            &grant_token,
        );

        // Post stake
        let stake_amount = 200;
        let token_client = token::Client::new(&env, &grant_token);
        token_client.mint(&recipient, &stake_amount);
        token_client.approve(&recipient, &contract_id, &stake_amount, &1000);
        client.mock_all_auths().post_stake(&grant_id, &stake_amount);

        // Create proposal and capture events
        let reason = String::from_str(&env, "Test proposal");
        let evidence = String::from_str(&env, "Test evidence");
        client.mock_all_auths().propose_slashing(&grant_id, &reason, &evidence).unwrap();

        // Verify proposal event
        let events = env.events().all();
        assert!(events.len() >= 1);
        
        let mut found_proposal_event = false;
        for i in 0..events.len() {
            let event = events.get(i).unwrap();
            let topics = event.topics;
            if topics.len() >= 1 {
                let topic = topics.get(0).unwrap();
                if topic == Symbol::new(&env, "slashing_proposed") {
                    found_proposal_event = true;
                    break;
                }
            }
        }
        assert!(found_proposal_event);

        // Set voting power and vote
        client.mock_all_auths().set_voting_power(&voter, &1000);
        client.mock_all_auths().vote_on_slashing(&proposal_id, &true);

        // Verify vote event
        let mut found_vote_event = false;
        for i in 0..events.len() {
            let event = events.get(i).unwrap();
            let topics = event.topics;
            if topics.len() >= 1 {
                let topic = topics.get(0).unwrap();
                if topic == Symbol::new(&env, "slashing_vote_cast") {
                    found_vote_event = true;
                    break;
                }
            }
        }
        assert!(found_vote_event);
    }
}
