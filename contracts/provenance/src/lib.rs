#![no_std]

pub mod chain;
mod errors;
mod resolver;
mod types;
mod verifier;

pub use chain::{
    compute_hop_hash, get_chain, submit_hop, verify_chain, BatchProvenanceChain, Hop,
    ProvenanceLock, GENESIS_PREV_HASH, LOCK_TIMEOUT_LEDGERS, MAX_CHAIN_HOPS,
};
pub use errors::Error;
pub use resolver::{get_provenance_result, resolve_provenance, write_hop_state};
pub use types::{
    HopState, ProvenanceAccessSet, ProvenanceResult, Score, StorageBudget,
    MAX_HOPS, SCORE_PRECISION, STORAGE_BUDGET, STORAGE_WARN_THRESHOLD,
};

// Re-export verifier functions for external testing / integration.
pub use verifier::{validate_hop_credential, verify_hop_signature};

use soroban_sdk::{contract, contractimpl, BytesN, Env, Vec};

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

    /// Atomically submit and append a provenance hop to a batch chain (Issue #160).
    pub fn submit_hop(env: Env, batch_id: BytesN<32>, hop: Hop) -> Result<BytesN<32>, Error> {
        chain::submit_hop(&env, &batch_id, &hop)
    }

    /// Retrieve the append-only multi-hop provenance chain for a batch (Issue #160).
    pub fn get_chain(env: Env, batch_id: BytesN<32>) -> Option<BatchProvenanceChain> {
        chain::get_chain(&env, &batch_id)
    }

    /// Verify cryptographic integrity and unbroken sequence of a batch chain (Issue #160).
    pub fn verify_chain(env: Env, batch_id: BytesN<32>) -> Result<bool, Error> {
        chain::verify_chain(&env, &batch_id)
    }
}

#[cfg(test)]
mod test;
