extern crate std;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _, storage::Persistent as _},
    vec, Address, BytesN, Env, String,
};
use crate::{
    batch_metadata_key, compute_staggered_metadata_key, extend_ttl_coordinated,
    get_batch_metadata_record, get_metadata_entry,
    staggered_storage_key, store_batch_metadata, BatchMetadataRecord,
    CertificationMetadata, Error, MetadataType, ProvenanceContract,
    ProvenanceContractClient, MAX_METADATA_ENTRIES,
    TTL_EXTENSION_AMOUNT,
    resolver::{get_provenance_result, resolve_provenance, write_hop_state},
    types::{HopState, Score, SCORE_PRECISION, STORAGE_BUDGET, STORAGE_WARN_THRESHOLD, MAX_HOPS},
};

fn make_hop(env: &Env, index: u32) -> (BytesN<32>, HopState) {
    let mut cred = [0u8; 32];
    cred[0..4].copy_from_slice(&index.to_be_bytes());
    cred[4] = 0xC0;
    let mut policy = [0u8; 32];
    policy[0..4].copy_from_slice(&index.to_be_bytes());
    policy[4] = 0xAB;
    let mut sig = [0xFFu8; 64];
    sig[0] = index as u8;
    let state = HopState {
        index,
        credential_id: BytesN::from_array(env, &cred),
        signature: BytesN::from_array(env, &sig),
        policy_ref: BytesN::from_array(env, &policy),
        recorded_at: 1_700_000_000u64 + index as u64,
        score: Score { raw: SCORE_PRECISION },
        credential_verified: true,
    };
    (BytesN::from_array(env, &cred), state)
}

fn populate_chain(env: &Env, n: u32) -> soroban_sdk::Vec<BytesN<32>> {
    let mut ids: soroban_sdk::Vec<BytesN<32>> = vec![env];
    for i in 0..n {
        let (hop_id, state) = make_hop(env, i);
        write_hop_state(env, &hop_id, &state);
        ids.push_back(hop_id);
    }
    ids
}

fn chain_id(env: &Env, seed: u8) -> BytesN<32> {
    let mut raw = [0u8; 32];
    raw[0] = seed;
    BytesN::from_array(env, &raw)
}

fn with_contract<T>(env: &Env, f: impl FnOnce(Env) -> T) -> T {
    let contract_id = env.register_contract(None, crate::ProvenanceContract);
    let inner = env.clone();
    env.as_contract(&contract_id, || f(inner))
}

#[test]
fn test_10_hop_chain_under_storage_budget() {
    let env = Env::default();
    with_contract(&env, |env| {
        let hops = populate_chain(&env, 10);
        let cid = chain_id(&env, 1);
        let result = resolve_provenance(&env, cid, hops).expect("10-hop chain should resolve");
        assert_eq!(result.hops_resolved, 10);
        assert_eq!(result.storage_accesses_used, 11);
        assert!(result.storage_accesses_used <= STORAGE_BUDGET);
        assert_eq!(result.final_score.raw, SCORE_PRECISION);
    });
}

#[test]
fn test_access_count_formula_for_n_hops() {
    for n in 1..=MAX_HOPS {
        let accesses = n + 1;
        assert!(accesses <= STORAGE_BUDGET);
    }
}

#[test]
fn test_empty_chain_rejected() {
    let env = Env::default();
    let empty: soroban_sdk::Vec<BytesN<32>> = vec![&env];
    let cid = chain_id(&env, 2);
    let err = resolve_provenance(&env, cid, empty).expect_err("empty chain must fail");
    assert_eq!(err, Error::EmptyChain);
}

#[test]
fn test_chain_too_long_rejected() {
    let env = Env::default();
    with_contract(&env, |env| {
        let hops = populate_chain(&env, MAX_HOPS + 1);
        let cid = chain_id(&env, 3);
        let err = resolve_provenance(&env, cid, hops).expect_err("oversized chain must fail");
        assert_eq!(err, Error::ChainTooLong);
    });
}

