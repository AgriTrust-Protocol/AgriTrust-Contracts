//! TTL-Coordinated Batch Certification Metadata Storage
//!
//! Stores batch certification metadata (certificates, inspection reports, lab results)
//! with TTL coordination guards and staggered key hashing to eliminate TTL shortening
//! and storage page collisions under concurrent certification submissions.
//!
//! Implements resolution blueprint for AgriTrust Issue #158:
//! 1. TTL coordination guard preventing concurrent extension shortening.
//! 2. Staggered key hashing using SHA256(batch_id || certifier_id || entry_type)
//!    to distribute entries across 4KB Soroban storage page boundaries.
//! 3. Background TTL reconciliation function `reconcile_metadata_ttl(batch_id)`.
//! 4. Aggregated metadata map storage per batch (`metadata:{batch_id} -> Map<BytesN<32>, CertificationMetadata>`).
//! 5. Comprehensive concurrency test ensuring all entries maintain TTL >= 90 days.

use soroban_sdk::{
    contracttype, symbol_short, xdr::ToXdr, Address, Bytes, BytesN, Env, IntoVal, Map, String,
    Val,
};

use crate::errors::Error;

// ── Constants ──────────────────────────────────────────────────────────────────

/// Certification persistence duration: 90 days in ledgers (at ~5s per ledger close).
pub const TTL_EXTENSION_AMOUNT: u32 = 1_555_200;

/// Ledger threshold for TTL extension: 30 days in ledgers.
pub const LEDGER_THRESHOLD: u32 = 518_400;

/// Storage page boundary size (4KB for Soroban ledger entry grouping).
pub const STORAGE_PAGE_SIZE: u32 = 4096;

/// Maximum metadata entries allowed per batch to protect storage and compute limits.
pub const MAX_METADATA_ENTRIES: u32 = 100;

// ── Types ──────────────────────────────────────────────────────────────────────

/// Categorization of batch certification metadata entries.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MetadataType {
    Certificate = 1,
    InspectionReport = 2,
    LabResult = 3,
    AuditAttestation = 4,
    Custom = 5,
}

/// Certification metadata record for a batch.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CertificationMetadata {
    /// Identifier of the batch being certified.
    pub batch_id: BytesN<32>,
    /// Address of the certifier entity or contract.
    pub certifier: Address,
    /// Type of certification entry.
    pub entry_type: MetadataType,
    /// Cryptographic hash of the off-chain or on-chain certification payload.
    pub data_hash: BytesN<32>,
    /// URI or content identifier (e.g. IPFS/HTTPS) for full document verification.
    pub uri: String,
    /// Unix timestamp when metadata was recorded.
    pub recorded_at: u64,
    /// Unix timestamp when certification expires (0 = indefinite).
    pub valid_until: u64,
    /// Ledger sequence until which this entry is guaranteed to live.
    pub ttl_extended_until_ledger: u32,
}

/// Aggregated batch metadata storage record holding all entries for a batch.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchMetadataRecord {
    /// Batch identifier.
    pub batch_id: BytesN<32>,
    /// Current count of stored metadata entries.
    pub count: u32,
    /// Aggregated entries map indexed by staggered entry hash key.
    pub entries: Map<BytesN<32>, CertificationMetadata>,
    /// Ledger sequence when TTL was last reconciled.
    pub last_reconciled_ledger: u32,
}

// ── Key Derivation Helpers ────────────────────────────────────────────────────

/// Derive a deterministic staggered storage key: SHA256(batch_id || certifier || entry_type).
///
/// This avoids 4KB storage page boundary collisions in Stellar's ledger grouping by
/// distributing different certifier entries across disparate cryptographic hash buckets.
pub fn compute_staggered_metadata_key(
    env: &Env,
    batch_id: &BytesN<32>,
    certifier: &Address,
    entry_type: MetadataType,
) -> BytesN<32> {
    let mut payload = Bytes::new(env);
    payload.push_back(b'M');
    payload.push_back(b'D');
    payload.append(&Bytes::from_array(env, &batch_id.to_array()));
    payload.append(&certifier.clone().to_xdr(env));
    payload.push_back(entry_type as u8);
    env.crypto().sha256(&payload).into()
}

/// Compute persistent storage key for the aggregated batch metadata record.
/// Key = b"BM" ++ batch_id bytes.
pub fn batch_metadata_key(env: &Env, batch_id: &BytesN<32>) -> Bytes {
    let mut key = Bytes::new(env);
    key.push_back(b'B');
    key.push_back(b'M');
    key.append(&Bytes::from_array(env, &batch_id.to_array()));
    key
}

