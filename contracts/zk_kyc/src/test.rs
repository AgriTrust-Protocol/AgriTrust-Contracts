use crate::{ZKKYCContract, ZKKYCContractClient};
use soroban_sdk::{
    testutils::{Address as _, BytesN as _},
    Address, Env,
};

// ============================================================================
// Original KYC status tests (preserved)
// ============================================================================

#[test]
fn test_zk_kyc() {
    let env = Env::default();
    env.mock_all_auths();

    let verifier = Address::generate(&env);
    let user = Address::generate(&env);

    let contract_id = env.register_contract(None, ZKKYCContract);
    let client = ZKKYCContractClient::new(&env, &contract_id);

    client.init(&verifier);

    assert_eq!(client.is_verified(&user), false);
    client.verify_user(&user);
    assert_eq!(client.is_verified(&user), true);

    client.revoke_user(&user);
    assert_eq!(client.is_verified(&user), false);
}

// ============================================================================
// Cross-jurisdiction nullifier replay tests (issue #16)
// ============================================================================

use crate::nullifier;

/// Helper: register the contract and a compliance domain, return (env, client, domain_id).
fn setup_with_domain(env: &Env) -> (Env, ZKKYCContractClient<'_>, soroban_sdk::BytesN<32>) {
    let verifier = Address::generate(env);
    let contract_id = env.register_contract(None, ZKKYCContract);
    let client = ZKKYCContractClient::new(env, &contract_id);
    client.init(&verifier);

    let domain_id = soroban_sdk::BytesN::<32>::random(env);
    client.register_compliance_domain(&domain_id);

    (env.clone(), client, domain_id)
}

#[test]
fn test_same_identity_different_domains_nullifiers_differ() {
    let env = Env::default();
    env.mock_all_auths();

    let identity_commitment = soroban_sdk::BytesN::<32>::random(&env);

    // Register domain A
    let domain_a = soroban_sdk::BytesN::<32>::random(&env);
    let domain_b = soroban_sdk::BytesN::<32>::random(&env);

    // Derive nullifiers for both domains
    let n_a = nullifier::derive_nullifier(&env, &identity_commitment, &domain_a);
    let n_b = nullifier::derive_nullifier(&env, &identity_commitment, &domain_b);

    // The two nullifiers must differ
    assert_ne!(
        n_a, n_b,
        "nullifiers across jurisdictions must be distinct"
    );
}

#[test]
fn test_submit_proof_via_client_cross_jurisdiction() {
    let env = Env::default();
    env.mock_all_auths();

    let identity_commitment = soroban_sdk::BytesN::<32>::random(&env);

    let (env, client, domain_a) = setup_with_domain(&env);

    // Register a second domain
    let domain_b = soroban_sdk::BytesN::<32>::random(&env);
    client.register_compliance_domain(&domain_b);

    // Submit same identity in both domains
    let n_a = client.submit_proof(&identity_commitment, &domain_a);
    let n_b = client.submit_proof(&identity_commitment, &domain_b);

    // Nullifiers must differ
    assert_ne!(
        n_a, n_b,
        "nullifiers must differ across compliance domains"
    );
}

#[test]
#[should_panic(expected = "replay")]
fn test_submit_proof_via_client_replay_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let identity_commitment = soroban_sdk::BytesN::<32>::random(&env);
    let (env, client, domain_id) = setup_with_domain(&env);

    // First submission succeeds
    client.submit_proof(&identity_commitment, &domain_id);

    // Replay should panic
    client.submit_proof(&identity_commitment, &domain_id);
}

#[test]
fn test_is_nullifier_consumed_via_client() {
    let env = Env::default();
    env.mock_all_auths();

    let identity_commitment = soroban_sdk::BytesN::<32>::random(&env);
    let (env, client, domain_id) = setup_with_domain(&env);

    // Before submission, nullifier should not be consumed
    let nullifier_hash = nullifier::derive_nullifier(&env, &identity_commitment, &domain_id);
    assert!(!client.is_nullifier_consumed(&domain_id, &nullifier_hash));

    // Submit and verify nullifier is now marked as consumed
    client.submit_proof(&identity_commitment, &domain_id);
    assert!(client.is_nullifier_consumed(&domain_id, &nullifier_hash));
}

#[test]
#[should_panic(expected = "not registered")]
fn test_unregistered_domain_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let identity_commitment = soroban_sdk::BytesN::<32>::random(&env);
    let domain_id = soroban_sdk::BytesN::<32>::random(&env);

    let verifier = Address::generate(&env);
    let contract_id = env.register_contract(None, ZKKYCContract);
    let client = ZKKYCContractClient::new(&env, &contract_id);
    client.init(&verifier);

    // Domain not registered — should panic
    client.submit_proof(&identity_commitment, &domain_id);
}

#[test]
fn test_multiple_users_independent_nullifiers() {
    let env = Env::default();
    env.mock_all_auths();

    let identity_a = soroban_sdk::BytesN::<32>::random(&env);
    let identity_b = soroban_sdk::BytesN::<32>::random(&env);

    let (env, client, domain_id) = setup_with_domain(&env);

    // Both users can submit independently in the same domain
    let n_a = client.submit_proof(&identity_a, &domain_id);
    let n_b = client.submit_proof(&identity_b, &domain_id);
    assert_ne!(n_a, n_b, "different users must produce different nullifiers");
}

#[test]
fn test_domain_registration_via_client() {
    let env = Env::default();
    env.mock_all_auths();

    let (env, client, domain_id) = setup_with_domain(&env);

    assert!(client.is_registered_domain(&domain_id));

    let unregistered = soroban_sdk::BytesN::<32>::random(&env);
    assert!(!client.is_registered_domain(&unregistered));
}

#[test]
fn test_nullifier_deterministic_derivation() {
    let env = Env::default();
    env.mock_all_auths();

    let identity = soroban_sdk::BytesN::<32>::random(&env);
    let domain = soroban_sdk::BytesN::<32>::random(&env);

    let n1 = nullifier::derive_nullifier(&env, &identity, &domain);
    let n2 = nullifier::derive_nullifier(&env, &identity, &domain);

    assert_eq!(n1, n2, "nullifier derivation must be deterministic");
}
