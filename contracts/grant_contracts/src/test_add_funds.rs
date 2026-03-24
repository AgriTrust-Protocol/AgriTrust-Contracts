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
    fn test_add_funds_basic() {
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

        // Create grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &total_amount,
            &flow_rate,
            &0,
            &Address::generate(&env), // lessor
            &String::from_str(&env, "PROP-001"),
            &String::from_str(&env, "SN-12345"),
            &1000, // 10% security deposit
            &1_000 + (total_amount / flow_rate),
        );

        // Add funds
        let additional_amount: i128 = 500;
        client.mock_all_auths().add_funds(&grant_id, &additional_amount);

        // Verify remaining balance increased
        let grant = client.get_grant(&grant_id).unwrap();
        assert_eq!(grant.remaining_balance, total_amount + additional_amount);
        
        // Verify end time extended
        let expected_extension = additional_amount / flow_rate; // 500 / 10 = 50 seconds
        assert_eq!(grant.lease_end_time, 1_000 + (total_amount / flow_rate) + expected_extension);
    }

    #[test]
    fn test_add_funds_zero_amount_fails() {
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

        // Create grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &total_amount,
            &flow_rate,
            &0,
            &Address::generate(&env),
            &String::from_str(&env, "PROP-001"),
            &String::from_str(&env, "SN-12345"),
            &1000,
            &1_000 + (total_amount / flow_rate),
        );

        // Try to add zero funds - should fail
        let result = client.mock_all_auths().try_add_funds(&grant_id, &0);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), Error::InvalidAmount);
    }

    #[test]
    fn test_add_funds_inactive_grant_fails() {
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

        // Create grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &total_amount,
            &flow_rate,
            &0,
            &Address::generate(&env),
            &String::from_str(&env, "PROP-001"),
            &String::from_str(&env, "SN-12345"),
            &1000,
            &1_000 + (total_amount / flow_rate),
        );

        // Cancel grant
        client.mock_all_auths().cancel_stream(&grant_id);

        // Try to add funds to cancelled grant - should fail
        let result = client.mock_all_auths().try_add_funds(&grant_id, &500);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), Error::InvalidState);
    }

    #[test]
    fn test_add_funds_unauthorized_fails() {
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

        // Create grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &total_amount,
            &flow_rate,
            &0,
            &Address::generate(&env),
            &String::from_str(&env, "PROP-001"),
            &String::from_str(&env, "SN-12345"),
            &1000,
            &1_000 + (total_amount / flow_rate),
        );

        // Try to add funds as non-admin - should fail
        let non_admin = Address::generate(&env);
        let result = client.try_add_funds(&grant_id, &500);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), Error::NotAuthorized);
    }

    #[test]
    fn test_add_funds_zero_flow_rate() {
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
        let flow_rate: i128 = 0; // Zero flow rate (time-locked)

        // Create grant with zero flow rate
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &total_amount,
            &flow_rate,
            &0,
            &Address::generate(&env),
            &String::from_str(&env, "PROP-001"),
            &String::from_str(&env, "SN-12345"),
            &1000,
            &1_000 + (total_amount / flow_rate),
        );

        // Add funds - should not extend end time
        let original_end_time = 1_000 + (total_amount / flow_rate);
        client.mock_all_auths().add_funds(&grant_id, &500);

        // Verify end time unchanged (flow rate is 0)
        let grant = client.get_grant(&grant_id).unwrap();
        assert_eq!(grant.lease_end_time, original_end_time);
        
        // Verify remaining balance increased
        assert_eq!(grant.remaining_balance, total_amount + 500);
    }

    #[test]
    fn test_add_funds_with_withdrawals() {
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

        // Create grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &total_amount,
            &flow_rate,
            &0,
            &Address::generate(&env),
            &String::from_str(&env, "PROP-001"),
            &String::from_str(&env, "SN-12345"),
            &1000,
            &1_000 + (total_amount / flow_rate),
        );

        // Mint tokens to contract
        let token_client = token::Client::new(&env, &grant_token);
        token_client.mint(&contract_id, &total_amount);

        // Withdraw some funds
        set_timestamp(&env, 1_050);
        client.mock_all_auths().withdraw(&grant_id, &100);

        // Add funds
        let additional_amount: i128 = 500;
        client.mock_all_auths().add_funds(&grant_id, &additional_amount);

        // Verify remaining balance calculation:
        // Original: 1000 total, 100 withdrawn, 100 claimable, 900 remaining
        // After add_funds: 1000 total, 100 withdrawn, 100 claimable, 1400 remaining
        let grant = client.get_grant(&grant_id).unwrap();
        assert_eq!(grant.remaining_balance, 1400);
        assert_eq!(grant.claimable, 100); // Should be unchanged
        assert_eq!(grant.withdrawn, 100); // Should be unchanged
    }

    #[test]
    fn test_grant_top_up_event() {
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

        // Create grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &total_amount,
            &flow_rate,
            &0,
            &Address::generate(&env),
            &String::from_str(&env, "PROP-001"),
            &String::from_str(&env, "SN-12345"),
            &1000,
            &1_000 + (total_amount / flow_rate),
        );

        // Add funds and capture events
        let additional_amount: i128 = 300;
        client.mock_all_auths().add_funds(&grant_id, &additional_amount);

        // Verify event was published
        let events = env.events().all();
        assert!(events.len() >= 1);
        
        // Find the grant_top_up event
        let mut found_event = false;
        for i in 0..events.len() {
            let event = events.get(i).unwrap();
            let topics = event.topics;
            if topics.len() >= 1 {
                let topic = topics.get(0).unwrap();
                if topic == Symbol::new(&env, "grant_top_up") {
                    found_event = true;
                    break;
                }
            }
        }
        assert!(found_event);
    }

    #[test]
    fn test_math_overflow_protection() {
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
        let total_amount: i128 = i128::MAX; // Max value

        // Create grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &total_amount,
            &flow_rate,
            &0,
            &Address::generate(&env),
            &String::from_str(&env, "PROP-001"),
            &String::from_str(&env, "SN-12345"),
            &1000,
            &1_000 + (total_amount / flow_rate),
        );

        // Try to add funds that would cause overflow
        let result = client.mock_all_auths().try_add_funds(&grant_id, &i128::MAX);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), Error::MathOverflow);
    }
}
