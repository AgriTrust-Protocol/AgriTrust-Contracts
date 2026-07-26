use soroban_sdk::Env;
use super::types::{Grant, GrantError, GrantTombstone};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Grant(u64),
    /// Lock held while an archive is in progress. Prevents a second
    /// `archive_grant` for the same `grant_id` from proceeding concurrently.
    ArchiveLock(u64),
    /// Monotonically increasing version counter for tombstone writes.
    /// Bumped on every successful archive of the grant.
    ArchiveVersion(u64),
    /// Versioned tombstone record. The second tuple element is the
    /// `archive_version` at the time of writing, so competing archivals
    /// never collide on the same key.
    GrantTombstone(u64, u32),
}

// ── Grant helpers ──────────────────────────────────────────────────────────

pub fn get_grant(env: &Env, grant_id: u64) -> Result<Grant, GrantError> {
    env.storage()
        .persistent()
        .get(&DataKey::Grant(grant_id))
        .ok_or(GrantError::GrantNotFound)
}

pub fn has_grant(env: &Env, grant_id: u64) -> bool {
    env.storage().persistent().has(&DataKey::Grant(grant_id))
}

pub fn remove_grant(env: &Env, grant_id: u64) {
    env.storage().persistent().remove(&DataKey::Grant(grant_id));
}

// ── Archive-lock helpers ───────────────────────────────────────────────────

/// Returns `true` when the lock is currently held (a concurrent archive is
/// in progress for the same `grant_id`).
pub fn is_archive_locked(env: &Env, grant_id: u64) -> bool {
    env.storage().persistent().has(&DataKey::ArchiveLock(grant_id))
}

/// Sets the archive lock (panics if already held).
pub fn set_archive_lock(env: &Env, grant_id: u64) {
    if is_archive_locked(env, grant_id) {
        panic_with_error!(env, GrantError::ConcurrentArchiveInProgress);
    }
    env.storage().persistent().set(&DataKey::ArchiveLock(grant_id), &true);
}

/// Clears the archive lock.
pub fn clear_archive_lock(env: &Env, grant_id: u64) {
    env.storage().persistent().remove(&DataKey::ArchiveLock(grant_id));
}

// ── Version counter helpers ────────────────────────────────────────────────

pub fn next_archive_version(env: &Env, grant_id: u64) -> u32 {
    let key = DataKey::ArchiveVersion(grant_id);
    let current: u32 = env.storage().persistent().get(&key).unwrap_or(0);
    let next = current.wrapping_add(1);
    env.storage().persistent().set(&key, &next);
    next
}

// ── Tombstone helpers ──────────────────────────────────────────────────────

pub fn write_tombstone(env: &Env, grant_id: u64, tombstone: &GrantTombstone) {
    let key = DataKey::GrantTombstone(grant_id, tombstone.archive_version);
    env.storage().persistent().set(&key, tombstone);
}

pub fn get_tombstone(env: &Env, grant_id: u64, version: u32) -> Option<GrantTombstone> {
    env.storage().persistent().get(&DataKey::GrantTombstone(grant_id, version))
}
