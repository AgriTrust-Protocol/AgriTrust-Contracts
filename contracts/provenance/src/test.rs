extern crate std;
use crate::{
    errors::Error,
    resolver::{get_provenance_result, resolve_provenance, write_hop_state},
    types::{HopState, Score, MAX_HOPS, SCORE_PRECISION, STORAGE_BUDGET, STORAGE_WARN_THRESHOLD},
};
use soroban_sdk::{vec, BytesN, Env};

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
        score: Score {
            raw: SCORE_PRECISION,
        },
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
            score: Score {
                raw: SCORE_PRECISION,
            },
            credential_verified: true,
        };
        let hop_id = BytesN::from_array(&env, &cred);
        write_hop_state(&env, &hop_id, &state);
        let mut ids: soroban_sdk::Vec<BytesN<32>> = vec![&env];
        ids.push_back(hop_id);
        let err =
            resolve_provenance(&env, chain_id(&env, 5), ids).expect_err("zeroed sig must fail");
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
            score: Score {
                raw: SCORE_PRECISION,
            },
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
        assert_eq!(
            persisted.storage_accesses_used,
            result.storage_accesses_used
        );
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
        let err =
            resolve_provenance(&env, cid, too_many_hops).expect_err("oversized chain must fail");
        assert_eq!(err, Error::ChainTooLong);
    });
}

fn b32(env: &Env, seed: u8) -> BytesN<32> {
    let mut raw = [0u8; 32];
    raw[0] = seed;
    BytesN::from_array(env, &raw)
}

#[test]
fn test_supply_chain_token_events_split_and_compliance() {
    use soroban_sdk::testutils::{Address as _, Ledger};
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 1_800_000_000);
    with_contract(&env, |env| {
        let admin = soroban_sdk::Address::generate(&env);
        let owner = soroban_sdk::Address::generate(&env);
        let farm = soroban_sdk::Address::generate(&env);
        let transporter = soroban_sdk::Address::generate(&env);
        let processor = soroban_sdk::Address::generate(&env);
        let issuer = soroban_sdk::Address::generate(&env);
        let issuer2 = soroban_sdk::Address::generate(&env);

        crate::supply_chain::initialize(&env, admin.clone());
        crate::supply_chain::set_authorized_custodian(&env, farm.clone(), true).unwrap();
        crate::supply_chain::set_authorized_custodian(&env, transporter.clone(), true).unwrap();
        crate::supply_chain::set_authorized_custodian(&env, processor.clone(), true).unwrap();

        let organic = b32(&env, 10);
        let fair_trade = b32(&env, 11);
        let standard = b32(&env, 12);
        let required = vec![&env, organic.clone(), fair_trade.clone()];
        crate::supply_chain::register_certificate_type(&env, organic.clone(), true).unwrap();
        crate::supply_chain::register_certificate_type(&env, fair_trade.clone(), true).unwrap();
        crate::supply_chain::set_compliance_standard(&env, standard.clone(), required).unwrap();

        let token_id = crate::supply_chain::mint_batch(
            &env,
            owner.clone(),
            b32(&env, 1),
            b32(&env, 2),
            100,
            farm.clone(),
            1_799_000_000,
            b32(&env, 3),
            1_900_000_000,
        )
        .unwrap();

        crate::supply_chain::add_custody_event(&env, token_id, farm, b32(&env, 20), b32(&env, 30))
            .unwrap();
        crate::supply_chain::add_custody_event(
            &env,
            token_id,
            transporter,
            b32(&env, 21),
            b32(&env, 31),
        )
        .unwrap();
        crate::supply_chain::add_custody_event(
            &env,
            token_id,
            processor,
            b32(&env, 22),
            b32(&env, 32),
        )
        .unwrap();
        crate::supply_chain::attach_certificate(
            &env,
            token_id,
            organic,
            issuer.clone(),
            b32(&env, 40),
            1_850_000_000,
        )
        .unwrap();
        crate::supply_chain::attach_certificate(
            &env,
            token_id,
            fair_trade,
            issuer2,
            b32(&env, 41),
            1_850_000_000,
        )
        .unwrap();

        assert!(crate::supply_chain::verify_compliance(&env, token_id, standard).unwrap());
        let children =
            crate::supply_chain::split(&env, owner, token_id, vec![&env, 40i128, 60i128]).unwrap();
        assert_eq!(children.len(), 2);
        let left = crate::supply_chain::token(&env, children.get_unchecked(0)).unwrap();
        let right = crate::supply_chain::token(&env, children.get_unchecked(1)).unwrap();
        assert_eq!(left.quantity, 40);
        assert_eq!(right.quantity, 60);
        assert_eq!(left.event_ids.len(), 3);
        assert_eq!(right.event_ids.len(), 3);
        assert_eq!(left.certificate_ids.len(), 2);
        assert_eq!(
            crate::supply_chain::token(&env, token_id).expect_err("source burned"),
            Error::TokenBurned
        );
    });
}

#[test]
fn test_soulbound_events_cannot_transfer_and_expired_certificates_are_omitted() {
    use soroban_sdk::testutils::{Address as _, Ledger};
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 2_000);
    with_contract(&env, |env| {
        let admin = soroban_sdk::Address::generate(&env);
        let owner = soroban_sdk::Address::generate(&env);
        let farm = soroban_sdk::Address::generate(&env);
        let issuer = soroban_sdk::Address::generate(&env);
        crate::supply_chain::initialize(&env, admin);
        crate::supply_chain::set_authorized_custodian(&env, farm.clone(), true).unwrap();
        let cert_type = b32(&env, 50);
        crate::supply_chain::register_certificate_type(&env, cert_type.clone(), true).unwrap();
        let token_id = crate::supply_chain::mint_batch(
            &env,
            owner.clone(),
            b32(&env, 51),
            b32(&env, 52),
            10,
            farm.clone(),
            1_900,
            b32(&env, 53),
            0,
        )
        .unwrap();
        let event_id = crate::supply_chain::add_custody_event(
            &env,
            token_id,
            farm,
            b32(&env, 54),
            b32(&env, 55),
        )
        .unwrap();
        assert_eq!(
            crate::supply_chain::transfer_event(&env, owner.clone(), issuer.clone(), event_id)
                .expect_err("soulbound"),
            Error::SoulboundTransfer
        );
        crate::supply_chain::attach_certificate(
            &env,
            token_id,
            cert_type,
            issuer,
            b32(&env, 56),
            2_010,
        )
        .unwrap();
        assert_eq!(
            crate::supply_chain::active_certificates(&env, token_id)
                .unwrap()
                .len(),
            1
        );
        env.ledger().with_mut(|li| li.timestamp = 2_011);
        assert_eq!(
            crate::supply_chain::active_certificates(&env, token_id)
                .unwrap()
                .len(),
            0
        );
    });
}
