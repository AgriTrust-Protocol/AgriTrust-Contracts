use soroban_sdk::{
    contracttype, symbol_short, Address, Bytes, BytesN, Env, Symbol, Vec,
};

use crate::errors::Error;

/// Maximum number of hops allowed across agricultural supply chain stages.
pub const MAX_CHAIN_HOPS: u32 = 32;

/// Timeout in ledgers before a held lock is considered abandoned and auto-released.
pub const LOCK_TIMEOUT_LEDGERS: u32 = 2;

/// Genesis hash for the initial stage (Farm) in a provenance chain.
pub const GENESIS_PREV_HASH: [u8; 32] = [0u8; 32];

/// Hop record linking a certification stage in the agricultural Merkle provenance chain.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Hop {
    /// Zero-based sequence number strictly matching position in chain.
    pub sequence: u32,
    /// Supply chain stage identifier (e.g., "farm", "process", "distrib", "retail").
    pub stage: Symbol,
    /// Verified public identity / address of stage certifier.
    pub signer: Address,
    /// Hash of the immediately preceding hop (or zeros for Genesis).
    pub prev_hash: BytesN<32>,
    /// Cryptographic digest of stage certification metadata & sensor payloads.
    pub metadata_hash: BytesN<32>,
    /// Cumulative Merkle node: SHA256(prev_hash || metadata_hash).
    pub hop_hash: BytesN<32>,
    /// Cryptographic signature payload.
    pub signature: BytesN<64>,
    /// Ledger sequence when hop was committed.
    pub ledger: u32,
    /// Timestamp when hop was committed.
    pub timestamp: u64,
}

/// Persistent state container for an entire batch's append-only provenance record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchProvenanceChain {
    pub batch_id: BytesN<32>,
    pub hops: Vec<Hop>,
    pub tip_hash: BytesN<32>,
    pub is_finalized: bool,
}

/// Mutual exclusion lock preventing concurrent multi-hop updates from corrupting the tip.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProvenanceLock {
    pub locked_by: Address,
    pub locked_at_ledger: u32,
}

// ── Storage Keys ─────────────────────────────────────────────────────────────

fn chain_key(_env: &Env, batch_id: &BytesN<32>) -> (Symbol, BytesN<32>) {
    (symbol_short!("chain"), batch_id.clone())
}

fn lock_key(_env: &Env, batch_id: &BytesN<32>) -> (Symbol, BytesN<32>) {
    (symbol_short!("lock"), batch_id.clone())
}

// ── Cryptographic Hashing ───────────────────────────────────────────────────

/// Compute cumulative Merkle hop hash: SHA256(prev_hash || metadata_hash).
pub fn compute_hop_hash(
    env: &Env,
    prev_hash: &BytesN<32>,
    metadata_hash: &BytesN<32>,
) -> BytesN<32> {
    let mut data = Bytes::new(env);
    data.append(&Bytes::from_array(env, &prev_hash.to_array()));
    data.append(&Bytes::from_array(env, &metadata_hash.to_array()));
    env.crypto().sha256(&data).into()
}

// ── Concurrency Guard: Lock Management ──────────────────────────────────────

/// Acquire mutual exclusion lock for a batch. Auto-recovers after LOCK_TIMEOUT_LEDGERS.
pub fn acquire_lock(env: &Env, batch_id: &BytesN<32>, signer: &Address) -> Result<(), Error> {
    let key = lock_key(env, batch_id);
    let current_ledger = env.ledger().sequence();

    if let Some(existing_lock) = env.storage().persistent().get::<_, ProvenanceLock>(&key) {
        let elapsed = current_ledger.saturating_sub(existing_lock.locked_at_ledger);
        if elapsed <= LOCK_TIMEOUT_LEDGERS && existing_lock.locked_by != *signer {
            return Err(Error::BatchLocked);
        }
    }

    let new_lock = ProvenanceLock {
        locked_by: signer.clone(),
        locked_at_ledger: current_ledger,
    };
    env.storage().persistent().set(&key, &new_lock);
    Ok(())
}

