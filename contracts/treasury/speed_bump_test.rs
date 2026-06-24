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
        let token = Address::generate(&env);
        let recipient = Address::generate(&env);

        client.initialize(&admin, &token, &100_000u64);

        // 5% of treasury = 5000, well under 10% threshold of 10000
        let executed = client.approve_transfer(&admin, &recipient, &5_000u64);
        assert!(executed);
        assert_eq!(client.get_pending_transfers().len(), 0);
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
}