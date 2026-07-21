use soroban_sdk::{panic_with_error, Env, Symbol};
use super::{DataKey, Grant, GrantError, GrantStatus, GrantTombstone};

/// Archive a grant by moving it from active storage to a versioned tombstone.
///
/// **Lock semantics:** before doing any work the function acquires an
/// `ArchiveLock(grant_id)`. If another archive for the same `grant_id` is
/// already in progress, the call fails with
/// `GrantError::ConcurrentArchiveInProgress` instead of corrupting or
/// silently overwriting the in-flight tombstone.
///
/// **Versioned tombstone:** each successful archive writes a
/// `GrantTombstone(grant_id, version)` entry where `version` is a
/// monotonically bumped counter. This guarantees that two archivals of the
/// same grant never collide on the same storage key.
///
/// After the tombstone is written, the active `Grant(grant_id)` entry is
/// removed and the lock is cleared.
pub fn archive_grant(env: Env, grant_id: u64) {
    // 1. Acquire the archive lock — panic if another archive is in-flight.
    if super::is_archive_locked(&env, grant_id) {
        panic_with_error!(&env, GrantError::ConcurrentArchiveInProgress);
    }
    super::set_archive_lock(&env, grant_id);

    // 2. Read the grant (will abort with GrantNotFound if already removed).
    let key = DataKey::Grant(grant_id);
    let grant: Grant = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(GrantError::GrantNotFound)
        .unwrap();

    // 3. Validate status — only Completed/Cancelled grants can be archived.
    match grant.status {
        GrantStatus::Completed | GrantStatus::Cancelled => {}
        _ => panic_with_error!(&env, GrantError::InvalidStatus),
    }

    // 4. Balance must be fully withdrawn before archiving.
    if grant.remaining_balance != 0 || grant.withdrawable_balance != 0 {
        panic_with_error!(&env, GrantError::NonZeroBalance);
    }

    // 5. Get the next version & build the tombstone.
    let archive_version = super::next_archive_version(&env, grant_id);
    let tombstone = GrantTombstone {
        original_grant: grant.clone(),
        archived_at_ledger: env.ledger().sequence(),
        archive_version,
    };

    // 6. Write the versioned tombstone.
    super::write_tombstone(&env, grant_id, &tombstone);

    // 7. Remove the active grant entry.
    super::remove_grant(&env, grant_id);

    // 8. Clear the lock so the next archive can proceed.
    super::clear_archive_lock(&env, grant_id);

    // 9. Emit event.
    env.events().publish(
        (Symbol::new(&env, "grant_archived"), Symbol::new(&env, "locked")),
        grant_id,
    );
}
