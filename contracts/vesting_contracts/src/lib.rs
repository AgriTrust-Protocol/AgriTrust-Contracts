#![no_std]

use agritrust_common::storage_keys::{
    derive_legacy_storage_key, derive_storage_key, DOMAIN_VESTING,
};
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, BytesN, Env, Symbol,
};

const VESTING_SCHEDULE_VARIANT: u8 = 1;
const VESTING_TTL_LEDGERS: u32 = 3_924_000; // ~545 days at 5 seconds/ledger

#[contract]
pub struct Contract;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VestingSchedule {
    pub grant_id: BytesN<32>,
    pub total_amount: i128,
    pub released_amount: i128,
    pub start_time: u64,
    pub end_time: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    VestingSchedule(BytesN<32>),
    LegacyMigrationComplete,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum VestingError {
    InvalidAmount = 1,
    InvalidSchedule = 2,
    ScheduleNotFound = 3,
    ScheduleAlreadyExists = 4,
    MigrationSourceMissing = 5,
}

pub fn vesting_schedule_key(env: &Env, grant_id: &BytesN<32>) -> BytesN<32> {
    derive_storage_key(env, DOMAIN_VESTING, VESTING_SCHEDULE_VARIANT, grant_id)
}

pub fn legacy_vesting_schedule_key(env: &Env, grant_id: &BytesN<32>) -> BytesN<32> {
    derive_legacy_storage_key(env, VESTING_SCHEDULE_VARIANT, grant_id)
}

#[contractimpl]
impl Contract {
    pub fn create_vesting_schedule(
        env: Env,
        grant_id: BytesN<32>,
        total_amount: i128,
        start_time: u64,
        end_time: u64,
    ) -> BytesN<32> {
        if total_amount <= 0 {
            panic_with_error!(&env, VestingError::InvalidAmount);
        }
        if end_time <= start_time {
            panic_with_error!(&env, VestingError::InvalidSchedule);
        }

        let key = vesting_schedule_key(&env, &grant_id);
        if env.storage().persistent().has(&key) {
            panic_with_error!(&env, VestingError::ScheduleAlreadyExists);
        }

        let schedule = VestingSchedule {
            grant_id: grant_id.clone(),
            total_amount,
            released_amount: 0,
            start_time,
            end_time,
        };
        env.storage().persistent().set(&key, &schedule);
        env.storage()
            .persistent()
            .extend_ttl(&key, VESTING_TTL_LEDGERS, VESTING_TTL_LEDGERS);
        env.events().publish(
            (Symbol::new(&env, "vesting_created"), grant_id),
            key.clone(),
        );
        key
    }

    pub fn read_vesting_schedule(env: Env, grant_id: BytesN<32>) -> VestingSchedule {
        let key = vesting_schedule_key(&env, &grant_id);
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(&env, VestingError::ScheduleNotFound))
    }

    pub fn migrate_legacy_vesting_schedule(env: Env, grant_id: BytesN<32>) -> BytesN<32> {
        let old_key = legacy_vesting_schedule_key(&env, &grant_id);
        let new_key = vesting_schedule_key(&env, &grant_id);
        let schedule: VestingSchedule = env
            .storage()
            .persistent()
            .get(&old_key)
            .unwrap_or_else(|| panic_with_error!(&env, VestingError::MigrationSourceMissing));

        env.storage().persistent().set(&new_key, &schedule);
        env.storage()
            .persistent()
            .extend_ttl(&new_key, VESTING_TTL_LEDGERS, VESTING_TTL_LEDGERS);
        env.storage().persistent().remove(&old_key);
        env.events().publish(
            (Symbol::new(&env, "vesting_migrated"), grant_id),
            new_key.clone(),
        );
        new_key
    }

    pub fn storage_key_collision_check(env: Env, grant_id: BytesN<32>) -> bool {
        vesting_schedule_key(&env, &grant_id) == legacy_vesting_schedule_key(&env, &grant_id)
    }
}

#[cfg(test)]
mod test;
