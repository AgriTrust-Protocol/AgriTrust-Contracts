//! Optimistic Concurrency Control with Two-Phase Commit
//!
//! Implements:
//! - Per-batch sequence counter for linearization (FIFO ordering)
//! - Two-phase commit: Phase 1 (ready_to_commit), Phase 2 (finalize)
//! - Compensating transactions for conflict resolution
//! - Automatic expiration after OPTIMISTIC_LOCK_TIMEOUT

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short,
    Bytes, Env, Map, Symbol, Vec,
};

// ─── Constants ────────────────────────────────────────────────────────────

pub const OPTIMISTIC_LOCK_TIMEOUT: u32 = 10; // ~10 ledger closes (~50s)
pub const MAX_CONCURRENT_MUTATIONS: u32 = 5;

// ─── Storage Keys ─────────────────────────────────────────────────────────

#[contracttype]
pub enum StateKey {
    Version,
    PendingMap(Bytes),           // batch_id -> Vec<PendingMutation>
    CompensationMap(Bytes),    // mutation_id -> Vec<CompensationEntry>
    BatchCounter(Bytes),         // batch_id -> next_seq_no
    ReadyToCommit(Bytes),        // mutation_id -> bool
    StateValue(Bytes),           // data_key -> value
}

// ─── Data Types ────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Copy)]
pub struct StateVersion {
    pub current_version: u64,
    pub current_seq_no: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct PendingMutation {
    pub batch_id: Bytes,
    pub mutation_id: Bytes,
    pub state_updates: Map<Bytes, Bytes>,  // key -> new value
    pub prev_values: Map<Bytes, Bytes>,   // key -> old value (for compensation)
    pub version: u64,
    pub seq_no: u64,
    pub expires_at: u32,
    pub status: MutationStatus,
}

#[contracttype]
#[derive(Clone, PartialEq, Debug)]
pub enum MutationStatus {
    Pending,
    ReadyToCommit,
    Committed,
    RolledBack,
    Expired,
}

#[contracttype]
#[derive(Clone)]
pub struct CompensationEntry {
    pub original_mutation_id: Bytes,
    pub compensation_state: Map<Bytes, Bytes>,  // reverted values
    pub reason: Symbol,
    pub timestamp: u32,
}

// ─── Core Module ──────────────────────────────────────────────────────────

#[contract]
pub struct OptimisticContract;

#[contractimpl]
impl OptimisticContract {
    /// Initialize the state contract
    pub fn initialize(env: Env) {
        let version = StateVersion {
            current_version: 0,
            current_seq_no: 0,
        };
        env.storage().persistent().set(&StateKey::Version, &version);
    }

    /// Begin an optimistic mutation. Returns the assigned seq_no.
    pub fn begin_optimistic(
        env: Env,
        batch_id: Bytes,
        state_updates: Map<Bytes, Bytes>,
    ) -> u64 {
        // Get or initialize batch counter
        let mut seq_no: u64 = env
            .storage()
            .persistent()
            .get(&StateKey::BatchCounter(batch_id.clone()))
            .unwrap_or(0);

        // Increment seq_no for this batch
        seq_no += 1;
        env.storage()
            .persistent()
            .set(&StateKey::BatchCounter(batch_id.clone()), &seq_no);

        // Get current version
        let version: StateVersion = env
            .storage()
            .persistent()
            .get(&StateKey::Version)
            .unwrap_or(StateVersion { current_version: 0, current_seq_no: 0 });

        // Capture previous values for compensation
        let mut prev_values: Map<Bytes, Bytes> = Map::new(&env);
        for key in state_updates.keys() {
            let prev: Bytes = env
                .storage()
                .persistent()
                .get(&StateKey::StateValue(key.clone()))
                .unwrap_or(Bytes::from_slice(&env, &[]));
            prev_values.set(key, prev);
        }

        // Generate mutation_id from batch_id + seq_no
        let mutation_id = Self::make_mutation_id(&env, &batch_id, seq_no);

        // Calculate expiration
        let expires_at = env.ledger().sequence() + OPTIMISTIC_LOCK_TIMEOUT;

        // Create pending mutation
        let pending = PendingMutation {
            batch_id: batch_id.clone(),
            mutation_id: mutation_id.clone(),
            state_updates: state_updates.clone(),
            prev_values,
            version: version.current_version,
            seq_no,
            expires_at,
            status: MutationStatus::Pending,
        };

        // Store in pending map
        let mut pending_list: Vec<PendingMutation> = env
            .storage()
            .persistent()
            .get(&StateKey::PendingMap(batch_id.clone()))
            .unwrap_or(Vec::new(&env));
        pending_list.push_back(pending);
        env.storage()
            .persistent()
            .set(&StateKey::PendingMap(batch_id.clone()), &pending_list);

        // Emit event
        env.events().publish(
            (symbol_short!("begin_opt"), batch_id, mutation_id),
            seq_no,
        );

        seq_no
    }

