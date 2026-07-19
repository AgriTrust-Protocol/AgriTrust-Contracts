#![no_std]

mod errors;
mod resolver;
mod supply_chain;
mod types;
mod verifier;

pub use errors::Error;
pub use resolver::{get_provenance_result, resolve_provenance, write_hop_state};
pub use types::{
    HopState, ProvenanceAccessSet, ProvenanceResult, Score, StorageBudget, MAX_HOPS,
    SCORE_PRECISION, STORAGE_BUDGET, STORAGE_WARN_THRESHOLD,
};

// Re-export verifier functions for external testing / integration.
pub use supply_chain::{Certificate, CustodyEvent, MerkleProof, ProvenanceToken};
pub use verifier::{validate_hop_credential, verify_hop_signature};

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, Vec};

#[contract]
pub struct ProvenanceContract;

#[contractimpl]
impl ProvenanceContract {
    /// Resolve a provenance chain and return the aggregated result.
    ///
    /// `chain_id`  — unique identifier for this chain resolution (used as
    ///               the storage key for the written ProvenanceResult).
    /// `hop_ids`   — ordered list of hop identifiers; each must have a
    ///               corresponding HopState in persistent storage (written
    ///               by `write_hop` before calling this).
    pub fn resolve(
        env: Env,
        chain_id: BytesN<32>,
        hop_ids: Vec<BytesN<32>>,
    ) -> Result<ProvenanceResult, Error> {
        resolve_provenance(&env, chain_id, hop_ids)
    }

    /// Write a HopState into persistent storage. Called by grant_contracts,
    /// compliance, admin, and treasury hops before resolution.
    pub fn write_hop(env: Env, hop_id: BytesN<32>, state: HopState) {
        write_hop_state(&env, &hop_id, &state);
    }

    /// Retrieve a previously resolved ProvenanceResult by chain_id.
    pub fn get_result(env: Env, chain_id: BytesN<32>) -> Option<ProvenanceResult> {
        get_provenance_result(&env, &chain_id)
    }

    pub fn initialize(env: Env, admin: Address) {
        supply_chain::initialize(&env, admin)
    }

    pub fn set_authorized_custodian(
        env: Env,
        custodian: Address,
        authorized: bool,
    ) -> Result<(), Error> {
        supply_chain::set_authorized_custodian(&env, custodian, authorized)
    }

    pub fn register_certificate_type(
        env: Env,
        cert_type: BytesN<32>,
        active: bool,
    ) -> Result<(), Error> {
        supply_chain::register_certificate_type(&env, cert_type, active)
    }

    pub fn set_compliance_standard(
        env: Env,
        standard_id: BytesN<32>,
        required_types: Vec<BytesN<32>>,
    ) -> Result<(), Error> {
        supply_chain::set_compliance_standard(&env, standard_id, required_types)
    }

    pub fn mint_batch(
        env: Env,
        owner: Address,
        batch_id: BytesN<32>,
        commodity_type: BytesN<32>,
        quantity: i128,
        origin_farm: Address,
        harvest_date: u64,
        metadata_cid_root: BytesN<32>,
        expires_at: u64,
    ) -> Result<u64, Error> {
        supply_chain::mint_batch(
            &env,
            owner,
            batch_id,
            commodity_type,
            quantity,
            origin_farm,
            harvest_date,
            metadata_cid_root,
            expires_at,
        )
    }

    pub fn owner_of(env: Env, token_id: u64) -> Result<Address, Error> {
        supply_chain::owner_of(&env, token_id)
    }

    pub fn transfer(env: Env, from: Address, to: Address, token_id: u64) -> Result<(), Error> {
        supply_chain::transfer(&env, from, to, token_id)
    }

    pub fn add_custody_event(
        env: Env,
        token_id: u64,
        custodian: Address,
        event_type: BytesN<32>,
        data_root: BytesN<32>,
    ) -> Result<u64, Error> {
        supply_chain::add_custody_event(&env, token_id, custodian, event_type, data_root)
    }

    pub fn transfer_event(
        env: Env,
        from: Address,
        to: Address,
        event_id: u64,
    ) -> Result<(), Error> {
        supply_chain::transfer_event(&env, from, to, event_id)
    }

    pub fn attach_certificate(
        env: Env,
        token_id: u64,
        cert_type: BytesN<32>,
        issuer: Address,
        data_root: BytesN<32>,
        expires_at: u64,
    ) -> Result<u64, Error> {
        supply_chain::attach_certificate(&env, token_id, cert_type, issuer, data_root, expires_at)
    }

    pub fn active_certificates(env: Env, token_id: u64) -> Result<Vec<Certificate>, Error> {
        supply_chain::active_certificates(&env, token_id)
    }

    pub fn verify_compliance(
        env: Env,
        token_id: u64,
        standard_id: BytesN<32>,
    ) -> Result<bool, Error> {
        supply_chain::verify_compliance(&env, token_id, standard_id)
    }

    pub fn split(
        env: Env,
        owner: Address,
        token_id: u64,
        quantities: Vec<i128>,
    ) -> Result<Vec<u64>, Error> {
        supply_chain::split(&env, owner, token_id, quantities)
    }

    pub fn merge(
        env: Env,
        owner: Address,
        token_ids: Vec<u64>,
        aggregate_batch_id: BytesN<32>,
        metadata_cid_root: BytesN<32>,
    ) -> Result<u64, Error> {
        supply_chain::merge(
            &env,
            owner,
            token_ids,
            aggregate_batch_id,
            metadata_cid_root,
        )
    }

    pub fn token(env: Env, token_id: u64) -> Result<ProvenanceToken, Error> {
        supply_chain::token(&env, token_id)
    }

    pub fn event(env: Env, event_id: u64) -> Result<CustodyEvent, Error> {
        supply_chain::event(&env, event_id)
    }

    pub fn verify_metadata(env: Env, root: BytesN<32>, proof: MerkleProof) -> bool {
        supply_chain::verify_metadata(&env, root, proof)
    }

    pub fn set_metadata_root(
        env: Env,
        owner: Address,
        token_id: u64,
        metadata_cid_root: BytesN<32>,
    ) -> Result<(), Error> {
        supply_chain::set_metadata_root(&env, owner, token_id, metadata_cid_root)
    }
}

#[cfg(test)]
mod test;
