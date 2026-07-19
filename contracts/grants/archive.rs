use soroban_sdk::{Env, Symbol};
use super::{DataKey, Grant, GrantError, GrantStatus, GrantTombstone};

pub fn archive_grant(env: Env, grant_id: u64) {
    let lock = DataKey::ArchiveLock(grant_id);

    if env.storage().persistent().has(&lock) {
        panic!("ConcurrentArchiveInProgress");
    }
    env.storage().persistent().set(&lock, &true);

    let key = DataKey::Grant(grant_id);

    let grant: Grant = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(GrantError::GrantNotFound)
        .unwrap();

    match grant.status {
        GrantStatus::Completed | GrantStatus::Cancelled => {}
        _ => panic!("InvalidStatus"),
    }

    if grant.remaining_balance != 0 || grant.withdrawable_balance != 0 {
        panic!("NonZeroBalance");
    }

    let version: u32 = match env
        .storage()
        .persistent()
        .get(&DataKey::ArchiveVersion(grant_id))
    {
        Some(v) => v + 1,
        None => 1,
    };

    let tombstone = GrantTombstone {
        original_grant: grant.clone(),
        archived_at_ledger: env.ledger().sequence(),
        archive_version: version,
    };

    env.storage()
        .persistent()
        .set(&DataKey::GrantTombstone(grant_id, version), &tombstone);
    env.storage()
        .persistent()
        .set(&DataKey::ArchiveVersion(grant_id), &version);
    env.storage().persistent().remove(&DataKey::Grant(grant_id));
    env.storage().persistent().remove(&lock);

    env.events().publish((Symbol::new(&env, "grant_archived"),), grant_id);
}