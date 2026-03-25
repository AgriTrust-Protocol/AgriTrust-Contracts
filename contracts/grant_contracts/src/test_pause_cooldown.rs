#![cfg(test)]

use super::{GrantContract, GrantContractClient, PAUSE_COOLDOWN_PERIOD, SUPER_MAJORITY_THRESHOLD};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _},
    token, Address, Env, Map, String, Vec, Symbol,
};

use crate::{GrantStatus, DataKey, Error};

const DAY: u64 = 24 * 60 * 60;

fn set_timestamp(env: &Env, timestamp: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = timestamp;
    });
}

fn setup_token(env: &Env, admin: &Address, amount: i128) -> Address {
    let token_address = env.register_stellar_asset_contract(admin.clone());
    token::StellarAssetClient::new(env, &token_address).mint(admin, &amount);
    token_address
}

#[test]
fn test_pause_cooldown_period() {
    let env = Env::default();
    env.mock_all_auths();
    set_timestamp(&env, 0);

    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let token_address = setup_token(&env, &admin, 1_000_000);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    // Initialize contract
    client.initialize(
        &admin,
        &token_address,
        &admin,
        &admin,
        &Address::generate(&env),
    );

    // Create a grant using the batch_init function since create_grant might not be available
    let mut configs = Vec::new(&env);
    configs.push_back(crate::GranteeConfig {
        recipient: recipient.clone(),
        total_amount: 1000i128,
        flow_rate: 100i128,
        asset: token_address,
        warmup_duration: 0,
        validator: None,
        milestone_amount: 0,
        total_milestones: 0,
        linked_addresses: Vec::new(&env),
    });

    let mut deposits = Map::new(&env);
    deposits.set(token_address, 1000i128);

    let result = client.batch_init_with_deposits(
        &configs,
        &deposits,
        &Some(1u64),
    );
    assert!(result.is_ok());

    let grant_id = 1u64;

    // Test 1: Initial pause should work
    let result = client.pause_stream(
        &admin,
        &grant_id,
        &String::from_str(&env, "Initial pause"),
        &false, // is_emergency
        &None,  // voting_power
    );
    assert!(result.is_ok());

    // Test 2: Resume should work
    let result = client.resume_stream(
        &admin,
        &grant_id,
        &String::from_str(&env, "Resume grant"),
    );
    assert!(result.is_ok());

    // Test 3: Pause during cooldown should fail
    let result = client.pause_stream(
        &admin,
        &grant_id,
        &String::from_str(&env, "Pause during cooldown"),
        &false, // is_emergency
        &None,  // voting_power
    );
    assert_eq!(result, Err(Error::PauseCooldownActive));

    // Test 4: Emergency pause without super-majority should fail
    let result = client.pause_stream(
        &admin,
        &grant_id,
        &String::from_str(&env, "Emergency pause without votes"),
        &true,  // is_emergency
        &Some(1000i128), // voting_power
    );
    assert_eq!(result, Err(Error::InsufficientSuperMajority));

    // Test 5: Emergency pause with super-majority should work
    // Set up total voting power
    client.set_total_voting_power(&10000i128);
    
    let super_majority_votes = (SUPER_MAJORITY_THRESHOLD * 10000i128 + 9999) / 10000; // Round up
    let result = client.pause_stream(
        &admin,
        &grant_id,
        &String::from_str(&env, "Emergency pause with super-majority"),
        &true,  // is_emergency
        &Some(super_majority_votes), // voting_power
    );
    assert!(result.is_ok());
}

#[test]
fn test_cooldown_expiration() {
    let env = Env::default();
    env.mock_all_auths();
    set_timestamp(&env, 0);

    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let token_address = setup_token(&env, &admin, 1_000_000);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    // Initialize contract
    client.initialize(
        &admin,
        &token_address,
        &admin,
        &admin,
        &Address::generate(&env),
    );

    // Create a grant
    let mut configs = Vec::new(&env);
    configs.push_back(crate::GranteeConfig {
        recipient: recipient.clone(),
        total_amount: 1000i128,
        flow_rate: 100i128,
        asset: token_address,
        warmup_duration: 0,
        validator: None,
        milestone_amount: 0,
        total_milestones: 0,
        linked_addresses: Vec::new(&env),
    });

    let mut deposits = Map::new(&env);
    deposits.set(token_address, 1000i128);

    client.batch_init_with_deposits(&configs, &deposits, &Some(1u64));

    let grant_id = 1u64;

    // Pause and resume
    client.pause_stream(
        &admin,
        &grant_id,
        &String::from_str(&env, "Initial pause"),
        &false,
        &None,
    );
    
    client.resume_stream(
        &admin,
        &grant_id,
        &String::from_str(&env, "Resume"),
    );

    // Try to pause immediately - should fail
    let result = client.pause_stream(
        &admin,
        &grant_id,
        &String::from_str(&env, "Pause during cooldown"),
        &false,
        &None,
    );
    assert_eq!(result, Err(Error::PauseCooldownActive));

    // Advance time beyond cooldown period
    set_timestamp(&env, PAUSE_COOLDOWN_PERIOD + 1);

    // Now pause should work
    let result = client.pause_stream(
        &admin,
        &grant_id,
        &String::from_str(&env, "Pause after cooldown"),
        &false,
        &None,
    );
    assert!(result.is_ok());
}
