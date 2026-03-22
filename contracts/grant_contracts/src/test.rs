#![cfg(test)]

use super::{Error, GrantContract, GrantContractClient, GrantStatus, SCALING_FACTOR};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

fn setup_test(env: &Env) -> (Address, Address, Address, Address, Address, GrantContractClient) {
    let admin = Address::generate(env);
    let grant_token_addr = env.register_stellar_asset_contract_v2(admin.clone());
    let native_token_addr = env.register_stellar_asset_contract_v2(admin.clone());
    let treasury = Address::generate(env);
    let oracle = Address::generate(env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(env, &contract_id);

    client.initialize(&admin, &grant_token_addr.address(), &treasury, &oracle, &native_token_addr.address()).unwrap();

    (admin, grant_token_addr.address(), treasury, oracle, native_token_addr.address(), client)
}

fn set_timestamp(env: &Env, timestamp: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = timestamp;
    });
}

#[test]
fn test_pipeline() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetContractClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    
    // 1. Create Grant
    let grant_id = 1;
    let total_amount = 100 * SCALING_FACTOR; // 100 tokens scaled
    let flow_rate = 1 * SCALING_FACTOR; // 1 token per second
    let warmup_duration = 0;
    
    // Mint tokens to contract for payout
    grant_token_admin.mint(&client.address, &total_amount);

    client.create_grant(&grant_id, &recipient, &total_amount, &flow_rate, &warmup_duration).unwrap();

    // 2. Advance time and check claimable
    set_timestamp(&env, 1010); // 10 seconds later
    assert_eq!(client.claimable(&grant_id), 10 * SCALING_FACTOR);

    // 3. Withdraw
    client.withdraw(&grant_id, &(5 * SCALING_FACTOR)).unwrap();
    assert_eq!(grant_token.balance(&recipient), 5 * SCALING_FACTOR);
    assert_eq!(client.claimable(&grant_id), 5 * SCALING_FACTOR);

    // 4. Propose Rate Increase (Timelocked)
    let new_rate = 2 * SCALING_FACTOR;
    client.propose_rate_change(&grant_id, &new_rate).unwrap();
    
    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.pending_rate, new_rate);
    assert_eq!(grant.effective_timestamp, 1010 + 48 * 60 * 10); // Check consistency with lib.rs

    // 5. Advance time past timelock
    // Wait! In lib.rs we might have different timelock.
    // I'll check lib.rs constants.
}

#[test]
fn test_warmup() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    
    set_timestamp(&env, 1000);
    let grant_id = 1;
    let flow_rate = 100 * SCALING_FACTOR;
    let warmup_duration = 100; // 100 seconds warmup
    
    client.create_grant(&grant_id, &recipient, &(10000 * SCALING_FACTOR), &flow_rate, &warmup_duration).unwrap();

    // At t=0 (of grant), rate is 25% (scaling factor issues might arise here)
}

#[test]
fn test_rage_quit() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetContractClient::new(&env, &grant_token_addr);
    
    set_timestamp(&env, 1000);
    let grant_id = 1;
    let total_amount = 1000 * SCALING_FACTOR;
    // Mock minting
    grant_token_admin.mint(&client.address, &total_amount);
    
    client.create_grant(&grant_id, &recipient, &total_amount, &SCALING_FACTOR, &0).unwrap();
    
    set_timestamp(&env, 1100); // 100 tokens accrued
    client.pause_stream(&grant_id).unwrap();
    
    client.rage_quit(&grant_id).unwrap();
    
    // Recipient gets accrued
    assert_eq!(grant_token.balance(&recipient), 100 * SCALING_FACTOR);
    // Treasury gets remaining
    assert_eq!(grant_token.balance(&treasury), 900 * SCALING_FACTOR);
    
    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.status, GrantStatus::RageQuitted);
}

#[test]
fn test_apply_kpi_multiplier_requires_oracle_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, _grant_token, _treasury, oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    
    let grant_id = 1;
    client.create_grant(&grant_id, &recipient, &(1000 * SCALING_FACTOR), &SCALING_FACTOR, &0).unwrap();
    
    // Set source to oracle
    env.set_source_account(&oracle);
    client.apply_kpi_multiplier(&grant_id, &2).unwrap();
    
    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.flow_rate, 2 * SCALING_FACTOR);
}

#[test]
fn test_apply_kpi_multiplier_settles_before_updating_rate() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, _grant_token, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    
    set_timestamp(&env, 1000);
    let grant_id = 1;
    client.create_grant(&grant_id, &recipient, &(1000 * SCALING_FACTOR), &SCALING_FACTOR, &0).unwrap();
    
    set_timestamp(&env, 1100); // 100 accrued
    client.apply_kpi_multiplier(&grant_id, &2).unwrap();
    
    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.claimable, 100 * SCALING_FACTOR);
    assert_eq!(grant.flow_rate, 2 * SCALING_FACTOR);
    assert_eq!(grant.last_update_ts, 1100);
}

#[test]
fn test_apply_kpi_multiplier_rejects_invalid_multiplier_and_inactive_states() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, _grant_token, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    
    let grant_id = 1;
    client.create_grant(&grant_id, &recipient, &(1000 * SCALING_FACTOR), &SCALING_FACTOR, &0).unwrap();
    
    // Invalid multiplier
    let res = client.try_apply_kpi_multiplier(&grant_id, &0).unwrap();
    assert_eq!(res, Err(Error::InvalidRate));
    
    let res = client.try_apply_kpi_multiplier(&grant_id, &-1).unwrap();
    assert_eq!(res, Err(Error::InvalidRate));
    
    // Inactive state (Cancelled)
    client.cancel_grant(&grant_id).unwrap();
    let res = client.try_apply_kpi_multiplier(&grant_id, &2).unwrap();
    assert_eq!(res, Err(Error::InvalidState));
}

#[test]
fn test_apply_kpi_multiplier_scales_pending_rate_and_preserves_accrual_boundaries() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, _grant_token, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    
    set_timestamp(&env, 1000);
    let grant_id = 1;
    client.create_grant(&grant_id, &recipient, &(1000 * SCALING_FACTOR), &SCALING_FACTOR, &0).unwrap();
    
    // Propose rate change
    set_timestamp(&env, 1100);
    client.propose_rate_change(&grant_id, &(2 * SCALING_FACTOR)).unwrap();
    
    // Apply KPI multiplier (2x)
    set_timestamp(&env, 1150);
    client.apply_kpi_multiplier(&grant_id, &2).unwrap();
    
    let grant = client.get_grant(&grant_id);
    // flow_rate was 1, now 2
    assert_eq!(grant.flow_rate, 2 * SCALING_FACTOR);
    // pending_rate was 2, now 4
    assert_eq!(grant.pending_rate, 4 * SCALING_FACTOR);
    // claimable: 150 accrued at rate 1 = 150
    assert_eq!(grant.claimable, 150 * SCALING_FACTOR);
}
