#![cfg(test)]

use super::*;

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
    Address, BytesN, Env, InvokeError, String, Symbol, TryFromVal, Val,
};

const DID: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const POLICY_URI: &str = "ipfs://bafybeigdyrztw5k7cgvxs3v4hdcbvhbxcvbjrq";

fn bytes(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

fn did(env: &Env) -> String {
    String::from_str(env, DID)
}

fn uri(env: &Env) -> String {
    String::from_str(env, POLICY_URI)
}

/// A fully deployed engine: one admin, one farmer who has anchored a coffee
/// batch twin, and one ODRL policy committed under its digest.
///
/// Authorizations are cleared after construction, so each test must opt in
/// explicitly. That keeps "nobody signed" distinguishable from "the wrong
/// principal signed".
struct World {
    env: Env,
    client: ContractClient<'static>,
    admin: Address,
    farmer: Address,
    consumer: Address,
    asset_id: BytesN<32>,
    policy_hash: BytesN<32>,
    merkle_root: BytesN<32>,
}

impl World {
    fn new() -> Self {
        World::new_at(0)
    }

    /// Deploy at a non-zero ledger so that validity windows can be expressed
    /// relative to an already-advanced chain.
    fn new_at(sequence: u32) -> Self {
        let env = Env::default();
        // Set before registration: the host anchors entry lifetimes to the
        // ledger that creates them, so jumping afterwards would archive the
        // contract instance.
        env.ledger().set_sequence_number(sequence);
        env.mock_all_auths();

        let contract_id = env.register(Contract, ());
        let client: ContractClient<'static> = ContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let farmer = Address::generate(&env);
        let consumer = Address::generate(&env);
        let asset_id = bytes(&env, 0xA1);
        let policy_hash = bytes(&env, 0xB2);
        let merkle_root = bytes(&env, 0xC3);

        client.initialize(&admin);
        client.register_asset_twin(
            &farmer,
            &asset_id,
            &did(&env),
            &symbol_short!("COFFEE"),
            &merkle_root,
        );
        client.register_odrl_policy(&farmer, &policy_hash, &uri(&env));

        env.set_auths(&[]);

        World {
            env,
            client,
            admin,
            farmer,
            consumer,
            asset_id,
            policy_hash,
            merkle_root,
        }
    }

    /// Grant the consumer a data contract over the farmer's batch.
    fn grant(&self, contract_id: &BytesN<32>, duration_ledgers: u32) {
        self.env.mock_all_auths();
        self.client.create_data_contract(
            &self.farmer,
            &self.consumer,
            contract_id,
            &self.asset_id,
            &self.policy_hash,
            &5_000i128,
            &duration_ledgers,
        );
        self.env.set_auths(&[]);
    }

    fn grant_record(&self, contract_id: &BytesN<32>, duration_ledgers: u32) -> DataContractRecord {
        self.grant(contract_id, duration_ledgers);
        self.client.get_data_contract(contract_id).unwrap()
    }
}

/// The invocation was rejected by the host because the required principal did
/// not sign. This is the layer that turns "someone else pressed the button"
/// into a hard rejection, independently of any engine-level rule.
fn assert_unsigned<T: core::fmt::Debug, C: core::fmt::Debug>(
    result: Result<Result<T, C>, Result<ContractError, InvokeError>>,
) {
    match result {
        Err(Err(_)) => {}
        other => panic!("expected host-level auth rejection, got {other:?}"),
    }
}

/// Assert the engine rejected the call with a specific governance error.
fn assert_denied<T: core::fmt::Debug, C: core::fmt::Debug>(
    result: Result<Result<T, C>, Result<ContractError, InvokeError>>,
    expected: ContractError,
) {
    match result {
        Err(Ok(actual)) => assert_eq!(actual, expected),
        other => panic!("expected {expected:?}, got {other:?}"),
    }
}

/// Pop the most recently published event.
fn last_event(env: &Env) -> (Address, soroban_sdk::Vec<Val>, Val) {
    let all = env.events().all();
    all.get(all.len() - 1).expect("an event was published")
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

#[test]
fn initialize_anchors_admin_exactly_once() {
    let w = World::new();

    assert_eq!(w.client.get_admin(), Some(w.admin.clone()));

    w.env.mock_all_auths();
    assert_denied(w.client.try_initialize(&w.admin), ContractError::AlreadyInitialized);

    // A rejected re-initialization must not re-point governance.
    assert_eq!(w.client.get_admin(), Some(w.admin.clone()));
}

#[test]
fn initialize_rejects_unsigned_admin() {
    let env = Env::default();
    let contract_id = env.register(Contract, ());
    let client: ContractClient<'static> = ContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    assert_unsigned(client.try_initialize(&admin));
    assert_eq!(client.get_admin(), None);
}

// ---------------------------------------------------------------------------
// Asset twin registration (:Asset / :Token)
// ---------------------------------------------------------------------------

#[test]
fn register_asset_twin_anchors_physical_to_digital_binding() {
    let w = World::new();

    let twin = w.client.get_asset_twin(&w.asset_id).expect("twin is anchored");

    assert_eq!(twin.asset_id, w.asset_id);
    assert_eq!(twin.originator, w.farmer);
    assert_eq!(twin.originator_did, did(&w.env));
    assert_eq!(twin.commodity_type, symbol_short!("COFFEE"));
    assert_eq!(twin.provenance_merkle_root, w.merkle_root);
    assert_eq!(twin.registered_at, w.env.ledger().timestamp());
}

#[test]
fn register_asset_twin_persists_timestamp_and_commitment() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Contract, ());
    let client: ContractClient<'static> = ContractClient::new(&env, &contract_id);

    let farmer = Address::generate(&env);
    let asset_id = bytes(&env, 0x11);
    let root = bytes(&env, 0x22);
    env.ledger().set_timestamp(1_700_000_000);

    client.register_asset_twin(
        &farmer,
        &asset_id,
        &did(&env),
        &symbol_short!("SOYA"),
        &root,
    );

    let twin = client.get_asset_twin(&asset_id).unwrap();
    assert_eq!(twin.registered_at, 1_700_000_000);
    assert_eq!(twin.provenance_merkle_root, root);
    assert_eq!(twin.commodity_type, symbol_short!("SOYA"));
    assert_eq!(twin.originator_did, did(&env));
    assert_eq!(twin.originator, farmer);
}

