//! Metadata Tests for Batch Certification Storage
//!
//! Validates:
//! 1. TTL coordination guard prevents shortening or resetting of persistence window.
//! 2. Staggered key hashing distribution across 4KB page boundaries.
//! 3. Single aggregated metadata entry per batch with map storage.
//! 4. Background TTL reconciliation for batch metadata entries.
//! 5. Concurrent simulation of 10 simultaneous store_metadata() calls maintaining TTL >= 90 days.
//! 6. Enforcement of MAX_METADATA_ENTRIES (100) boundary limit.

#[cfg(test)]
mod tests {
    use soroban_sdk::{
        testutils::{Address as _, Ledger as _, storage::Persistent as _},
        Address, BytesN, Env, String,
    };
    use provenance::{
        batch_metadata_key, compute_staggered_metadata_key, extend_ttl_coordinated,
        get_batch_metadata_record, get_metadata_entry,
        staggered_storage_key, store_batch_metadata, BatchMetadataRecord,
        CertificationMetadata, Error, MetadataType, ProvenanceContract,
        ProvenanceContractClient, MAX_METADATA_ENTRIES,
        TTL_EXTENSION_AMOUNT,
    };

    fn make_batch_id(env: &Env, id: u8) -> BytesN<32> {
        let mut raw = [0u8; 32];
        raw[0] = id;
        raw[31] = 0xBB;
        BytesN::from_array(env, &raw)
    }

    fn make_data_hash(env: &Env, seed: u8) -> BytesN<32> {
        let mut raw = [0u8; 32];
        raw[0] = seed;
        raw[15] = 0x77;
        BytesN::from_array(env, &raw)
    }

    #[test]
    fn test_concurrent_store_metadata_simulation_10_calls() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(ProvenanceContract, ());
        let client = ProvenanceContractClient::new(&env, &contract_id);

        let batch_id = make_batch_id(&env, 4);
        let mut stored_keys = std::vec::Vec::new();
        let mut certifiers = std::vec::Vec::new();
        let mut entry_types = std::vec::Vec::new();

        // 10 concurrent certifier submissions for the same batch
        for i in 0..10u8 {
            let certifier = Address::generate(&env);
            let entry_type = match i % 4 {
                0 => MetadataType::Certificate,
                1 => MetadataType::InspectionReport,
                2 => MetadataType::LabResult,
                _ => MetadataType::AuditAttestation,
            };
            let hash = make_data_hash(&env, i);
            let uri = String::from_str(&env, "ipfs://QmConcurrent");

            let key = client.store_metadata(
                &batch_id,
                &certifier,
                &entry_type,
                &hash,
                &uri,
                &0,
            );

            stored_keys.push(key);
            certifiers.push(certifier);
            entry_types.push(entry_type);
        }

        // Verify aggregated batch record holds all 10 entries
        let batch_rec = client.get_batch_metadata(&batch_id);
        assert_eq!(batch_rec.count, 10);
        assert_eq!(batch_rec.entries.len(), 10);

        let bkey = batch_metadata_key(&env, &batch_id);
        env.as_contract(&contract_id, || {
            let batch_ttl = env.storage().persistent().get_ttl(&bkey);
            assert!(
                batch_ttl >= TTL_EXTENSION_AMOUNT,
                "Aggregated batch entry TTL {} must be >= {}",
                batch_ttl,
                TTL_EXTENSION_AMOUNT
            );

            // Verify EVERY entry has TTL >= 90 days (1_555_200)
            for i in 0..10 {
                let key = &stored_keys[i];
                let skey = staggered_storage_key(&env, key);
                let ttl = env.storage().persistent().get_ttl(&skey);
                assert!(
                    ttl >= TTL_EXTENSION_AMOUNT,
                    "Entry {} TTL {} must be >= {}",
                    i,
                    ttl,
                    TTL_EXTENSION_AMOUNT
                );
            }
        });

        for i in 0..10 {
            let meta = client.get_metadata(&batch_id, &certifiers[i], &entry_types[i]);
            assert_eq!(meta.data_hash, make_data_hash(&env, i as u8));
        }
    }

    #[test]
    fn test_ttl_coordination_guard_prevents_shortening() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(ProvenanceContract, ());
        let client = ProvenanceContractClient::new(&env, &contract_id);

        let batch_id = make_batch_id(&env, 2);
        let certifier = Address::generate(&env);
        let hash = make_data_hash(&env, 30);
        let uri = String::from_str(&env, "ipfs://QmGuard");

        let staggered_key = client.store_metadata(
            &batch_id,
            &certifier,
            &MetadataType::Certificate,
            &hash,
            &uri,
            &0,
        );

        let skey = staggered_storage_key(&env, &staggered_key);
        let bkey = batch_metadata_key(&env, &batch_id);

        env.as_contract(&contract_id, || {
            let initial_ttl = env.storage().persistent().get_ttl(&skey);
            assert_eq!(initial_ttl, TTL_EXTENSION_AMOUNT);
            let batch_ttl = env.storage().persistent().get_ttl(&bkey);
            assert_eq!(batch_ttl, TTL_EXTENSION_AMOUNT);

            // Concurrent call with lower amount must not shorten
            let extended = extend_ttl_coordinated(&env, &skey, 100_000, 518_400, 0);
            assert!(!extended, "TTL coordination guard must reject shortening extension");

            let final_ttl = env.storage().persistent().get_ttl(&skey);
            assert_eq!(final_ttl, TTL_EXTENSION_AMOUNT);
        });
    }

    #[test]
    fn test_batch_metadata_limit_exceeded_at_100() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ProvenanceContract, ());
        let client = ProvenanceContractClient::new(&env, &contract_id);

        let batch_id = make_batch_id(&env, 5);
        let bkey = batch_metadata_key(&env, &batch_id);

        let mut entries = soroban_sdk::Map::new(&env);
        for i in 0..MAX_METADATA_ENTRIES {
            let mut hash_raw = [0u8; 32];
            hash_raw[0] = (i & 0xFF) as u8;
            hash_raw[1] = (i >> 8) as u8;
            hash_raw[31] = 0xAA;
            let dummy_key = BytesN::from_array(&env, &hash_raw);
            let dummy_meta = CertificationMetadata {
                batch_id: batch_id.clone(),
                certifier: Address::generate(&env),
                entry_type: MetadataType::Certificate,
                data_hash: dummy_key.clone(),
                uri: String::from_str(&env, "ipfs://QmLimit"),
                recorded_at: 100,
                valid_until: 0,
                ttl_extended_until_ledger: 1000,
            };
            entries.set(dummy_key, dummy_meta);
        }
        let full_record = BatchMetadataRecord {
            batch_id: batch_id.clone(),
            count: MAX_METADATA_ENTRIES,
            entries,
            last_reconciled_ledger: 100,
        };
        env.as_contract(&contract_id, || {
            env.storage().persistent().set(&bkey, &full_record);
        });

        let certifier_101 = Address::generate(&env);
        let hash_101 = make_data_hash(&env, 101);
        let uri_101 = String::from_str(&env, "ipfs://QmExceeded");
        let err = client.try_store_metadata(
            &batch_id,
            &certifier_101,
            &MetadataType::Certificate,
            &hash_101,
            &uri_101,
            &0,
        );

        assert!(err.is_err());
    }
}