/// Release the mutual exclusion lock for a batch.
pub fn release_lock(env: &Env, batch_id: &BytesN<32>) {
    let key = lock_key(env, batch_id);
    env.storage().persistent().remove(&key);
}

// ── Append-Only Chain Operations ─────────────────────────────────────────────

/// Atomically submit and append a new provenance hop to the batch's append-only vector.
pub fn submit_hop(env: &Env, batch_id: &BytesN<32>, hop: &Hop) -> Result<BytesN<32>, Error> {
    // 1. Authorize submitter
    hop.signer.require_auth();

    // 2. Acquire concurrency lock
    acquire_lock(env, batch_id, &hop.signer)?;

    let ckey = chain_key(env, batch_id);
    let mut chain = env
        .storage()
        .persistent()
        .get::<_, BatchProvenanceChain>(&ckey)
        .unwrap_or(BatchProvenanceChain {
            batch_id: batch_id.clone(),
            hops: Vec::new(env),
            tip_hash: BytesN::from_array(env, &GENESIS_PREV_HASH),
            is_finalized: false,
        });

    // Guard against overflow beyond stage limit
    if chain.hops.len() >= MAX_CHAIN_HOPS {
        release_lock(env, batch_id);
        return Err(Error::MaxChainHopsExceeded);
    }

    // 3. Hop Sequence Number Guard: Strict contiguous sequence enforcement
    if hop.sequence != chain.hops.len() {
        release_lock(env, batch_id);
        return Err(Error::SequenceMismatch);
    }

    // 4. Validate Inter-Hop Cryptographic Linkage
    let expected_prev_hash = if chain.hops.is_empty() {
        BytesN::from_array(env, &GENESIS_PREV_HASH)
    } else {
        chain.hops.last().unwrap().hop_hash
    };

    if hop.prev_hash != expected_prev_hash {
        release_lock(env, batch_id);
        return Err(Error::ChainBroken);
    }

    // 5. Verify Hop Merkle Node Hash: SHA256(prev_hash || metadata_hash)
    let calculated_hash = compute_hop_hash(env, &hop.prev_hash, &hop.metadata_hash);
    if hop.hop_hash != calculated_hash {
        release_lock(env, batch_id);
        return Err(Error::ChainBroken);
    }

    // 6. Append to vector and update tip
    chain.hops.push_back(hop.clone());
    chain.tip_hash = hop.hop_hash.clone();

    // 7. Commit state atomically
    env.storage().persistent().set(&ckey, &chain);

    // 8. Release lock
    release_lock(env, batch_id);

    // 9. Publish event
    env.events().publish(
        (symbol_short!("prov"), symbol_short!("hop_app")),
        (batch_id.clone(), hop.sequence, hop.hop_hash.clone()),
    );

    Ok(chain.tip_hash)
}

/// Retrieve the full batch provenance chain record.
pub fn get_chain(env: &Env, batch_id: &BytesN<32>) -> Option<BatchProvenanceChain> {
    let ckey = chain_key(env, batch_id);
    env.storage().persistent().get(&ckey)
}

/// Verify contiguous provenance chain integrity from genesis to tip.
pub fn verify_chain(env: &Env, batch_id: &BytesN<32>) -> Result<bool, Error> {
    let chain = match get_chain(env, batch_id) {
        Some(c) => c,
        None => return Err(Error::EmptyChain),
    };

    if chain.hops.is_empty() {
        return Err(Error::EmptyChain);
    }

    let mut current_expected_prev = BytesN::from_array(env, &GENESIS_PREV_HASH);

    for (idx, hop) in chain.hops.iter().enumerate() {
        if hop.sequence != idx as u32 {
            return Err(Error::SequenceMismatch);
        }

        if hop.prev_hash != current_expected_prev {
            return Err(Error::ChainBroken);
        }

        let computed = compute_hop_hash(env, &hop.prev_hash, &hop.metadata_hash);
        if hop.hop_hash != computed {
            return Err(Error::ChainBroken);
        }

        current_expected_prev = hop.hop_hash.clone();
    }

    // Verify tip matches last hop
    if chain.tip_hash != current_expected_prev {
        return Err(Error::ChainBroken);
    }

    Ok(true)
}