#[test]
fn register_asset_twin_emits_provenance_event() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Contract, ());
    let client: ContractClient<'static> = ContractClient::new(&env, &contract_id);

    let farmer = Address::generate(&env);
    let asset_id = bytes(&env, 0x33);
    let root = bytes(&env, 0x44);

    client.register_asset_twin(
        &farmer,
        &asset_id,
        &did(&env),
        &symbol_short!("CATTLE"),
        &root,
    );

    let (_, topics, data) = last_event(&env);

    // topics = (symbol_short!("twin_reg"), originator)
    assert_eq!(topics.len(), 2);
    let topic: Symbol = Symbol::try_from_val(&env, &topics.get_unchecked(0)).unwrap();
    let who: Address = Address::try_from_val(&env, &topics.get_unchecked(1)).unwrap();
    assert_eq!(topic, symbol_short!("twin_reg"));
    assert_eq!(who, farmer);

    // data = (asset_id, commodity, timestamp)
    let payload: (BytesN<32>, Symbol, u64) =
        <(BytesN<32>, Symbol, u64)>::try_from_val(&env, &data).unwrap();
    assert_eq!(
        payload,
        (asset_id, symbol_short!("CATTLE"), env.ledger().timestamp())
    );
}

#[test]
fn register_asset_twin_requires_originator_signature() {
    let w = World::new();
    let asset_id = bytes(&w.env, 0x55);

    // Nobody has authorized this invocation.
    assert_unsigned(
        w.client.try_register_asset_twin(
            &w.farmer,
            &asset_id,
            &did(&w.env),
            &symbol_short!("CATTLE"),
            &bytes(&w.env, 0x66),
        ),
    );
    assert!(w.client.get_asset_twin(&asset_id).is_none());
}

