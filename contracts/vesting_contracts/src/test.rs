#![cfg(test)]

use super::*;
use agritrust_common::storage_keys::{derive_legacy_storage_key, derive_storage_key, DOMAIN_GRANT};
use soroban_sdk::{BytesN, Env};

#[test]
fn domain_prefix_separates_grant_and_vesting_keys_for_same_identifier() {
    let env = Env::default();
    let grant_id = BytesN::from_array(&env, &[7; 32]);

    // Old derivation used only variant + identifier, so independent modules choosing the same
    // variant byte and grant ID produced identical raw keys.
    let old_grant_metadata_key =
        derive_legacy_storage_key(&env, VESTING_SCHEDULE_VARIANT, &grant_id);
    let old_vesting_schedule_key =
        derive_legacy_storage_key(&env, VESTING_SCHEDULE_VARIANT, &grant_id);
    assert_eq!(old_grant_metadata_key, old_vesting_schedule_key);

    let new_grant_metadata_key =
        derive_storage_key(&env, DOMAIN_GRANT, VESTING_SCHEDULE_VARIANT, &grant_id);
    let new_vesting_schedule_key = vesting_schedule_key(&env, &grant_id);
    assert_ne!(new_grant_metadata_key, new_vesting_schedule_key);
    assert_ne!(old_vesting_schedule_key, new_vesting_schedule_key);
}

#[test]
fn vesting_schedule_round_trips_under_domain_key() {
    let env = Env::default();
    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);
    let grant_id = BytesN::from_array(&env, &[11; 32]);

    let key = client.create_vesting_schedule(&grant_id, &1_000_0000, &100, &200);
    assert_eq!(key, vesting_schedule_key(&env, &grant_id));

    let stored = client.read_vesting_schedule(&grant_id);
    assert_eq!(stored.grant_id, grant_id);
    assert_eq!(stored.total_amount, 1_000_0000);
    assert_eq!(stored.released_amount, 0);
    assert_eq!(stored.start_time, 100);
    assert_eq!(stored.end_time, 200);
}

#[test]
fn migration_rewrites_legacy_key_and_removes_old_entry() {
    let env = Env::default();
    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);
    let grant_id = BytesN::from_array(&env, &[23; 32]);
    let legacy_key = legacy_vesting_schedule_key(&env, &grant_id);
    let schedule = VestingSchedule {
        grant_id: grant_id.clone(),
        total_amount: 42_0000000,
        released_amount: 7_0000000,
        start_time: 10,
        end_time: 100,
    };

    env.as_contract(&contract_id, || {
        env.storage().persistent().set(&legacy_key, &schedule);
    });

    let new_key = client.migrate_legacy_vesting_schedule(&grant_id);
    assert_eq!(new_key, vesting_schedule_key(&env, &grant_id));

    env.as_contract(&contract_id, || {
        assert!(!env.storage().persistent().has(&legacy_key));
    });
    assert_eq!(client.read_vesting_schedule(&grant_id), schedule);
}
