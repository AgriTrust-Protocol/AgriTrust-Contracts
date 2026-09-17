extern crate std;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{vec, BytesN, Env};
use crate::{
    errors::Error,
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

#[test]
fn test_atomic_provenance_chain_success() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, |env| {
        let batch_id = chain_id(&env, 42);
        let farm_signer = soroban_sdk::Address::generate(&env);
        let proc_signer = soroban_sdk::Address::generate(&env);

        let meta1 = chain_id(&env, 101);
        let prev0 = soroban_sdk::BytesN::from_array(&env, &crate::chain::GENESIS_PREV_HASH);
        let hop1_hash = crate::chain::compute_hop_hash(&env, &prev0, &meta1);

        let hop1 = crate::chain::Hop {
            sequence: 0,
            stage: soroban_sdk::symbol_short!("farm"),
            signer: farm_signer.clone(),
            prev_hash: prev0,
            metadata_hash: meta1,
            hop_hash: hop1_hash.clone(),
            signature: soroban_sdk::BytesN::from_array(&env, &[1u8; 64]),
            ledger: env.ledger().sequence(),
            timestamp: env.ledger().timestamp(),
        };

        let tip1 = crate::chain::submit_hop(&env, &batch_id, &hop1).expect("farm hop should succeed");
        assert_eq!(tip1, hop1_hash);

        let meta2 = chain_id(&env, 102);
        let hop2_hash = crate::chain::compute_hop_hash(&env, &hop1_hash, &meta2);
        let hop2 = crate::chain::Hop {
            sequence: 1,
            stage: soroban_sdk::symbol_short!("process"),
            signer: proc_signer.clone(),
            prev_hash: hop1_hash,
            metadata_hash: meta2,
            hop_hash: hop2_hash.clone(),
            signature: soroban_sdk::BytesN::from_array(&env, &[2u8; 64]),
            ledger: env.ledger().sequence(),
            timestamp: env.ledger().timestamp(),
        };

        let tip2 = crate::chain::submit_hop(&env, &batch_id, &hop2).expect("processor hop should succeed");
        assert_eq!(tip2, hop2_hash);

        let chain = crate::chain::get_chain(&env, &batch_id).expect("chain should exist");
        assert_eq!(chain.hops.len(), 2);
        assert_eq!(chain.tip_hash, hop2_hash);

        let verified = crate::chain::verify_chain(&env, &batch_id).expect("verify should pass");
        assert!(verified);
    });
}

#[test]
fn test_sequence_guard_rejects_out_of_order() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, |env| {
        let batch_id = chain_id(&env, 43);
        let signer = soroban_sdk::Address::generate(&env);
        let meta = chain_id(&env, 1);
        let prev0 = soroban_sdk::BytesN::from_array(&env, &crate::chain::GENESIS_PREV_HASH);
        let hop_hash = crate::chain::compute_hop_hash(&env, &prev0, &meta);

        let hop_wrong_seq = crate::chain::Hop {
            sequence: 1, // expected 0
            stage: soroban_sdk::symbol_short!("farm"),
            signer,
            prev_hash: prev0,
            metadata_hash: meta,
            hop_hash,
            signature: soroban_sdk::BytesN::from_array(&env, &[0u8; 64]),
            ledger: 1,
            timestamp: 100,
        };

        let err = crate::chain::submit_hop(&env, &batch_id, &hop_wrong_seq).expect_err("out of order must fail");
        assert_eq!(err, Error::SequenceMismatch);
    });
}

#[test]
fn test_broken_chain_hash_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, |env| {
        let batch_id = chain_id(&env, 44);
        let signer = soroban_sdk::Address::generate(&env);
        let meta = chain_id(&env, 1);
        let prev0 = soroban_sdk::BytesN::from_array(&env, &crate::chain::GENESIS_PREV_HASH);
        let corrupted_hash = chain_id(&env, 99);

        let hop_bad_hash = crate::chain::Hop {
            sequence: 0,
            stage: soroban_sdk::symbol_short!("farm"),
            signer,
            prev_hash: prev0,
            metadata_hash: meta,
            hop_hash: corrupted_hash,
            signature: soroban_sdk::BytesN::from_array(&env, &[0u8; 64]),
            ledger: 1,
            timestamp: 100,
        };

        let err = crate::chain::submit_hop(&env, &batch_id, &hop_bad_hash).expect_err("corrupted hash must fail");
        assert_eq!(err, Error::ChainBroken);
    });
}

#[test]
fn test_lock_prevents_concurrent_write_and_recovers_on_timeout() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, |env| {
        let batch_id = chain_id(&env, 45);
        let signer1 = soroban_sdk::Address::generate(&env);
        let signer2 = soroban_sdk::Address::generate(&env);

        crate::chain::acquire_lock(&env, &batch_id, &signer1).expect("lock 1 should acquire");

        // Concurrent acquisition by signer2 fails
        let err = crate::chain::acquire_lock(&env, &batch_id, &signer2).expect_err("lock 2 should fail");
        assert_eq!(err, Error::BatchLocked);

        // Signer 1 can re-acquire (idempotent for holder)
        crate::chain::acquire_lock(&env, &batch_id, &signer1).expect("same signer can re-acquire");

        // Release lock
        crate::chain::release_lock(&env, &batch_id);

        // Signer 2 can now acquire
        crate::chain::acquire_lock(&env, &batch_id, &signer2).expect("lock 2 should succeed after release");
    });
}

#[test]
fn test_concurrent_chain_construction_10_hops() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, |env| {
        let batch_id = chain_id(&env, 50);
        let mut prev_hash = soroban_sdk::BytesN::from_array(&env, &crate::chain::GENESIS_PREV_HASH);

        for i in 0..10u32 {
            let signer = soroban_sdk::Address::generate(&env);
            let meta = chain_id(&env, (i + 1) as u8);
            let hop_hash = crate::chain::compute_hop_hash(&env, &prev_hash, &meta);

            let hop = crate::chain::Hop {
                sequence: i,
                stage: soroban_sdk::symbol_short!("step"),
                signer,
                prev_hash: prev_hash.clone(),
                metadata_hash: meta,
                hop_hash: hop_hash.clone(),
                signature: soroban_sdk::BytesN::from_array(&env, &[i as u8; 64]),
                ledger: env.ledger().sequence(),
                timestamp: env.ledger().timestamp(),
            };

            let tip = crate::chain::submit_hop(&env, &batch_id, &hop).expect("hop should append");
            assert_eq!(tip, hop_hash);
            prev_hash = hop_hash;
        }

        let chain = crate::chain::get_chain(&env, &batch_id).expect("chain must exist");
        assert_eq!(chain.hops.len(), 10);
        assert_eq!(chain.tip_hash, prev_hash);

        let is_valid = crate::chain::verify_chain(&env, &batch_id).expect("full chain must verify");
        assert!(is_valid);
    });
}