#[test]
fn register_asset_twin_rejects_hijack_of_existing_batch() {
    let w = World::new();
    let interloper = Address::generate(&w.env);
    let forged_root = bytes(&w.env, 0x77);
    w.env.mock_all_auths();

    assert_denied(
        w.client.try_register_asset_twin(
            &interloper,
            &w.asset_id,
            &did(&w.env),
            &symbol_short!("COFFEE"),
            &forged_root,
        ),
        ContractError::Unauthorized,
    );

    // The farmer's original provenance commitment must survive untouched.
    let twin = w.client.get_asset_twin(&w.asset_id).unwrap();
    assert_eq!(twin.provenance_merkle_root, w.merkle_root);
    assert_ne!(twin.provenance_merkle_root, forged_root);
    assert_eq!(twin.originator, w.farmer);
}

#[test]
fn register_asset_twin_allows_originator_to_correct_commitment() {
    let w = World::new();
    let corrected = bytes(&w.env, 0x88);
    w.env.mock_all_auths();

    w.client.register_asset_twin(
        &w.farmer,
        &w.asset_id,
        &did(&w.env),
        &symbol_short!("COFFEE"),
        &corrected,
    );

    assert_eq!(
        w.client
            .get_asset_twin(&w.asset_id)
            .unwrap()
            .provenance_merkle_root,
        corrected
    );
}

// ---------------------------------------------------------------------------
// ODRL policy anchoring (:ODRLPolicy)
// ---------------------------------------------------------------------------

#[test]
fn odrl_policy_is_anchored_by_its_issuer() {
    let w = World::new();

    let policy = w.client.get_odrl_policy(&w.policy_hash).unwrap();
    assert_eq!(policy.policy_hash, w.policy_hash);
    assert_eq!(policy.issuer, w.farmer);
    assert_eq!(policy.policy_uri, uri(&w.env));
    assert_eq!(policy.registered_at, w.env.ledger().timestamp());
}

#[test]
fn odrl_policy_requires_issuer_signature() {
    let w = World::new();
    let digest = bytes(&w.env, 0x99);

    assert_unsigned(
        w.client
            .try_register_odrl_policy(&w.farmer, &digest, &uri(&w.env)),
    );
    assert!(w.client.get_odrl_policy(&digest).is_none());
}

// ---------------------------------------------------------------------------
// Data contract authorization (:DataContract)
// ---------------------------------------------------------------------------

#[test]
fn create_data_contract_grants_bounded_encumbered_access() {
    let w = World::new();
    let cid = bytes(&w.env, 0x01);
    w.env.mock_all_auths();

    w.client.create_data_contract(
        &w.farmer,
        &w.consumer,
        &cid,
        &w.asset_id,
        &w.policy_hash,
        &25_000i128,
        &5_000u32,
    );

    let grant = w.client.get_data_contract(&cid).unwrap();
    assert_eq!(grant.contract_id, cid);
    assert_eq!(grant.asset_id, w.asset_id);
    assert_eq!(grant.data_consumer, w.consumer);
    assert_eq!(grant.odrl_policy_hash, w.policy_hash);
    assert_eq!(grant.access_fee, 25_000i128);
    assert_eq!(grant.valid_until_ledger, 5_000u32);
    assert!(!grant.is_revoked);
}

#[test]
fn create_data_contract_window_starts_at_current_ledger() {
    let w = World::new_at(7_000);
    let cid = bytes(&w.env, 0x1A);
    w.env.mock_all_auths();

    w.client.create_data_contract(
        &w.farmer,
        &w.consumer,
        &cid,
        &w.asset_id,
        &w.policy_hash,
        &1i128,
        &500u32,
    );

    assert_eq!(
        w.client.get_data_contract(&cid).unwrap().valid_until_ledger,
        7_500u32
    );
}

