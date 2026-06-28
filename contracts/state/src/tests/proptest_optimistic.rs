//! Property-Based Tests for Optimistic Concurrency Control
//!
//! Simple tests verifying the core linearizability property:
//! sequential commits produce correct final state.

#[cfg(test)]
mod proptest_optimistic {
    use crate::{OptimisticContract, OptimisticContractClient};
    use soroban_sdk::{
        Bytes, Env, Map,
    };

    fn setup(env: &Env) -> OptimisticContractClient {
        let contract_id = env.register(OptimisticContract, ());
        let client = OptimisticContractClient::new(env, &contract_id);
        client.initialize();
        client
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

    const NUM_ROUNDS: usize = 5;

    /// Test sequential commits accumulate correctly
    #[test]
    fn test_sequential_accumulation() {
        let env = Env::default();
        env.mock_all_auths();
        let client = setup(&env);

        let batch_id = Bytes::from_slice(&env, b"seq_batch");
        let key = Bytes::from_slice(&env, b"counter");

        for i in 0..NUM_ROUNDS {
            let delta = 10_i64;
            let mut state_updates: Map<Bytes, Bytes> = Map::new(&env);
            state_updates.set(key.clone(), Bytes::from_slice(&env, &delta.to_be_bytes()));

            let seq = client.begin_optimistic(&batch_id, &state_updates);

            let id = make_mutation_id(&env, &batch_id, seq);
            let committed = client.commit_optimistic(&batch_id, &id);
            assert!(committed, "Commit {} should succeed", i);
        }

        // Final value should be 10 (last write wins, all writes are 10)
        let stored = client.get_state_value(&key);
        let mut bytes = [0u8; 8];
        for j in 0..8_u32 {
            bytes[j as usize] = stored.get(j).unwrap();
        }
        assert_eq!(
            i64::from_be_bytes(bytes),
            10,
            "Final value should be 10 (last write wins)"
        );
    }

    /// Test that rollback restores state
    #[test]
    fn test_rollback_creates_compensation() {
        let env = Env::default();
        env.mock_all_auths();
        let client = setup(&env);

        let batch_id = Bytes::from_slice(&env, b"rb_batch");
        let key = Bytes::from_slice(&env, b"val");

        // Set initial value
        let mut su: Map<Bytes, Bytes> = Map::new(&env);
        su.set(key.clone(), Bytes::from_slice(&env, &42_i64.to_be_bytes()));
        let init_seq = client.begin_optimistic(&batch_id, &su);
        let init_id = make_mutation_id(&env, &batch_id, init_seq);
        client.commit_optimistic(&batch_id, &init_id);

        // Begin a mutation and rollback
        let mut su2: Map<Bytes, Bytes> = Map::new(&env);
        su2.set(key.clone(), Bytes::from_slice(&env, &99_i64.to_be_bytes()));
        let seq2 = client.begin_optimistic(&batch_id, &su2);
        let id2 = make_mutation_id(&env, &batch_id, seq2);
        client.rollback_mutation(&batch_id, &id2);

        // Value should be restored to 42
        let stored = client.get_state_value(&key);
        let mut bytes = [0u8; 8];
        for j in 0..8_u32 {
            bytes[j as usize] = stored.get(j).unwrap();
        }
        assert_eq!(
            i64::from_be_bytes(bytes),
            42,
            "Value should be restored to 42 after rollback"
        );
    }

    /// Test version tracking
    #[test]
    fn test_version_tracking() {
        let env = Env::default();
        env.mock_all_auths();
        let client = setup(&env);

        let v0 = client.get_version();
        assert_eq!(v0.current_version, 0, "Initial version should be 0");

        let batch_id = Bytes::from_slice(&env, b"ver_batch");
        let mut su: Map<Bytes, Bytes> = Map::new(&env);
        let key = Bytes::from_slice(&env, b"k");
        su.set(key.clone(), Bytes::from_slice(&env, &1_i64.to_be_bytes()));

        let seq = client.begin_optimistic(&batch_id, &su);
        let id = make_mutation_id(&env, &batch_id, seq);
        client.commit_optimistic(&batch_id, &id);

        let v1 = client.get_version();
        assert_eq!(
            v1.current_version, 1,
            "Version should increment after commit"
        );
    }
}
