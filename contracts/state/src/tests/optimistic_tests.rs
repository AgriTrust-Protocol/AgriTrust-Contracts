//! Tests for Optimistic Concurrency Control
//!
//! Unit tests verifying the 2PC protocol, compensation, and linearizability.

#[cfg(test)]
mod tests {
    use crate::{OptimisticContract, OptimisticContractClient};
    use soroban_sdk::{
        Bytes, Env, Map,
    };

    fn setup(env: &Env) -> (OptimisticContractClient, Bytes) {
        let contract_id = env.register(OptimisticContract, ());
        let client = OptimisticContractClient::new(env, &contract_id);
        client.initialize();
        let batch_id = Bytes::from_slice(env, b"test_batch");
        (client, batch_id)
    }

    fn make_key(env: &Env, name: &[u8]) -> Bytes {
        Bytes::from_slice(env, name)
    }

    fn make_value(env: &Env, val: u64) -> Bytes {
        Bytes::from_slice(env, &val.to_be_bytes())
    }

    fn make_mutation_id(env: &Env, batch_id: &Bytes, seq_no: u64) -> Bytes {
        let mut data = Bytes::new(env);
        for i in 0..batch_id.len() {
            data.push_back(batch_id.get(i).unwrap());
        }
        let seq_bytes = seq_no.to_be_bytes();
        for b in seq_bytes.iter() {
            data.push_back(*b);
        }
        data
    }

    #[test]
    fn test_initialize() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _) = setup(&env);
        let version = client.get_version();
        assert_eq!(version.current_version, 0);
        assert_eq!(version.current_seq_no, 0);
    }

    #[test]
    fn test_begin_assigns_seq_no() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, batch_id) = setup(&env);

        let seq1 = client.begin_optimistic(&batch_id, &Map::new(&env));
        assert_eq!(seq1, 1);

        let seq2 = client.begin_optimistic(&batch_id, &Map::new(&env));
        assert_eq!(seq2, 2);
    }

    #[test]
    fn test_commit_in_order() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, batch_id) = setup(&env);
        let key = make_key(&env, b"balance");
        let val1 = make_value(&env, 100);
        let val2 = make_value(&env, 200);

        let seq1 = {
            let mut updates = Map::new(&env);
            updates.set(key.clone(), val1.clone());
            client.begin_optimistic(&batch_id, &updates)
        };

        let seq2 = {
            let mut updates = Map::new(&env);
            updates.set(key.clone(), val2.clone());
            client.begin_optimistic(&batch_id, &updates)
        };

        let id1 = make_mutation_id(&env, &batch_id, seq1);
        let committed1 = client.commit_optimistic(&batch_id, &id1);
        assert!(committed1);

        let stored = client.get_state_value(&key);
        assert_eq!(stored, val1);

        let id2 = make_mutation_id(&env, &batch_id, seq2);
        let committed2 = client.commit_optimistic(&batch_id, &id2);
        assert!(committed2);

        let stored2 = client.get_state_value(&key);
        assert_eq!(stored2, val2);

        let final_version = client.get_version();
        assert_eq!(final_version.current_version, 2);
        assert_eq!(final_version.current_seq_no, 2);
    }

    #[test]
    fn test_commit_out_of_order_compensates() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, batch_id) = setup(&env);
        let key = make_key(&env, b"balance");
        let val1 = make_value(&env, 100);
        let val2 = make_value(&env, 200);

        let seq1 = {
            let mut updates = Map::new(&env);
            updates.set(key.clone(), val1.clone());
            client.begin_optimistic(&batch_id, &updates)
        };

        let seq2 = {
            let mut updates = Map::new(&env);
            updates.set(key.clone(), val2.clone());
            client.begin_optimistic(&batch_id, &updates)
        };

        // Try to commit second first (should fail - out of order)
        let id2 = make_mutation_id(&env, &batch_id, seq2);
        let result = client.commit_optimistic(&batch_id, &id2);
        assert!(!result);

        // Now commit first (should succeed)
        let id1 = make_mutation_id(&env, &batch_id, seq1);
        let result1 = client.commit_optimistic(&batch_id, &id1);
        assert!(result1);

        // State should reflect only first mutation
        let stored = client.get_state_value(&key);
        assert_eq!(stored, val1);
    }

    #[test]
    fn test_rollback_restores_state() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, batch_id) = setup(&env);
        let key = make_key(&env, b"balance");
        let initial_val = make_value(&env, 50);

        // Set initial state via begin + commit
        let mut init_updates = Map::new(&env);
        init_updates.set(key.clone(), initial_val.clone());
        let init_seq = client.begin_optimistic(&batch_id, &init_updates);
        let init_id = make_mutation_id(&env, &batch_id, init_seq);
        client.commit_optimistic(&batch_id, &init_id);

        // Begin a new mutation
        let new_val = make_value(&env, 100);
        let mut updates = Map::new(&env);
        updates.set(key.clone(), new_val.clone());
        let seq = client.begin_optimistic(&batch_id, &updates);

        // Rollback
        let id = make_mutation_id(&env, &batch_id, seq);
        client.rollback_mutation(&batch_id, &id);

        // Verify state restored
        let stored = client.get_state_value(&key);
        assert_eq!(stored, initial_val);
    }

    #[test]
    fn test_expire_before_timeout_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, batch_id) = setup(&env);
        let key = make_key(&env, b"balance");
        let val = make_value(&env, 100);

        let mut updates = Map::new(&env);
        updates.set(key.clone(), val.clone());
        let seq = client.begin_optimistic(&batch_id, &updates);
        let id = make_mutation_id(&env, &batch_id, seq);

        // Cannot expire before timeout
        let expired = client.expire_pending(&batch_id, &id);
        assert!(!expired, "Should not expire before timeout");
    }

    #[test]
    fn test_version_linearization() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, batch_id) = setup(&env);
        let key = make_key(&env, b"counter");

        // Submit 5 sequential mutations
        for i in 1u64..=5 {
            let mut updates = Map::new(&env);
            updates.set(key.clone(), make_value(&env, i * 10));
            client.begin_optimistic(&batch_id, &updates);
        }

        // Commit all in order
        for i in 1u64..=5 {
            let id = make_mutation_id(&env, &batch_id, i);
            let committed = client.commit_optimistic(&batch_id, &id);
            assert!(committed, "seq {} should commit", i);
        }

        // Verify final state
        let final_version = client.get_version();
        assert_eq!(final_version.current_version, 5);
        assert_eq!(final_version.current_seq_no, 5);

        // Final value should be 50 (5 * 10)
        let stored = client.get_state_value(&key);
        assert_eq!(stored, make_value(&env, 50));
    }
}
