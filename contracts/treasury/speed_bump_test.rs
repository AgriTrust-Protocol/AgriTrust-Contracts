#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    #[test]
    fn test_small_transfer_executes_immediately() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, SpeedBumpContract);
        let client = SpeedBumpContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let recipient = Address::generate(&env);

        // Register token and mint to treasury contract
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract(token_admin);
        let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);
        token_client.mint(&contract_id, &10_000i128);

        client.initialize(&admin, &token, &100_000u64);

        // 5% of treasury = 5000, well under 10% threshold of 10000
        let executed = client.approve_transfer(&admin, &recipient, &5_000u64);
        assert!(executed);
        assert_eq!(client.get_pending_transfers().len(), 0);

        // Verify recipient received the tokens
        let real_token = soroban_sdk::token::Client::new(&env, &token);
        assert_eq!(real_token.balance(&recipient), 5_000i128);
    }

    #[test]
    fn test_large_transfer_is_queued() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, SpeedBumpContract);
        let client = SpeedBumpContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let token = Address::generate(&env);
        let recipient = Address::generate(&env);

        client.initialize(&admin, &token, &100_000u64);

        // 15% of treasury exceeds 10% threshold
        let executed = client.approve_transfer(&admin, &recipient, &15_000u64);
        assert!(!executed);
        assert_eq!(client.get_pending_transfers().len(), 1);
    }

    #[test]
    #[should_panic(expected = "Speed bump active")]
    fn test_execute_before_delay_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, SpeedBumpContract);
        let client = SpeedBumpContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let token = Address::generate(&env);
        let recipient = Address::generate(&env);

        client.initialize(&admin, &token, &100_000u64);
        client.approve_transfer(&admin, &recipient, &15_000u64);

        let pending = client.get_pending_transfers();
        let transfer_id = pending.get(0).unwrap().id;

        // Try to execute immediately — should panic
        client.execute_transfer(&admin, &transfer_id);
    }

    #[test]
    fn test_execute_after_delay_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, SpeedBumpContract);
        let client = SpeedBumpContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let recipient = Address::generate(&env);

        // Register token and mint to treasury contract
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract(token_admin);
        let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);
        token_client.mint(&contract_id, &20_000i128);

        client.initialize(&admin, &token, &100_000u64);
        
        // 15% of treasury exceeds 10% threshold, so it queues
        let executed = client.approve_transfer(&admin, &recipient, &15_000u64);
        assert!(!executed);
        assert_eq!(client.get_pending_transfers().len(), 1);

        let pending = client.get_pending_transfers();
        let transfer_id = pending.get(0).unwrap().id;

        // Advance time by 72 hours + 1 second
        env.ledger().with_mut(|li| {
            li.timestamp += 72 * 60 * 60 + 1;
        });

        // Execute the pending transfer
        client.execute_transfer(&admin, &transfer_id);

        // Verify recipient received the tokens and pending transfers is empty
        let real_token = soroban_sdk::token::Client::new(&env, &token);
        assert_eq!(real_token.balance(&recipient), 15_000i128);
        assert_eq!(client.get_pending_transfers().len(), 0);
    }
}