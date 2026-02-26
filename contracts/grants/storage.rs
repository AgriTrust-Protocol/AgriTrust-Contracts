use soroban_sdk::Env;
use super::types::{Grant, GrantError};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Grant(u64),
}

// Helpers
pub fn get_grant(env: &Env, grant_id: u64) -> Result<Grant, GrantError> {
    env.storage()
        .persistent()
        .get(&DataKey::Grant(grant_id))
        .ok_or(GrantError::GrantNotFound)
}