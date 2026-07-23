use soroban_sdk::{Address, Env};
use soroban_sdk::contracttype;

use crate::StorageKey;

#[derive(Clone, Debug)]
#[contracttype]
pub struct TimelockAction {
    pub proposal_id: u64,
    pub queue_time: u64,
    pub cancelled: bool,
}

pub struct TimeLock;

impl TimeLock {
    pub const DELAY_SEC: u64 = 172800;

    pub fn initialize(env: &Env) {
        env.storage().instance().set(&StorageKey::TimelockDelay, &Self::DELAY_SEC);
    }

    pub fn queue_action(env: &Env, proposal_id: u64, queue_time: u64) {
        let action = TimelockAction {
            proposal_id,
            queue_time,
            cancelled: false,
        };
        env.storage().instance().set(&StorageKey::TimelockAction(proposal_id), &action);

        env.events().publish(
            (soroban_sdk::symbol_short!("queue"), proposal_id),
            queue_time,
        );
    }

    pub fn cancel_action(env: &Env, proposal_id: u64) {
        let mut action: TimelockAction = env.storage()
            .instance()
            .get(&StorageKey::TimelockAction(proposal_id))
            .expect("timelock action not found");

        action.cancelled = true;
        env.storage().instance().set(&StorageKey::TimelockAction(proposal_id), &action);

        env.events().publish(
            (soroban_sdk::symbol_short!("tl_cancel"), proposal_id),
            env.ledger().timestamp(),
        );
    }

    pub fn assert_can_execute(env: &Env, proposal_id: u64, queue_time: u64) {
        let now = env.ledger().timestamp();

        if now < queue_time.checked_add(Self::DELAY_SEC).expect("overflow") {
            panic!("timelock delay not elapsed");
        }

        if let Some(action) = env.storage().instance().get::<_, TimelockAction>(&StorageKey::TimelockAction(proposal_id)) {
            if action.cancelled {
                panic!("timelock action was cancelled");
            }
        }
    }
}
