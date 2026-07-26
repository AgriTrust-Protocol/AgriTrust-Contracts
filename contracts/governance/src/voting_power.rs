use soroban_sdk::{Address, Env};
use soroban_sdk::contracttype;

use crate::StorageKey;

#[derive(Clone, Debug)]
#[contracttype]
pub struct LockedBalance {
    pub amount: i128,
    pub lock_weeks: u32,
    pub unlock_time: u64,
}

pub struct VotingPower;

impl VotingPower {
    pub fn initialize(env: &Env) {
        env.storage().instance().set(&StorageKey::MaxMultiplier, &3i128);
        env.storage().instance().set(&StorageKey::BaseMultiplier, &1i128);
    }

    pub fn lock_tokens(
        env: &Env,
        holder: &Address,
        amount: i128,
        duration_weeks: u32,
    ) {
        if amount <= 0 {
            panic!("amount must be positive");
        }
        if duration_weeks < 1 {
            panic!("duration must be at least 1 week");
        }

        let token: Address = env.storage().instance().get(&StorageKey::GovernanceToken).expect("not initialized");

        let token_client = soroban_sdk::token::Client::new(env, &token);
        token_client.transfer(holder, &env.current_contract_address(), &amount);

        let now = env.ledger().timestamp();
        let unlock_time = now.checked_add(
            (duration_weeks as u64).checked_mul(604800).expect("overflow"),
        ).expect("overflow");

        let locked = LockedBalance {
            amount,
            lock_weeks: duration_weeks,
            unlock_time,
        };

        env.storage().instance().set(&StorageKey::LockedBalance(holder.clone()), &locked);

        env.events().publish(
            (soroban_sdk::symbol_short!("lock"),),
            (holder.clone(), amount, duration_weeks, unlock_time),
        );
    }

    pub fn withdraw_locked(env: &Env, holder: &Address) {
        let locked: LockedBalance = env.storage()
            .instance()
            .get(&StorageKey::LockedBalance(holder.clone()))
            .expect("no locked balance");

        let now = env.ledger().timestamp();
        if now < locked.unlock_time {
            panic!("tokens still locked");
        }

        env.storage().instance().remove(&StorageKey::LockedBalance(holder.clone()));

        let token: Address = env.storage().instance().get(&StorageKey::GovernanceToken).expect("not initialized");
        let token_client = soroban_sdk::token::Client::new(env, &token);
        token_client.transfer(&env.current_contract_address(), holder, &locked.amount);

        env.events().publish(
            (soroban_sdk::symbol_short!("withdraw"),),
            (holder.clone(), locked.amount),
        );
    }

    pub fn integer_sqrt(n: i128) -> i128 {
        if n <= 0 {
            return 0;
        }
        let mut x = n;
        let mut y = (x + 1) / 2;
        while y < x {
            x = y;
            y = (x + n / x) / 2;
        }
        x
    }

    pub fn calculate_power(env: &Env, holder: &Address) -> i128 {
        let base_voting_power = match env.storage().instance().get::<_, LockedBalance>(&StorageKey::LockedBalance(holder.clone())) {
            Some(locked) => locked.amount,
            None => {
                let token: Address = env.storage().instance().get(&StorageKey::GovernanceToken).expect("not initialized");
                let token_client = soroban_sdk::token::Client::new(env, &token);
                token_client.balance(holder)
            }
        };

        if base_voting_power <= 0 {
            return 0;
        }

        match env.storage().instance().get::<_, LockedBalance>(&StorageKey::LockedBalance(holder.clone())) {
            Some(locked) => {
                let max_mult: i128 = env.storage().instance().get(&StorageKey::MaxMultiplier).unwrap_or(3);
                let base_mult: i128 = env.storage().instance().get(&StorageKey::BaseMultiplier).unwrap_or(1);

                let multiplier = if locked.lock_weeks >= 156 {
                    max_mult
                } else {
                    let additional = (locked.lock_weeks as i128).checked_mul(
                        max_mult.checked_sub(base_mult).expect("overflow"),
                    ).expect("overflow") / 156;
                    base_mult.checked_add(additional).expect("overflow").min(max_mult)
                };

                base_voting_power.checked_mul(multiplier).expect("overflow")
            }
            None => base_voting_power,
        }
    }
}