/// Compute persistent storage key for an individual staggered metadata entry.
/// Key = b"MD" ++ staggered_hash bytes.
pub fn staggered_storage_key(env: &Env, staggered_key: &BytesN<32>) -> Bytes {
    let mut key = Bytes::new(env);
    key.push_back(b'M');
    key.push_back(b'D');
    key.append(&Bytes::from_array(env, &staggered_key.to_array()));
    key
}

// ── TTL Coordination Guard ────────────────────────────────────────────────────

/// Read the current remaining TTL of a storage key.
///
/// Uses host-level testutils inspection when available, or relies on tracked expiry
/// in guest execution where direct host TTL queries are restricted.
pub fn storage_get_ttl<K>(env: &Env, key: &K, _tracked_expiry_ledger: u32) -> u32
where
    K: IntoVal<Env, Val> + Clone,
{
    if !env.storage().persistent().has(key) {
        return 0;
    }
    #[cfg(any(test, feature = "testutils"))]
    {
        use soroban_sdk::testutils::storage::Persistent;
        env.storage().persistent().get_ttl(key)
    }
    #[cfg(not(any(test, feature = "testutils")))]
    {
        let seq = env.ledger().sequence();
        if _tracked_expiry_ledger > seq {
            _tracked_expiry_ledger - seq
        } else {
            0
        }
    }
}

/// TTL Coordination Guard: extends TTL only if the current remaining TTL is less than `extend_to`.
///
/// This invariant prevents concurrent operations with lower or different threshold parameters
/// from resetting or shortening an entry's 90-day persistence window.
pub fn extend_ttl_coordinated<K>(
    env: &Env,
    key: &K,
    threshold: u32,
    extend_to: u32,
    tracked_expiry_ledger: u32,
) -> bool
where
    K: IntoVal<Env, Val> + Clone,
{
    let current_ttl = storage_get_ttl(env, key, tracked_expiry_ledger);
    if current_ttl < extend_to {
        env.storage().persistent().extend_ttl(key, threshold, extend_to);
        true
    } else {
        false
    }
}

// ── Core Operations ───────────────────────────────────────────────────────────

/// Store certification metadata for a batch with TTL coordination and staggered page hashing.
///
/// Ensures:
/// 1. `certifier.require_auth()`
/// 2. `MAX_METADATA_ENTRIES` (100) limit enforcement.
/// 3. Duplicate entry detection.
/// 4. Atomic storage in both aggregated batch record and staggered hashed key.
/// 5. Coordinated TTL extension guaranteeing >= 90 days without shortening from concurrent calls.
pub fn store_batch_metadata(
    env: &Env,
    batch_id: BytesN<32>,
    certifier: Address,
    entry_type: MetadataType,
    data_hash: BytesN<32>,
    uri: String,
    valid_until: u64,
) -> Result<BytesN<32>, Error> {
    certifier.require_auth();

    let bkey = batch_metadata_key(env, &batch_id);
    let mut batch_record: BatchMetadataRecord = env
        .storage()
        .persistent()
        .get(&bkey)
        .unwrap_or_else(|| BatchMetadataRecord {
            batch_id: batch_id.clone(),
            count: 0,
            entries: Map::new(env),
            last_reconciled_ledger: env.ledger().sequence(),
        });

    if batch_record.entries.len() >= MAX_METADATA_ENTRIES {
        return Err(Error::BatchMetadataLimitExceeded);
    }

    let staggered_key = compute_staggered_metadata_key(env, &batch_id, &certifier, entry_type);
    let skey = staggered_storage_key(env, &staggered_key);

    if batch_record.entries.contains_key(staggered_key.clone()) {
        return Err(Error::DuplicateMetadataEntry);
    }

    let current_ledger = env.ledger().sequence();
    let target_expiry_ledger = current_ledger.saturating_add(TTL_EXTENSION_AMOUNT);

    let metadata = CertificationMetadata {
        batch_id: batch_id.clone(),
        certifier: certifier.clone(),
        entry_type,
        data_hash,
        uri,
        recorded_at: env.ledger().timestamp(),
        valid_until,
        ttl_extended_until_ledger: target_expiry_ledger,
    };

    // 1. Update aggregated batch map
    batch_record.entries.set(staggered_key.clone(), metadata.clone());
    batch_record.count = batch_record.entries.len();
    batch_record.last_reconciled_ledger = current_ledger;
    env.storage().persistent().set(&bkey, &batch_record);

    // 2. Write individual staggered entry for storage page dispersal
    env.storage().persistent().set(&skey, &metadata);

    // 3. TTL Coordination Guard on both aggregated entry and staggered entry
    extend_ttl_coordinated(
        env,
        &bkey,
        LEDGER_THRESHOLD,
        TTL_EXTENSION_AMOUNT,
        target_expiry_ledger,
    );
    extend_ttl_coordinated(
        env,
        &skey,
        LEDGER_THRESHOLD,
        TTL_EXTENSION_AMOUNT,
        target_expiry_ledger,
    );

    // Emit event for off-chain indexing and audit trail
    env.events().publish(
        (symbol_short!("prov"), symbol_short!("meta_add")),
        (batch_id, certifier, entry_type as u32),
    );

    Ok(staggered_key)
}