    /// Commit an optimistic mutation using 2PC.
    /// Returns true if committed, false if compensated/expired.
    pub fn commit_optimistic(env: Env, batch_id: Bytes, mutation_id: Bytes) -> bool {
        // Get current version
        let mut version: StateVersion = env
            .storage()
            .persistent()
            .get(&StateKey::Version)
            .unwrap();

        // Find the pending mutation
        let mut pending_list: Vec<PendingMutation> = env
            .storage()
            .persistent()
            .get(&StateKey::PendingMap(batch_id.clone()))
            .unwrap_or(Vec::new(&env));

        let mut found_idx: Option<u32> = None;
        for (i, pending) in pending_list.iter().enumerate() {
            if pending.mutation_id == mutation_id {
                found_idx = Some(i as u32);
                break;
            }
        }

        let idx = match found_idx {
            Some(i) => i,
            None => panic!("mutation not found"),
        };

        let mut mutation: PendingMutation = pending_list.get(idx).unwrap();

        // Check expiration
        if env.ledger().sequence() > mutation.expires_at {
            mutation.status = MutationStatus::Expired;
            pending_list.set(idx, mutation);
            env.storage()
                .persistent()
                .set(&StateKey::PendingMap(batch_id.clone()), &pending_list);
            env.events().publish(
                (symbol_short!("expired"), batch_id, mutation_id),
                version.current_seq_no,
            );
            return false;
        }

        // ── Phase 1: Linearization check ──
        if mutation.seq_no != version.current_seq_no + 1 {
            // Not our turn — apply compensation
            let compensation = CompensationEntry {
                original_mutation_id: mutation_id.clone(),
                compensation_state: mutation.prev_values.clone(),
                reason: symbol_short!("ooo_comp"),
                timestamp: env.ledger().sequence(),
            };

            let mut comp_list: Vec<CompensationEntry> = env
                .storage()
                .persistent()
                .get(&StateKey::CompensationMap(mutation_id.clone()))
                .unwrap_or(Vec::new(&env));
            comp_list.push_back(compensation);
            env.storage()
                .persistent()
                .set(&StateKey::CompensationMap(mutation_id.clone()), &comp_list);

            mutation.status = MutationStatus::RolledBack;
            pending_list.set(idx, mutation);
            env.storage()
                .persistent()
                .set(&StateKey::PendingMap(batch_id.clone()), &pending_list);

            env.events().publish(
                (symbol_short!("compense"), batch_id, mutation_id),
                version.current_seq_no,
            );
            return false;
        }

        // Mark ready (Phase 1 complete)
        env.storage()
            .persistent()
            .set(&StateKey::ReadyToCommit(mutation_id.clone()), &true);
        mutation.status = MutationStatus::ReadyToCommit;
        pending_list.set(idx, mutation.clone());
        env.storage()
            .persistent()
            .set(&StateKey::PendingMap(batch_id.clone()), &pending_list);

        // ── Phase 2: Apply state changes ──
        for key in mutation.state_updates.keys() {
            let k = key.clone();
            let new_value: Bytes = mutation.state_updates.get(k.clone()).unwrap();
            env.storage()
                .persistent()
                .set(&StateKey::StateValue(k), &new_value);
        }

        // Increment version and seq_no
        version.current_version += 1;
        version.current_seq_no = mutation.seq_no;
        env.storage()
            .persistent()
            .set(&StateKey::Version, &version);

        // Mark committed
        let committed_list: Vec<PendingMutation> = env
            .storage()
            .persistent()
            .get(&StateKey::PendingMap(batch_id.clone()))
            .unwrap_or(Vec::new(&env));
        if let Some(i) = (0..committed_list.len()).find(|i| committed_list.get(*i as u32).unwrap().mutation_id == mutation_id) {
            let mut committed_mut = committed_list.get(i as u32).unwrap();
            committed_mut.status = MutationStatus::Committed;
            let mut new_list = committed_list.clone();
            new_list.set(i as u32, committed_mut);
            env.storage()
                .persistent()
                .set(&StateKey::PendingMap(batch_id.clone()), &new_list);
        }

        // Emit commit event
        env.events().publish(
            (symbol_short!("committed"), batch_id, mutation_id),
            (version.current_version, version.current_seq_no),
        );

        true
    }

