#![no_std]
use soroban_sdk::{contractimpl, Env, Address};

mod grant;
mod trigger;

use grant::{Grant, GrantStatus};
use trigger::trigger;

pub struct ConditionalFunding;

#[contractimpl]
impl ConditionalFunding {
    pub fn init_grant(env: Env, id: u32, owner: Address, amount: i128) -> Grant {
        let grant = Grant::new(id, owner.clone(), amount);
        env.storage().set(&id, &grant);
        grant
    }

    pub fn complete_grant(env: Env, id: u32) {
        let mut grant: Grant = env.storage().get(&id).unwrap();
        grant.complete();
        env.storage().set(&id, &grant);
    }

    pub fn start_dependent(env: Env, prereq_id: u32, dependent_id: u32) -> bool {
        let prereq: Grant = env.storage().get(&prereq_id).unwrap();
        let mut dependent: Grant = env.storage().get(&dependent_id).unwrap();

        let started = trigger(&env, &prereq, &mut dependent);
        env.storage().set(&dependent_id, &dependent);
        started
    }

    pub fn get_grant_status(env: Env, id: u32) -> GrantStatus {
        let grant: Grant = env.storage().get(&id).unwrap();
        grant.status
    }
}