/// Retrieve certification metadata for a batch, certifier, and entry type.
pub fn get_metadata_entry(
    env: &Env,
    batch_id: &BytesN<32>,
    certifier: &Address,
    entry_type: MetadataType,
) -> Result<CertificationMetadata, Error> {
    let staggered_key = compute_staggered_metadata_key(env, batch_id, certifier, entry_type);
    let skey = staggered_storage_key(env, &staggered_key);

    // First check individual staggered storage entry
    if let Some(meta) = env.storage().persistent().get::<_, CertificationMetadata>(&skey) {
        if meta.valid_until > 0 && env.ledger().timestamp() > meta.valid_until {
            return Err(Error::MetadataExpired);
        }
        return Ok(meta);
    }

    // Fallback to aggregated batch map
    let bkey = batch_metadata_key(env, batch_id);
    if let Some(batch_record) = env.storage().persistent().get::<_, BatchMetadataRecord>(&bkey) {
        if let Some(meta) = batch_record.entries.get(staggered_key) {
            if meta.valid_until > 0 && env.ledger().timestamp() > meta.valid_until {
                return Err(Error::MetadataExpired);
            }
            return Ok(meta);
        }
    }

    Err(Error::MetadataNotFound)
}

/// Retrieve the aggregated batch metadata record.
pub fn get_batch_metadata_record(
    env: &Env,
    batch_id: &BytesN<32>,
) -> Option<BatchMetadataRecord> {
    let bkey = batch_metadata_key(env, batch_id);
    env.storage().persistent().get(&bkey)
}

/// Background TTL reconciliation function: scans all entries for a batch and ensures
/// all entries and the aggregated batch record have TTL >= TTL_EXTENSION_AMOUNT.
pub fn reconcile_batch_metadata_ttl(
    env: &Env,
    batch_id: &BytesN<32>,
) -> Result<u32, Error> {
    let bkey = batch_metadata_key(env, batch_id);
    let mut batch_record: BatchMetadataRecord = env
        .storage()
        .persistent()
        .get(&bkey)
        .ok_or(Error::MetadataNotFound)?;

    let current_ledger = env.ledger().sequence();
    let target_expiry_ledger = current_ledger.saturating_add(TTL_EXTENSION_AMOUNT);
    let mut reconciled_count: u32 = 0;

    // Extend aggregated batch key if needed
    if extend_ttl_coordinated(
        env,
        &bkey,
        LEDGER_THRESHOLD,
        TTL_EXTENSION_AMOUNT,
        batch_record.last_reconciled_ledger.saturating_add(TTL_EXTENSION_AMOUNT),
    ) {
        reconciled_count = reconciled_count.saturating_add(1);
    }

    // Iterate through all staggered metadata entries
    let keys = batch_record.entries.keys();
    for entry_key in keys.iter() {
        if let Some(mut meta) = batch_record.entries.get(entry_key.clone()) {
            let skey = staggered_storage_key(env, &entry_key);
            let extended = extend_ttl_coordinated(
                env,
                &skey,
                LEDGER_THRESHOLD,
                TTL_EXTENSION_AMOUNT,
                meta.ttl_extended_until_ledger,
            );
            if extended {
                meta.ttl_extended_until_ledger = target_expiry_ledger;
                batch_record.entries.set(entry_key, meta);
                reconciled_count = reconciled_count.saturating_add(1);
            }
        }
    }

    batch_record.last_reconciled_ledger = current_ledger;
    env.storage().persistent().set(&bkey, &batch_record);

    env.events().publish(
        (symbol_short!("prov"), symbol_short!("reconcile")),
        (batch_id.clone(), reconciled_count),
    );

    Ok(reconciled_count)
}