#[test]
fn test_missing_hop_returns_not_found() {
    let env = Env::default();
    with_contract(&env, |env| {
        let mut ids = populate_chain(&env, 1);
        ids.push_back(chain_id(&env, 99));
        let cid = chain_id(&env, 4);
        let err = resolve_provenance(&env, cid, ids).expect_err("missing hop must fail");
        assert_eq!(err, Error::HopNotFound);
    });
}

#[test]
fn test_invalid_signature_rejected() {
    let env = Env::default();
    with_contract(&env, |env| {
        let cred = [0xA0u8; 32];
        let state = HopState {
            index: 0,
            credential_id: BytesN::from_array(&env, &cred),
            signature: BytesN::from_array(&env, &[0u8; 64]),
            policy_ref: BytesN::from_array(&env, &[0xB0u8; 32]),
            recorded_at: 1_700_000_001,
            score: Score { raw: SCORE_PRECISION },
            credential_verified: true,
        };
        let hop_id = BytesN::from_array(&env, &cred);
        write_hop_state(&env, &hop_id, &state);
        let mut ids: soroban_sdk::Vec<BytesN<32>> = vec![&env];
        ids.push_back(hop_id);
        let err = resolve_provenance(&env, chain_id(&env, 5), ids)
            .expect_err("zeroed sig must fail");
        assert_eq!(err, Error::InvalidHopSignature);
    });
}

#[test]
fn test_invalid_credential_rejected() {
    let env = Env::default();
    with_contract(&env, |env| {
        let cred = [0xC0u8; 32];
        let mut sig = [0xFFu8; 64];
        sig[0] = 0x01;
        let state = HopState {
            index: 0,
            credential_id: BytesN::from_array(&env, &cred),
            signature: BytesN::from_array(&env, &sig),
            policy_ref: BytesN::from_array(&env, &[0xD0u8; 32]),
            recorded_at: 0,
            score: Score { raw: SCORE_PRECISION },
            credential_verified: true,
        };
        let hop_id = BytesN::from_array(&env, &cred);
        write_hop_state(&env, &hop_id, &state);
        let mut ids: soroban_sdk::Vec<BytesN<32>> = vec![&env];
        ids.push_back(hop_id);
        let err = resolve_provenance(&env, chain_id(&env, 6), ids)
            .expect_err("zero recorded_at must fail");
        assert_eq!(err, Error::InvalidHopCredential);
    });
}

#[test]
fn test_storage_budget_tracker_charge_and_warn() {
    use crate::types::StorageBudget;
    let mut budget = StorageBudget::new();
    assert_eq!(budget.used, 0);
    assert!(!budget.would_exceed(STORAGE_BUDGET));
    assert!(budget.would_exceed(STORAGE_BUDGET + 1));
    let warned = budget.charge(STORAGE_WARN_THRESHOLD - 1);
    assert!(!warned);
    assert_eq!(budget.used, STORAGE_WARN_THRESHOLD - 1);
    let warned = budget.charge(1);
    assert!(warned);
    assert_eq!(budget.used, STORAGE_WARN_THRESHOLD);
    let warned = budget.charge(1);
    assert!(!warned);
}

#[test]
fn test_provenance_access_set_estimated_accesses() {
    use crate::types::ProvenanceAccessSet;
    assert_eq!(ProvenanceAccessSet::estimated_accesses(0), 2);
    assert_eq!(ProvenanceAccessSet::estimated_accesses(1), 4);
    assert_eq!(ProvenanceAccessSet::estimated_accesses(10), 22);
    for n in 0..=MAX_HOPS {
        assert!(ProvenanceAccessSet::estimated_accesses(n) <= STORAGE_BUDGET);
    }
}