#[test]
fn create_data_contract_emits_agreement_event() {
    let w = World::new();
    let cid = bytes(&w.env, 0x02);
    w.env.mock_all_auths();

    w.client.create_data_contract(
        &w.farmer,
        &w.consumer,
        &cid,
        &w.asset_id,
        &w.policy_hash,
        &1_000i128,
        &100u32,
    );

    let (_, topics, data) = last_event(&w.env);

    // topics = (symbol_short!("contract"), farmer)
    assert_eq!(topics.len(), 2);
    let topic: Symbol = Symbol::try_from_val(&w.env, &topics.get_unchecked(0)).unwrap();
    let who: Address = Address::try_from_val(&w.env, &topics.get_unchecked(1)).unwrap();
    assert_eq!(topic, symbol_short!("contract"));
    assert_eq!(who, w.farmer);

    // data = (contract_id, consumer, policy_hash)
    let payload: (BytesN<32>, Address, BytesN<32>) =
        <(BytesN<32>, Address, BytesN<32>)>::try_from_val(&w.env, &data).unwrap();
    assert_eq!(payload, (cid.clone(), w.consumer.clone(), w.policy_hash.clone()));
}

#[test]
fn create_data_contract_requires_farmer_signature() {
    let w = World::new();
    let cid = bytes(&w.env, 0x03);

    assert_unsigned(
        w.client.try_create_data_contract(
            &w.farmer,
            &w.consumer,
            &cid,
            &w.asset_id,
            &w.policy_hash,
            &1_000i128,
            &100u32,
        ),
    );
    assert!(w.client.get_data_contract(&cid).is_none());
}

#[test]
fn create_data_contract_rejects_corporation_licensing_farmers_batch() {
    let w = World::new();
    let cid = bytes(&w.env, 0x04);
    w.env.mock_all_auths();

    // The corporation signs, but it is not the batch originator.
    assert_denied(
        w.client.try_create_data_contract(
            &w.consumer,
            &w.consumer,
            &cid,
            &w.asset_id,
            &w.policy_hash,
            &1i128,
            &100u32,
        ),
        ContractError::Unauthorized,
    );
    assert!(w.client.get_data_contract(&cid).is_none());
}

#[test]
fn create_data_contract_requires_existing_asset_twin() {
    let w = World::new();
    let cid = bytes(&w.env, 0x05);
    w.env.mock_all_auths();

    assert_denied(
        w.client.try_create_data_contract(
            &w.farmer,
            &w.consumer,
            &cid,
            &bytes(&w.env, 0xEE),
            &w.policy_hash,
            &1i128,
            &100u32,
        ),
        ContractError::AssetNotFound,
    );
}

#[test]
fn create_data_contract_rejects_unanchored_odrl_policy() {
    let w = World::new();
    let cid = bytes(&w.env, 0x06);
    let rogue_policy = bytes(&w.env, 0xAA);
    w.env.mock_all_auths();

    assert_denied(
        w.client.try_create_data_contract(
            &w.farmer,
            &w.consumer,
            &cid,
            &w.asset_id,
            &rogue_policy,
            &1i128,
            &100u32,
        ),
        ContractError::PolicyViolation,
    );
    assert!(w.client.get_data_contract(&cid).is_none());
}

#[test]
fn create_data_contract_blocks_reissue_of_a_foreign_agreement_id() {
    let w = World::new();
    let cid = bytes(&w.env, 0x07);
    w.grant(&cid, 100u32);

    // A second farmer must not be able to overwrite an existing agreement id.
    let other_farmer = Address::generate(&w.env);
    let other_asset = bytes(&w.env, 0xAB);
    w.env.mock_all_auths();
    w.client.register_asset_twin(
        &other_farmer,
        &other_asset,
        &did(&w.env),
        &symbol_short!("CATTLE"),
        &bytes(&w.env, 0xCD),
    );

    assert_denied(
        w.client.try_create_data_contract(
            &other_farmer,
            &w.consumer,
            &cid,
            &other_asset,
            &w.policy_hash,
            &1i128,
            &100u32,
        ),
        ContractError::Unauthorized,
    );

    // The original agreement must be intact.
    let grant = w.client.get_data_contract(&cid).unwrap();
    assert_eq!(grant.asset_id, w.asset_id);
    assert_eq!(grant.data_consumer, w.consumer);
    assert_eq!(grant.valid_until_ledger, 100u32);
}