    /// Rollback a pending mutation with compensation logging
    pub fn rollback_mutation(env: Env, batch_id: Bytes, mutation_id: Bytes) {
        let mut pending_list: Vec<PendingMutation> = env
            .storage()
            .persistent()
            .get(&StateKey::PendingMap(batch_id.clone()))
            .unwrap_or(Vec::new(&env));

        let mut found_idx: Option<u32> = None;
        for (i, pending) in pending_list.iter().enumerate() {
            if pending.mutation_id == mutation_id {
                found_idx = Some(i as u32);
                break;
            }
        }

        let idx = match found_idx {
            Some(i) => i,
            None => return,
        };

        let mut mutation: PendingMutation = pending_list.get(idx).unwrap();

        // Restore previous values (compensating action)
        for key in mutation.prev_values.keys() {
            let k = key.clone();
            let prev_value: Bytes = mutation.prev_values.get(k.clone()).unwrap();
            if prev_value.is_empty() {
                env.storage().persistent().remove(&StateKey::StateValue(k));
            } else {
                env.storage()
                    .persistent()
                    .set(&StateKey::StateValue(k), &prev_value);
            }
        }

        // Log compensation
        let compensation = CompensationEntry {
            original_mutation_id: mutation_id.clone(),
            compensation_state: mutation.prev_values.clone(),
            reason: symbol_short!("explicit"),
            timestamp: env.ledger().sequence(),
        };

        let mut comp_list: Vec<CompensationEntry> = env
            .storage()
            .persistent()
            .get(&StateKey::CompensationMap(mutation_id.clone()))
            .unwrap_or(Vec::new(&env));
        comp_list.push_back(compensation);
        env.storage()
            .persistent()
            .set(&StateKey::CompensationMap(mutation_id.clone()), &comp_list);

        // Mark rolled back
        mutation.status = MutationStatus::RolledBack;
        pending_list.set(idx, mutation);
        env.storage()
            .persistent()
            .set(&StateKey::PendingMap(batch_id.clone()), &pending_list);

        env.events().publish(
            (symbol_short!("rolledbk"), batch_id, mutation_id),
            env.ledger().sequence(),
        );
    }

    /// Expire a pending mutation after timeout. Anyone can call.
    pub fn expire_pending(env: Env, batch_id: Bytes, mutation_id: Bytes) -> bool {
        let mut pending_list: Vec<PendingMutation> = env
            .storage()
            .persistent()
            .get(&StateKey::PendingMap(batch_id.clone()))
            .unwrap_or(Vec::new(&env));

        let mut found_idx: Option<u32> = None;
        for (i, pending) in pending_list.iter().enumerate() {
            if pending.mutation_id == mutation_id {
                found_idx = Some(i as u32);
                break;
            }
        }

        let idx = match found_idx {
            Some(i) => i,
            None => return false,
        };

        let mut mutation: PendingMutation = pending_list.get(idx).unwrap();

        if env.ledger().sequence() <= mutation.expires_at {
            return false;
        }

        if mutation.status != MutationStatus::Pending && mutation.status != MutationStatus::ReadyToCommit {
            return false;
        }

        // Mark expired
        mutation.status = MutationStatus::Expired;
        pending_list.set(idx, mutation);
        env.storage()
            .persistent()
            .set(&StateKey::PendingMap(batch_id.clone()), &pending_list);

        env.events().publish(
            (symbol_short!("expired"), batch_id, mutation_id),
            env.ledger().sequence(),
        );

        true
    }

    /// Get current state version
    pub fn get_version(env: Env) -> StateVersion {
        env.storage()
            .persistent()
            .get(&StateKey::Version)
            .unwrap()
    }

    /// Get a state value by key
    pub fn get_state_value(env: Env, key: Bytes) -> Bytes {
        env.storage()
            .persistent()
            .get(&StateKey::StateValue(key))
            .unwrap_or(Bytes::from_slice(&env, &[]))
    }

    /// Get pending mutation by ID
    pub fn get_pending(env: Env, batch_id: Bytes, mutation_id: Bytes) -> Option<PendingMutation> {
        let pending_list: Vec<PendingMutation> = env
            .storage()
            .persistent()
            .get(&StateKey::PendingMap(batch_id))
            .unwrap_or(Vec::new(&env));

        for pending in pending_list.iter() {
            if pending.mutation_id == mutation_id {
                return Some(pending);
            }
        }
        None
    }

    /// Generate mutation_id from batch_id and seq_no
    fn make_mutation_id(env: &Env, batch_id: &Bytes, seq_no: u64) -> Bytes {
        let mut data = Bytes::new(env);
        for i in 0..batch_id.len() {
            data.push_back(batch_id.get(i).unwrap());
        }
        let seq_bytes = seq_no.to_be_bytes();
        for &b in &seq_bytes {
            data.push_back(b);
        }
        data
    }
}
