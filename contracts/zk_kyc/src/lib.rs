#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Verifier,
    KycStatus(Address),
}

#[contract]
pub struct ZKKYCContract;

#[contractimpl]
impl ZKKYCContract {
    pub fn init(env: Env, verifier: Address) {
        if env.storage().instance().has(&DataKey::Verifier) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Verifier, &verifier);
    }

    pub fn verify_user(env: Env, user: Address) {
        let verifier: Address = env.storage().instance().get(&DataKey::Verifier).unwrap();
        verifier.require_auth();
        env.storage().persistent().set(&DataKey::KycStatus(user), &true);
    }

    pub fn revoke_user(env: Env, user: Address) {
        let verifier: Address = env.storage().instance().get(&DataKey::Verifier).unwrap();
        verifier.require_auth();
        env.storage().persistent().remove(&DataKey::KycStatus(user));
    }

    pub fn is_verified(env: Env, user: Address) -> bool {
        env.storage().persistent().get(&DataKey::KycStatus(user)).unwrap_or(false)
    }
}

#[cfg(test)]
mod test;
