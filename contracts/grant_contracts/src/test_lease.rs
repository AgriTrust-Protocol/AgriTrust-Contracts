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
    fn test_create_lease_grant() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        let recipient = Address::generate(&env);
        let lessor = Address::generate(&env);
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
            &oracle,
            &native_token,
        );

        let grant_id: u64 = 1;
        let total_amount: i128 = 1000;
        let flow_rate: i128 = 10;
        let property_id = String::from_str(&env, "PROP-001");
        let serial_number = String::from_str(&env, "SN-12345");
        let security_deposit_percentage: i128 = 1000; // 10%

        // Create lease grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &total_amount,
            &flow_rate,
            &0,
            &lessor,
            &property_id,
            &serial_number,
            &security_deposit_percentage,
            &1_000 + (total_amount / flow_rate), // 100 seconds from now
        );

        // Verify lease info
        let (lessor_addr, prop_id, serial, security_deposit, lease_end, terminated) = 
            client.get_lease_info(&grant_id).unwrap();
        assert_eq!(lessor_addr, lessor);
        assert_eq!(prop_id, property_id);
        assert_eq!(serial, serial_number);
        assert_eq!(security_deposit, 100); // 10% of 1000
        assert_eq!(lease_end, 1_000 + 100);
        assert!(!terminated);

        // Verify property history
        let history = client.get_property_history(&property_id);
        assert_eq!(history.len(), 1);
        assert_eq!(history.get(0).unwrap(), (grant_id, recipient.clone(), 1_000));
    }

    #[test]
    fn test_lease_withdrawal_to_lessor() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        let recipient = Address::generate(&env);
        let lessor = Address::generate(&env);
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
            &oracle,
            &native_token,
        );

        let grant_id: u64 = 1;
        let total_amount: i128 = 1000;
        let flow_rate: i128 = 10;
        let property_id = String::from_str(&env, "PROP-001");
        let serial_number = String::from_str(&env, "SN-12345");

        // Create lease grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &total_amount,
            &flow_rate,
            &0,
            &lessor,
            &property_id,
            &serial_number,
            &1000, // 10% security deposit
            &1_000 + (total_amount / flow_rate),
        );

        // Mint tokens to contract for payments
        let token_client = token::Client::new(&env, &grant_token);
        token_client.mint(&contract_id, &total_amount);

        // Withdraw from lease - should pay lessor
        set_timestamp(&env, 1_050);
        client.mock_all_auths().withdraw(&grant_id, &100);

        // Verify lessor received payment
        assert_eq!(token_client.balance(&lessor), 100);
        assert_eq!(token_client.balance(&contract_id), total_amount - 100);
    }

    #[test]
    fn test_lease_termination_by_oracle() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        let recipient = Address::generate(&env);
        let lessor = Address::generate(&env);
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
            &oracle,
            &native_token,
        );

        let grant_id: u64 = 1;
        let total_amount: i128 = 1000;
        let flow_rate: i128 = 10;
        let property_id = String::from_str(&env, "PROP-001");
        let security_deposit: i128 = 100; // 10% deposit

        // Create lease grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &total_amount,
            &flow_rate,
            &0,
            &lessor,
            &property_id,
            &String::from_str(&env, "SN-12345"),
            &1000,
            &1_000 + (total_amount / flow_rate),
        );

        // Mint tokens and security deposit
        let token_client = token::Client::new(&env, &grant_token);
        token_client.mint(&contract_id, &total_amount);
        token_client.mint(&contract_id, &security_deposit);

        // Terminate lease by oracle
        let reason = String::from_str(&env, "Breach of contract terms");
        set_timestamp(&env, 1_100); // After lease end time
        client.mock_all_auths().terminate_lease_by_oracle(&grant_id, &reason);

        // Verify lease is terminated
        let (_, _, _, _, _, terminated) = client.get_lease_info(&grant_id).unwrap();
        assert!(terminated);

        // Verify security deposit returned to treasury
        assert_eq!(token_client.balance(&treasury), security_deposit);

        // Verify further withdrawals are blocked
        let result = client.mock_all_auths().try_withdraw(&grant_id, &100);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), Error::InvalidState);
    }

    #[test]
    fn test_lease_cannot_terminate_early() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        let recipient = Address::generate(&env);
        let lessor = Address::generate(&env);
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
            &oracle,
            &native_token,
        );

        let grant_id: u64 = 1;
        let total_amount: i128 = 1000;
        let property_id = String::from_str(&env, "PROP-001");

        // Create lease grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &total_amount,
            &flow_rate,
            &0,
            &lessor,
            &property_id,
            &String::from_str(&env, "SN-12345"),
            &1000,
            &1_000 + (total_amount / flow_rate), // Lease ends in future
        );

        // Try to terminate before lease end time - should fail
        let reason = String::from_str(&env, "Early termination");
        set_timestamp(&env, 1_050); // Before lease end time
        let result = client.mock_all_auths().try_terminate_lease_by_oracle(&grant_id, &reason);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), Error::LeaseNotExpired);
    }

    #[test]
    fn test_invalid_security_deposit_percentage() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        let recipient = Address::generate(&env);
        let lessor = Address::generate(&env);
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
            &oracle,
            &native_token,
        );

        let grant_id: u64 = 1;
        let total_amount: i128 = 1000;

        // Try to create lease with too low security deposit (4%)
        set_timestamp(&env, 1_000);
        let result = client.mock_all_auths().try_create_grant(
            &grant_id,
            &recipient,
            &total_amount,
            &flow_rate,
            &0,
            &lessor,
            &String::from_str(&env, "PROP-001"),
            &String::from_str(&env, "SN-12345"),
            &400, // 4% - below minimum 5%
            &1_000 + (total_amount / flow_rate),
        );
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), Error::InvalidSecurityDeposit);

        // Try to create lease with too high security deposit (25%)
        let result = client.mock_all_auths().try_create_grant(
            &grant_id,
            &recipient,
            &total_amount,
            &flow_rate,
            &0,
            &lessor,
            &String::from_str(&env, "PROP-001"),
            &String::from_str(&env, "SN-12345"),
            &2500, // 25% - above maximum 20%
            &1_000 + (total_amount / flow_rate),
        );
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), Error::InvalidSecurityDeposit);
    }

    #[test]
    fn test_regular_grant_lease_functions_fail() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
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
            &oracle,
            &native_token,
        );

        // Create regular grant (not lease)
        let grant_id: u64 = 1;
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &1000,
            &flow_rate,
            &0,
        );

        // Try to get lease info - should fail
        let result = client.mock_all_auths().try_get_lease_info(&grant_id);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), Error::InvalidLeaseTerms);

        // Try to terminate regular grant as lease - should fail
        let reason = String::from_str(&env, "Test termination");
        let result = client.mock_all_auths().try_terminate_lease_by_oracle(&grant_id, &reason);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), Error::InvalidLeaseTerms);
    }

    #[test]
    fn test_double_lease_termination_fails() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        let recipient = Address::generate(&env);
        let lessor = Address::generate(&env);
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
            &oracle,
            &native_token,
        );

        let grant_id: u64 = 1;
        let property_id = String::from_str(&env, "PROP-001");

        // Create lease grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &1000,
            &flow_rate,
            &0,
            &lessor,
            &property_id,
            &String::from_str(&env, "SN-12345"),
            &1000,
            &1_000 + (1000 / flow_rate),
        );

        // Terminate lease
        let reason = String::from_str(&env, "First termination");
        set_timestamp(&env, 1_100);
        client.mock_all_auths().terminate_lease_by_oracle(&grant_id, &reason);

        // Try to terminate again - should fail
        let result = client.mock_all_auths().try_terminate_lease_by_oracle(&grant_id, &reason);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), Error::LeaseAlreadyTerminated);
    }
}