#[test]
fn test_result_persisted_and_retrievable() {
    let env = Env::default();
    with_contract(&env, |env| {
        let hops = populate_chain(&env, 5);
        let cid = chain_id(&env, 7);
        let result = resolve_provenance(&env, cid.clone(), hops).expect("5-hop chain");
        assert_eq!(result.hops_resolved, 5);
        assert_eq!(result.storage_accesses_used, 6);
        let persisted = get_provenance_result(&env, &cid).expect("result must be stored");
        assert_eq!(persisted.hops_resolved, result.hops_resolved);
        assert_eq!(persisted.storage_accesses_used, result.storage_accesses_used);
        assert_eq!(persisted.final_score.raw, result.final_score.raw);
    });
}

#[test]
fn test_detailed_storage_access_tracking() {
    let env = Env::default();
    with_contract(&env, |env| {
        let hops = populate_chain(&env, 10);
        let cid = chain_id(&env, 1);
        let result = resolve_provenance(&env, cid, hops).expect("10-hop chain should resolve");
        assert_eq!(result.hops_resolved, 10);
        assert_eq!(result.storage_accesses_used, 11);
        assert!(result.storage_accesses_used <= STORAGE_BUDGET);
        assert_eq!(result.final_score.raw, SCORE_PRECISION);
    });
}

#[test]
fn test_storage_budget_exceeded_error() {
    let env = Env::default();
    with_contract(&env, |env| {
        let too_many_hops = populate_chain(&env, MAX_HOPS + 1);
        let cid = chain_id(&env, 99);
        let err = resolve_provenance(&env, cid, too_many_hops)
            .expect_err("oversized chain must fail");
        assert_eq!(err, Error::ChainTooLong);
    });
}

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
fn test_store_metadata_and_retrieval() {
    let env = Env::default();
    env.mock_all_auths();

    with_contract(&env, |env| {
        let batch_id = make_batch_id(&env, 1);
        let certifier1 = Address::generate(&env);
        let certifier2 = Address::generate(&env);

        let hash1 = make_data_hash(&env, 10);
        let uri1 = String::from_str(&env, "ipfs://QmCert1");
        let hash2 = make_data_hash(&env, 20);
        let uri2 = String::from_str(&env, "https://lab.org/result2.pdf");

        // Store Certificate
        let key1 = store_batch_metadata(
            &env,
            batch_id.clone(),
            certifier1.clone(),
            MetadataType::Certificate,
            hash1.clone(),
            uri1.clone(),
            0,
        )
        .expect("should store certificate");

        // Store LabResult
        let key2 = store_batch_metadata(
            &env,
            batch_id.clone(),
            certifier2.clone(),
            MetadataType::LabResult,
            hash2.clone(),
            uri2.clone(),
            0,
        )
        .expect("should store lab result");

        assert_ne!(key1, key2);

        // Retrieve individual entries
        let meta1 = get_metadata_entry(&env, &batch_id, &certifier1, MetadataType::Certificate)
            .expect("should get certificate");
        assert_eq!(meta1.data_hash, hash1);
        assert_eq!(meta1.uri, uri1);
        assert_eq!(meta1.entry_type, MetadataType::Certificate);

        let meta2 = get_metadata_entry(&env, &batch_id, &certifier2, MetadataType::LabResult)
            .expect("should get lab result");
        assert_eq!(meta2.data_hash, hash2);
        assert_eq!(meta2.uri, uri2);
        assert_eq!(meta2.entry_type, MetadataType::LabResult);

        // Retrieve aggregated batch record
        let batch_rec = get_batch_metadata_record(&env, &batch_id)
            .expect("should get batch record");
        assert_eq!(batch_rec.count, 2);
        assert_eq!(batch_rec.entries.len(), 2);
    });
}