// ---------------------------------------------------------------------------
// Unilateral revocation (Farmer Data Sovereignty)
// ---------------------------------------------------------------------------

#[test]
fn farmer_can_unilaterally_revoke_corporate_access() {
    let w = World::new();
    let cid = bytes(&w.env, 0x0A);
    w.grant(&cid, 1_000u32);

    // Access is live before revocation.
    w.env.mock_all_auths();
    assert_eq!(w.client.verify_data_access(&w.consumer, &cid), w.merkle_root);

    // The farmer terminates the grant; the corporation never consents.
    w.env.mock_all_auths();
    w.client.revoke_data_access(&w.farmer, &cid);
    assert!(w.client.get_data_contract(&cid).unwrap().is_revoked);

    w.env.mock_all_auths();
    assert_denied(
        w.client.try_verify_data_access(&w.consumer, &cid),
        ContractError::AccessRevoked,
    );
}

#[test]
fn revoke_emits_sovereignty_event() {
    let w = World::new();
    let cid = bytes(&w.env, 0x0B);
    w.grant(&cid, 1_000u32);

    w.env.mock_all_auths();
    w.client.revoke_data_access(&w.farmer, &cid);

    let (_, topics, data) = last_event(&w.env);

    // topics = (symbol_short!("revoked"), farmer)
    assert_eq!(topics.len(), 2);
    let topic: Symbol = Symbol::try_from_val(&w.env, &topics.get_unchecked(0)).unwrap();
    let who: Address = Address::try_from_val(&w.env, &topics.get_unchecked(1)).unwrap();
    assert_eq!(topic, symbol_short!("revoked"));
    assert_eq!(who, w.farmer);

    // data = contract_id
    let revoked: BytesN<32> = BytesN::try_from_val(&w.env, &data).unwrap();
    assert_eq!(revoked, cid);
}

#[test]
fn revoke_is_terminal() {
    let w = World::new();
    let cid = bytes(&w.env, 0x0C);
    w.grant(&cid, 1_000u32);

    w.env.mock_all_auths();
    w.client.revoke_data_access(&w.farmer, &cid);

    w.env.mock_all_auths();
    assert_denied(
        w.client.try_revoke_data_access(&w.farmer, &cid),
        ContractError::AccessRevoked,
    );
}

#[test]
fn revoke_rejects_parties_other_than_the_farmer() {
    let w = World::new();
    let cid = bytes(&w.env, 0x0D);
    w.grant(&cid, 1_000u32);

    // Neither the corporation nor an unrelated party may revoke on the farmer's
    // behalf; sovereignty runs one way only.
    w.env.mock_all_auths();
    let corporate_admin = Address::generate(&w.env);
    assert_denied(
        w.client.try_revoke_data_access(&corporate_admin, &cid),
        ContractError::Unauthorized,
    );

    w.env.mock_all_auths();
    assert_denied(
        w.client.try_revoke_data_access(&w.consumer, &cid),
        ContractError::Unauthorized,
    );

    assert!(!w.client.get_data_contract(&cid).unwrap().is_revoked);
}

#[test]
fn revoke_requires_farmer_signature() {
    let w = World::new();
    let cid = bytes(&w.env, 0x0E);
    w.grant(&cid, 1_000u32);

    assert_unsigned(w.client.try_revoke_data_access(&w.farmer, &cid));
    assert!(!w.client.get_data_contract(&cid).unwrap().is_revoked);
}

