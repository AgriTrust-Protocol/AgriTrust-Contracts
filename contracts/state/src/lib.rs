//! State Module - Optimistic Concurrency Control
//!
//! Implements two-phase commit (2PC) with compensating transactions for
//! batch state mutations. Resolves race conditions in concurrent
//! optimistic transactions using per-batch sequence counters as a
//! linearization point.
//!
//! Protocol:
//! 1. `begin_optimistic()` — assigns seq_no, stores PendingMutation
//! 2. `commit_optimistic()` — 2PC: check seq_no, apply or compensate
//! 3. `rollback_mutation()` — reverts with compensation logging
//! 4. `expire_pending()` — timeout cleanup

#![no_std]

mod optimistic_mutator;
mod rollback;

pub use optimistic_mutator::{
    CompensationEntry, MutationStatus,
    PendingMutation, StateKey, StateVersion,
    OptimisticContract, OptimisticContractClient,
    OPTIMISTIC_LOCK_TIMEOUT, MAX_CONCURRENT_MUTATIONS,
};
pub use rollback::{apply_compensation, rollback_mutation, verify_compensation_integrity};

#[cfg(test)]
mod tests;
