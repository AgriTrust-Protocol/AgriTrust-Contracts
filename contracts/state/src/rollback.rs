//! Rollback Module
//!
//! Provides rollback and compensation logic for optimistic mutations.
//! Ensures every rollback entry corresponds to a reverted state change
//! (invariant: ∀ rollback_entry: ∃ reverted_mutation).

use soroban_sdk::{contracttype, symbol_short, Bytes, Env, Map, Symbol, Vec};

use crate::optimistic_mutator::StateKey;
use crate::{
    CompensationEntry, MutationStatus, PendingMutation,
};

/// Rollback a mutation and create a compensating entry
pub fn rollback_mutation(
    env: &Env,
    batch_id: &Bytes,
    mutation_id: &Bytes,
) -> Option<CompensationEntry> {
    let mut pending_list: Vec<PendingMutation> = env
        .storage()
        .persistent()
        .get(&StateKey::PendingMap(batch_id.clone()))
        .unwrap_or(Vec::new(&env));

    let mut found_idx: Option<u32> = None;
    for (i, pending) in pending_list.iter().enumerate() {
        if pending.mutation_id == *mutation_id {
            found_idx = Some(i as u32);
            break;
        }
    }

    let idx = found_idx?;
    let mut mutation: PendingMutation = pending_list.get(idx).unwrap();

    // Restore previous values
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

    // Create compensation entry
    let compensation = CompensationEntry {
        original_mutation_id: mutation_id.clone(),
        compensation_state: mutation.prev_values.clone(),
        reason: symbol_short!("rb_module"),
        timestamp: env.ledger().sequence(),
    };

    // Store compensation
    let mut comp_list: Vec<CompensationEntry> = env
        .storage()
        .persistent()
        .get(&StateKey::CompensationMap(mutation_id.clone()))
        .unwrap_or(Vec::new(&env));
    comp_list.push_back(compensation.clone());
    env.storage()
        .persistent()
        .set(&StateKey::CompensationMap(mutation_id.clone()), &comp_list);

    // Mark rolled back
    mutation.status = MutationStatus::RolledBack;
    pending_list.set(idx, mutation);
    env.storage()
        .persistent()
        .set(&StateKey::PendingMap(batch_id.clone()), &pending_list);

    Some(compensation)
}

/// Apply a compensation entry to state (revert to previous values)
pub fn apply_compensation(env: &Env, compensation: &CompensationEntry) {
    for key in compensation.compensation_state.keys() {
        let k = key.clone();
        let value: Bytes = compensation.compensation_state.get(k.clone()).unwrap();
        if value.is_empty() {
            env.storage().persistent().remove(&StateKey::StateValue(k));
        } else {
            env.storage()
                .persistent()
                .set(&StateKey::StateValue(k), &value);
        }
    }
}

/// Verify compensation chain integrity for a mutation
pub fn verify_compensation_integrity(
    env: &Env,
    mutation_id: &Bytes,
) -> bool {
    let comp_list: Vec<CompensationEntry> = env
        .storage()
        .persistent()
        .get(&StateKey::CompensationMap(mutation_id.clone()))
        .unwrap_or(Vec::new(&env));

    for comp in comp_list.iter() {
        if comp.compensation_state.is_empty() {
            return false;
        }
    }

    true
}