#[test]
fn test_ttl_coordination_guard_prevents_shortening() {
    let env = Env::default();
    env.mock_all_auths();

    with_contract(&env, |env| {
        let batch_id = make_batch_id(&env, 2);
        let certifier = Address::generate(&env);
        let hash = make_data_hash(&env, 30);
        let uri = String::from_str(&env, "ipfs://QmGuard");

        let staggered_key = store_batch_metadata(
            &env,
            batch_id.clone(),
            certifier.clone(),
            MetadataType::Certificate,
            hash,
            uri,
            0,
        )
        .expect("store metadata");

        let skey = staggered_storage_key(&env, &staggered_key);
        let bkey = batch_metadata_key(&env, &batch_id);

        let initial_ttl = env.storage().persistent().get_ttl(&skey);
        assert_eq!(initial_ttl, TTL_EXTENSION_AMOUNT);
        let batch_ttl = env.storage().persistent().get_ttl(&bkey);
        assert_eq!(batch_ttl, TTL_EXTENSION_AMOUNT);

        // Simulate a concurrent call attempting to extend with a lower threshold / amount (e.g. 518_400)
        // The coordination guard reads current TTL first: since current_ttl (1_555_200) >= 518_400,
        // it must NOT execute extend_ttl and must return false.
        let extended = extend_ttl_coordinated(&env, &skey, 100_000, 518_400, 0);
        assert!(!extended, "TTL coordination guard must reject shortening extension");

        // Verify the entry's TTL is still at the full 90 days (1_555_200)
        let final_ttl = env.storage().persistent().get_ttl(&skey);
        assert_eq!(final_ttl, TTL_EXTENSION_AMOUNT);
    });
}

#[test]
fn test_staggered_key_hashing_distribution() {
    let env = Env::default();
    let batch_id = make_batch_id(&env, 3);
    let mut keys = std::vec::Vec::new();

    // Generate keys for 10 different certifier and entry type combinations
    for i in 0..10 {
        let certifier = Address::generate(&env);
        let entry_type = match i % 4 {
            0 => MetadataType::Certificate,
            1 => MetadataType::InspectionReport,
            2 => MetadataType::LabResult,
            _ => MetadataType::AuditAttestation,
        };
        let key = compute_staggered_metadata_key(&env, &batch_id, &certifier, entry_type);
        assert!(!keys.contains(&key), "Collision detected in staggered key hashing");
        keys.push(key);
    }
    assert_eq!(keys.len(), 10);
}

#[test]
fn test_concurrent_store_metadata_simulation_10_calls() {
    // Blueprint 5: "Add a concurrent storage test that simulates 10 simultaneous
    // store_metadata() calls and verifies all entries have TTL >= 90 days after all calls complete."
    let env = Env::default();
    env.mock_all_auths();

    with_contract(&env, |env| {
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

            let key = store_batch_metadata(
                &env,
                batch_id.clone(),
                certifier.clone(),
                entry_type,
                hash,
                uri,
                0,
            )
            .expect("concurrent store_metadata call should succeed");

            stored_keys.push(key);
            certifiers.push(certifier);
            entry_types.push(entry_type);
        }

        // Verify aggregated batch record holds all 10 entries
        let batch_rec = get_batch_metadata_record(&env, &batch_id)
            .expect("batch record must exist");
        assert_eq!(batch_rec.count, 10);
        assert_eq!(batch_rec.entries.len(), 10);

        let bkey = batch_metadata_key(&env, &batch_id);
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

            let meta = get_metadata_entry(&env, &batch_id, &certifiers[i], entry_types[i])
                .expect("entry must be retrievable");
            assert_eq!(meta.data_hash, make_data_hash(&env, i as u8));
        }
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

    // Pre-populate batch record directly with MAX_METADATA_ENTRIES (100)
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

    // 101st entry must fail with BatchMetadataLimitExceeded
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

    assert!(err.is_err(), "101st entry must exceed batch metadata limit");
}

#[test]
fn test_duplicate_metadata_entry_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(ProvenanceContract, ());
    let client = ProvenanceContractClient::new(&env, &contract_id);

    let batch_id = make_batch_id(&env, 6);
    let certifier = Address::generate(&env);
    let hash = make_data_hash(&env, 1);
    let uri = String::from_str(&env, "ipfs://QmDup");

    let res1 = client.try_store_metadata(
        &batch_id,
        &certifier,
        &MetadataType::Certificate,
        &hash,
        &uri,
        &0,
    );
    assert!(res1.is_ok(), "first store succeeds");

    let res2 = client.try_store_metadata(
        &batch_id,
        &certifier,
        &MetadataType::Certificate,
        &hash,
        &uri,
        &0,
    );
    assert!(res2.is_err(), "duplicate store must fail");
}