#[test]
fn revoke_rejects_unknown_agreement() {
    let w = World::new();
    w.env.mock_all_auths();

    assert_denied(
        w.client.try_revoke_data_access(&w.farmer, &bytes(&w.env, 0xFF)),
        ContractError::AssetNotFound,
    );
}

// ---------------------------------------------------------------------------
// ODRL enforcement at access time
// ---------------------------------------------------------------------------

#[test]
fn verify_returns_provenance_commitment_to_the_licensed_consumer() {
    let w = World::new();
    let cid = bytes(&w.env, 0x10);
    w.grant(&cid, 1_000u32);

    w.env.mock_all_auths();
    let root = w.client.verify_data_access(&w.consumer, &cid);

    assert_eq!(root, w.merkle_root);
    assert_eq!(
        root,
        w.client
            .get_asset_twin(&w.asset_id)
            .unwrap()
            .provenance_merkle_root
    );
}

#[test]
fn verify_rejects_party_outside_the_agreement() {
    let w = World::new();
    let cid = bytes(&w.env, 0x11);
    w.grant(&cid, 1_000u32);

    w.env.mock_all_auths();
    let rival_corporation = Address::generate(&w.env);
    assert_denied(
        w.client.try_verify_data_access(&rival_corporation, &cid),
        ContractError::Unauthorized,
    );
}

#[test]
fn verify_requires_consumer_signature() {
    let w = World::new();
    let cid = bytes(&w.env, 0x12);
    w.grant(&cid, 1_000u32);

    assert_unsigned(w.client.try_verify_data_access(&w.consumer, &cid));
}

#[test]
fn verify_rejects_access_once_the_validity_window_elapses() {
    let w = World::new();
    let cid = bytes(&w.env, 0x13);
    let grant = w.grant_record(&cid, 100u32);

    // The final valid ledger still grants access.
    w.env.ledger().set_sequence_number(grant.valid_until_ledger);
    w.env.mock_all_auths();
    assert_eq!(w.client.verify_data_access(&w.consumer, &cid), w.merkle_root);

    // One ledger past the window, the agreement has lapsed.
    w.env.ledger().set_sequence_number(grant.valid_until_ledger + 1);
    w.env.mock_all_auths();
    assert_denied(
        w.client.try_verify_data_access(&w.consumer, &cid),
        ContractError::ContractExpired,
    );

    // Lapsing is not revocation: the agreement record is unchanged.
    let after = w.client.get_data_contract(&cid).unwrap();
    assert!(!after.is_revoked);
    assert_eq!(after.valid_until_ledger, grant.valid_until_ledger);
}

#[test]
fn verify_rejects_unknown_agreement() {
    let w = World::new();
    w.env.mock_all_auths();

    assert_denied(
        w.client.try_verify_data_access(&w.consumer, &bytes(&w.env, 0xFD)),
        ContractError::AssetNotFound,
    );
}

#[test]
fn revocation_dominates_an_otherwise_live_window() {
    // Revocation must be reported even while the validity window is still open.
    let w = World::new();
    let cid = bytes(&w.env, 0x14);
    let grant = w.grant_record(&cid, 100u32);

    w.env.mock_all_auths();
    w.client.revoke_data_access(&w.farmer, &cid);

    w.env.ledger().set_sequence_number(grant.valid_until_ledger);
    w.env.mock_all_auths();
    assert_denied(
        w.client.try_verify_data_access(&w.consumer, &cid),
        ContractError::AccessRevoked,
    );
}

#[test]
fn zero_duration_agreement_is_never_enforceable() {
    // A zero-length window is accepted but immediately ineligible, so the
    // consumer can never obtain the provenance commitment.
    let w = World::new();
    let cid = bytes(&w.env, 0x15);
    w.grant(&cid, 0u32);

    // The window closed on the ledger that created the agreement.
    w.env.ledger().set_sequence_number(1);
    w.env.mock_all_auths();
    assert_denied(
        w.client.try_verify_data_access(&w.consumer, &cid),
        ContractError::ContractExpired,
    );
}
