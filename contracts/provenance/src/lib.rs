#![no_std]

mod errors;
mod metadata;
mod resolver;
mod types;
mod verifier;

pub use errors::Error;
pub use metadata::{
    batch_metadata_key, compute_staggered_metadata_key, extend_ttl_coordinated,
    get_batch_metadata_record, get_metadata_entry, reconcile_batch_metadata_ttl,
    staggered_storage_key, storage_get_ttl, store_batch_metadata, BatchMetadataRecord,
    CertificationMetadata, MetadataType, LEDGER_THRESHOLD, MAX_METADATA_ENTRIES,
    STORAGE_PAGE_SIZE, TTL_EXTENSION_AMOUNT,
};
pub use resolver::{get_provenance_result, resolve_provenance, write_hop_state};
pub use types::{
    HopState, ProvenanceAccessSet, ProvenanceResult, Score, StorageBudget,
    SCORE_PRECISION, STORAGE_BUDGET, STORAGE_WARN_THRESHOLD, MAX_HOPS,
};

// Re-export verifier functions for external testing / integration.
pub use verifier::{validate_hop_credential, verify_hop_signature};

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String, Vec};

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

    /// Store batch certification metadata with TTL coordination guard.
    /// Returns the deterministic 32-byte staggered storage key.
    pub fn store_metadata(
        env: Env,
        batch_id: BytesN<32>,
        certifier: Address,
        entry_type: MetadataType,
        data_hash: BytesN<32>,
        uri: String,
        valid_until: u64,
    ) -> Result<BytesN<32>, Error> {
        store_batch_metadata(
            &env,
            batch_id,
            certifier,
            entry_type,
            data_hash,
            uri,
            valid_until,
        )
    }

    /// Retrieve certification metadata for a specific batch, certifier, and entry type.
    pub fn get_metadata(
        env: Env,
        batch_id: BytesN<32>,
        certifier: Address,
        entry_type: MetadataType,
    ) -> Result<CertificationMetadata, Error> {
        get_metadata_entry(&env, &batch_id, &certifier, entry_type)
    }

    /// Retrieve the aggregated batch metadata record containing all entries.
    pub fn get_batch_metadata(
        env: Env,
        batch_id: BytesN<32>,
    ) -> Result<BatchMetadataRecord, Error> {
        get_batch_metadata_record(&env, &batch_id).ok_or(Error::MetadataNotFound)
    }

    /// Background reconciliation function: scans all metadata entries for the batch
    /// and extends any entry whose TTL is below TTL_EXTENSION_AMOUNT.
    pub fn reconcile_metadata_ttl(
        env: Env,
        batch_id: BytesN<32>,
    ) -> Result<u32, Error> {
        reconcile_batch_metadata_ttl(&env, &batch_id)
    }

    /// Compute the deterministic staggered storage key: SHA256(batch_id || certifier || entry_type).
    pub fn compute_metadata_key(
        env: Env,
        batch_id: BytesN<32>,
        certifier: Address,
        entry_type: MetadataType,
    ) -> BytesN<32> {
        compute_staggered_metadata_key(&env, &batch_id, &certifier, entry_type)
    }
}

#[cfg(test)]
mod test;