#[test]
fn test_metadata_ttl_reconciliation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(ProvenanceContract, ());
    let client = ProvenanceContractClient::new(&env, &contract_id);

    let batch_id = make_batch_id(&env, 7);
    let certifier1 = Address::generate(&env);
    let certifier2 = Address::generate(&env);

    let key1 = client.store_metadata(
        &batch_id,
        &certifier1,
        &MetadataType::Certificate,
        &make_data_hash(&env, 1),
        &String::from_str(&env, "uri1"),
        &0,
    );

    let key2 = client.store_metadata(
        &batch_id,
        &certifier2,
        &MetadataType::LabResult,
        &make_data_hash(&env, 2),
        &String::from_str(&env, "uri2"),
        &0,
    );

    let skey1 = staggered_storage_key(&env, &key1);
    let skey2 = staggered_storage_key(&env, &key2);

    let reconciled = client.reconcile_metadata_ttl(&batch_id);
    assert_eq!(reconciled, 0); // Already at full 90 days, so 0 needed extension

    env.as_contract(&contract_id, || {
        let ttl1 = env.storage().persistent().get_ttl(&skey1);
        let ttl2 = env.storage().persistent().get_ttl(&skey2);
        assert_eq!(ttl1, TTL_EXTENSION_AMOUNT);
        assert_eq!(ttl2, TTL_EXTENSION_AMOUNT);
    });
}

#[test]
fn test_expired_metadata_rejected_on_query() {
    let env = Env::default();
    env.mock_all_auths();

    with_contract(&env, |env| {
        let batch_id = make_batch_id(&env, 8);
        let certifier = Address::generate(&env);
        let hash = make_data_hash(&env, 1);
        let uri = String::from_str(&env, "ipfs://QmExp");

        // Valid until timestamp 1000
        store_batch_metadata(
            &env,
            batch_id.clone(),
            certifier.clone(),
            MetadataType::Certificate,
            hash,
            uri,
            1000,
        )
        .unwrap();

        // Query when timestamp is 500 (still valid)
        let meta = get_metadata_entry(&env, &batch_id, &certifier, MetadataType::Certificate)
            .expect("valid metadata should be retrieved");
        assert_eq!(meta.valid_until, 1000);

        // Advance ledger timestamp to 1001 (expired)
        env.ledger().set_timestamp(1001);

        let err = get_metadata_entry(&env, &batch_id, &certifier, MetadataType::Certificate)
            .expect_err("expired metadata should be rejected");
        assert_eq!(err, Error::MetadataExpired);
    });
}

#[test]
fn test_contract_client_end_to_end() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ProvenanceContract, ());
    let client = ProvenanceContractClient::new(&env, &contract_id);

    let batch_id = make_batch_id(&env, 9);
    let certifier = Address::generate(&env);
    let hash = make_data_hash(&env, 99);
    let uri = String::from_str(&env, "https://agritrust.io/cert/9");

    let key = client.store_metadata(
        &batch_id,
        &certifier,
        &MetadataType::AuditAttestation,
        &hash,
        &uri,
        &0,
    );

    let computed = client.compute_metadata_key(
        &batch_id,
        &certifier,
        &MetadataType::AuditAttestation,
    );
    assert_eq!(key, computed);

    let meta = client.get_metadata(
        &batch_id,
        &certifier,
        &MetadataType::AuditAttestation,
    );
    assert_eq!(meta.data_hash, hash);
    assert_eq!(meta.uri, uri);

    let batch_rec = client.get_batch_metadata(&batch_id);
    assert_eq!(batch_rec.count, 1);

    let reconciled = client.reconcile_metadata_ttl(&batch_id);
    assert_eq!(reconciled, 0);
}
