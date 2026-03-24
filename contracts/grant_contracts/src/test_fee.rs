#![cfg(test)]
extern crate std;

use crate::{GrantContract, GrantContractClient, SCALING_FACTOR};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

fn setup_test(env: &Env) -> (Address, Address, Address, Address, Address, GrantContractClient<'_>) {
    let admin = Address::generate(env);
    let grant_token_addr = env.register_stellar_asset_contract_v2(admin.clone());
    let native_token_addr = env.register_stellar_asset_contract_v2(admin.clone());
    let treasury = Address::generate(env);
    let oracle = Address::generate(env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(env, &contract_id);

    client.initialize(&admin, &grant_token_addr.address(), &treasury, &oracle, &native_token_addr.address());

    (admin, grant_token_addr.address(), treasury, oracle, native_token_addr.address(), client)
}

fn set_timestamp(env: &Env, timestamp: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = timestamp;
    });
}

#[test]
fn test_fee_on_withdrawal() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);
    let grant_token = token::Client::new(&env, &grant_token_addr);

    client.set_platform_fee(&50); // Set a 0.5% fee

    set_timestamp(&env, 1000);
    let total_amount = 1000 * SCALING_FACTOR;
    grant_token_admin.mint(&client.address, &total_amount);

    client.create_grant(&1, &recipient, &total_amount, &SCALING_FACTOR, &0, &1);

    set_timestamp(&env, 1100); // 100 tokens accrued
    
    // Request withdrawing 100 tokens
    client.withdraw(&1, &(100 * SCALING_FACTOR));
    
    // Fee should be strictly 0.5% of 100 = 0.5 tokens
    // Recipient receives the remaining 99.5 tokens
    assert_eq!(grant_token.balance(&recipient), 995000000); // 99.5 * 10^7
    assert_eq!(grant_token.balance(&treasury), 5000000); // 0.5 * 10^7
}

#[test]
fn test_fee_on_rage_quit() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);
    let grant_token = token::Client::new(&env, &grant_token_addr);

    client.set_platform_fee(&50); // Set a 0.5% fee

    set_timestamp(&env, 1000);
    let total_amount = 1000 * SCALING_FACTOR;
    grant_token_admin.mint(&client.address, &total_amount);

    client.create_grant(&1, &recipient, &total_amount, &SCALING_FACTOR, &0, &1);

    set_timestamp(&env, 1100); // 100 tokens accrued
    client.pause_stream(&1);
    
    client.rage_quit(&1);
    
    // Recipient gets 99.5 tokens
    assert_eq!(grant_token.balance(&recipient), 995000000); // 99.5 * 10^7
    // Treasury gets the remaining stream (900) + the scraped fee (0.5) = 900.5 tokens
    assert_eq!(grant_token.balance(&treasury), 9005000000); // 900.5 * 10^7
}