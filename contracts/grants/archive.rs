use soroban_sdk::{Env, Symbol};
use super::{DataKey, Grant, GrantError, GrantStatus};

pub fn archive_grant(env: Env, grant_id: u64) {
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

    env.storage().persistent().remove(&key);
    env.events().publish((Symbol::new(&env, "grant_archived"),), grant_id);
}