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
    fn test_create_financial_snapshot_basic() {
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

        // Withdraw some funds to create a financial history
        set_timestamp(&env, 1_050);
        client.mock_all_auths().withdraw(&grant_id, &100);

        // Create financial snapshot
        let snapshot = client.mock_all_auths().create_financial_snapshot(&grant_id).unwrap();

        // Verify snapshot structure
        assert_eq!(snapshot.grant_id, grant_id);
        assert_eq!(snapshot.total_received, 100); // withdrawn amount
        assert_eq!(snapshot.version, 1);
        assert_eq!(snapshot.expiry, 1_050 + 86400); // 24 hours from now
        assert_eq!(snapshot.timestamp, 1_050);

        // Verify hash and signature are not empty
        let mut hash_empty = true;
        for byte in snapshot.hash {
            if byte != 0 {
                hash_empty = false;
                break;
            }
        }
        assert!(!hash_empty);

        let mut signature_empty = true;
        for byte in snapshot.contract_signature {
            if byte != 0 {
                signature_empty = false;
                break;
            }
        }
        assert!(!signature_empty);
    }

    #[test]
    fn test_financial_snapshot_unauthorized_fails() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let recipient = Address::generate(&env);
        let unauthorized = Address::generate(&env);
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

        // Create grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &1000,
            &10,
            &0,
            &Address::generate(&env),
            &String::from_str(&env, "PROP-001"),
            &String::from_str(&env, "SN-12345"),
            &1000,
            &1_000 + 100,
        );

        // Try to create snapshot as unauthorized user - should fail
        let result = client.try_create_financial_snapshot(&grant_id);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), Error::NotAuthorized);
    }

    #[test]
    fn test_financial_snapshot_withdrawn_and_claimable() {
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

        // Create snapshot (should include both withdrawn and claimable)
        let snapshot = client.mock_all_auths().create_financial_snapshot(&grant_id).unwrap();

        // After 50 seconds at 10/second rate, we should have 500 claimable
        // Total received should be 100 (withdrawn) + 500 (claimable) = 600
        assert_eq!(snapshot.total_received, 600);
    }

    #[test]
    fn test_verify_financial_snapshot_valid() {
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

        // Create grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &1000,
            &10,
            &0,
            &Address::generate(&env),
            &String::from_str(&env, "PROP-001"),
            &String::from_str(&env, "SN-12345"),
            &1000,
            &1_000 + 100,
        );

        // Create snapshot
        let snapshot = client.mock_all_auths().create_financial_snapshot(&grant_id).unwrap();

        // Verify snapshot with correct data
        let result = client.mock_all_auths().verify_financial_snapshot(
            &grant_id,
            &snapshot.timestamp,
            &snapshot.total_received,
            &snapshot.hash,
            &snapshot.contract_signature,
        );

        assert!(result.is_ok());
        assert!(result.unwrap()); // Should return true for valid verification
    }

    #[test]
    fn test_verify_financial_snapshot_invalid_hash() {
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

        // Create grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &1000,
            &10,
            &0,
            &Address::generate(&env),
            &String::from_str(&env, "PROP-001"),
            &String::from_str(&env, "SN-12345"),
            &1000,
            &1_000 + 100,
        );

        // Create snapshot
        let snapshot = client.mock_all_auths().create_financial_snapshot(&grant_id).unwrap();

        // Try to verify with invalid hash
        let mut invalid_hash = snapshot.hash;
        invalid_hash[0] = invalid_hash[0].wrapping_add(1); // Modify first byte

        let result = client.mock_all_auths().verify_financial_snapshot(
            &grant_id,
            &snapshot.timestamp,
            &snapshot.total_received,
            &invalid_hash,
            &snapshot.contract_signature,
        );

        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), Error::InvalidSnapshot);
    }

    #[test]
    fn test_verify_financial_snapshot_invalid_signature() {
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

        // Create grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &1000,
            &10,
            &0,
            &Address::generate(&env),
            &String::from_str(&env, "PROP-001"),
            &String::from_str(&env, "SN-12345"),
            &1000,
            &1_000 + 100,
        );

        // Create snapshot
        let snapshot = client.mock_all_auths().create_financial_snapshot(&grant_id).unwrap();

        // Try to verify with invalid signature
        let mut invalid_signature = snapshot.contract_signature;
        invalid_signature[0] = invalid_signature[0].wrapping_add(1); // Modify first byte

        let result = client.mock_all_auths().verify_financial_snapshot(
            &grant_id,
            &snapshot.timestamp,
            &snapshot.total_received,
            &snapshot.hash,
            &invalid_signature,
        );

        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), Error::InvalidSignature);
    }

    #[test]
    fn test_get_snapshot_info_valid() {
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

        // Create grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &1000,
            &10,
            &0,
            &Address::generate(&env),
            &String::from_str(&env, "PROP-001"),
            &String::from_str(&env, "SN-12345"),
            &1000,
            &1_000 + 100,
        );

        // Create snapshot
        let snapshot = client.mock_all_auths().create_financial_snapshot(&grant_id).unwrap();

        // Get snapshot info
        let retrieved = client.mock_all_auths().get_snapshot_info(&grant_id, &snapshot.timestamp).unwrap();

        // Verify retrieved snapshot matches original
        assert_eq!(retrieved.grant_id, snapshot.grant_id);
        assert_eq!(retrieved.total_received, snapshot.total_received);
        assert_eq!(retrieved.timestamp, snapshot.timestamp);
        assert_eq!(retrieved.expiry, snapshot.expiry);
        assert_eq!(retrieved.version, snapshot.version);
        assert_eq!(retrieved.hash, snapshot.hash);
        assert_eq!(retrieved.contract_signature, snapshot.contract_signature);
    }

    #[test]
    fn test_get_snapshot_info_expired() {
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

        // Create grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &1000,
            &10,
            &0,
            &Address::generate(&env),
            &String::from_str(&env, "PROP-001"),
            &String::from_str(&env, "SN-12345"),
            &1000,
            &1_000 + 100,
        );

        // Create snapshot
        let snapshot = client.mock_all_auths().create_financial_snapshot(&grant_id).unwrap();

        // Fast forward past expiry
        set_timestamp(&env, snapshot.expiry + 1);

        // Try to get expired snapshot info - should fail
        let result = client.mock_all_auths().try_get_snapshot_info(&grant_id, &snapshot.timestamp);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), Error::SnapshotExpired);
    }

    #[test]
    fn test_snapshot_not_found() {
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

        // Try to get non-existent snapshot
        let result = client.mock_all_auths().try_get_snapshot_info(&grant_id, &1_000);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), Error::SnapshotNotFound);
    }

    #[test]
    fn test_snapshot_nonce_increment() {
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

        // Create grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &1000,
            &10,
            &0,
            &Address::generate(&env),
            &String::from_str(&env, "PROP-001"),
            &String::from_str(&env, "SN-12345"),
            &1000,
            &1_000 + 100,
        );

        // Create first snapshot
        let snapshot1 = client.mock_all_auths().create_financial_snapshot(&grant_id).unwrap();

        // Create second snapshot
        set_timestamp(&env, 1_100);
        let snapshot2 = client.mock_all_auths().create_financial_snapshot(&grant_id).unwrap();

        // Verify timestamps are different (nonce ensures uniqueness)
        assert_ne!(snapshot1.timestamp, snapshot2.timestamp);
        assert!(snapshot2.timestamp > snapshot1.timestamp);
    }

    #[test]
    fn test_financial_snapshot_event_emission() {
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

        // Create grant
        set_timestamp(&env, 1_000);
        client.mock_all_auths().create_grant(
            &grant_id,
            &recipient,
            &1000,
            &10,
            &0,
            &Address::generate(&env),
            &String::from_str(&env, "PROP-001"),
            &String::from_str(&env, "SN-12345"),
            &1000,
            &1_000 + 100,
        );

        // Create snapshot and capture events
        client.mock_all_auths().create_financial_snapshot(&grant_id).unwrap();

        // Verify event was published
        let events = env.events().all();
        assert!(events.len() >= 1);
        
        // Find the financial_snapshot_created event
        let mut found_event = false;
        for i in 0..events.len() {
            let event = events.get(i).unwrap();
            let topics = event.topics;
            if topics.len() >= 1 {
                let topic = topics.get(0).unwrap();
                if topic == Symbol::new(&env, "financial_snapshot_created") {
                    found_event = true;
                    break;
                }
            }
        }
        assert!(found_event);
    }
}
