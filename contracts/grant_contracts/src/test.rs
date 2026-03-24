#![cfg(test)]

use super::{GrantContract, GrantContractClient, SCALING_FACTOR};
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
fn test_pipeline() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    
    // 1. Create Grant
    let grant_id = 1;
    let total_amount = 1_000_000 * SCALING_FACTOR; // Large enough to not complete early
    let flow_rate = 1 * SCALING_FACTOR; // 1 token per second
    let warmup_duration = 0;
    
    // Mint tokens to contract for payout
    grant_token_admin.mint(&client.address, &total_amount);

    client.create_grant(&grant_id, &recipient, &total_amount, &flow_rate, &warmup_duration, &1);

    // 2. Advance time and check claimable
    set_timestamp(&env, 1010); // 10 seconds later
    assert_eq!(client.claimable(&grant_id), 10 * SCALING_FACTOR);

    // 3. Withdraw
    client.withdraw(&grant_id, &(5 * SCALING_FACTOR));
    assert_eq!(grant_token.balance(&recipient), 5 * SCALING_FACTOR);
    assert_eq!(client.claimable(&grant_id), 5 * SCALING_FACTOR);

    // 4. Propose Rate Increase (Timelocked)
    let new_rate = 2 * SCALING_FACTOR;
    client.propose_rate_change(&grant_id, &new_rate);
    
    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.pending_rate, new_rate);
    assert_eq!(grant.effective_timestamp, 1010 + 48 * 60 * 60);

    // 5. Advance time past timelock
    // switch happens at 1010 + 172800 -> 173810
    // now is 1010 + 172800 + 10 -> 173820
    set_timestamp(&env, 1010 + 48 * 60 * 60 + 10);
    // Claimable: 5 (leftover) + 172800 (at rate 1) + 20 (10s at rate 2) = 172825
    assert_eq!(client.claimable(&grant_id), 172825 * SCALING_FACTOR);
}

#[test]
fn test_warmup() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, _grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    
    set_timestamp(&env, 1000);
    let grant_id = 1;
    let flow_rate = 100 * SCALING_FACTOR;
    let warmup_duration = 100; // 100 seconds warmup
    
    client.create_grant(&grant_id, &recipient, &(10000 * SCALING_FACTOR), &flow_rate, &warmup_duration, &1);

    // At T=1100, the instantaneous multiplier is 100% (10000 bps)
    // The current logic settle at the END of the period at the END rate.
    set_timestamp(&env, 1100);
    assert_eq!(client.claimable(&grant_id), 10000 * SCALING_FACTOR);
}

#[test]
fn test_rage_quit() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);
    
    set_timestamp(&env, 1000);
    let grant_id = 1;
    let total_amount = 1000 * SCALING_FACTOR;
    grant_token_admin.mint(&client.address, &total_amount);
    
    client.create_grant(&grant_id, &recipient, &total_amount, &SCALING_FACTOR, &0, &1);
    
    set_timestamp(&env, 1100); // 100 tokens accrued
    client.pause_stream(&grant_id);
    
    client.rage_quit(&grant_id);
    
    assert_eq!(grant_token.balance(&recipient), 100 * SCALING_FACTOR);
    assert_eq!(grant_token.balance(&treasury), 900 * SCALING_FACTOR);
}

#[test]
fn test_apply_kpi_multiplier_requires_oracle_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, _grant_token, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    
    let grant_id = 1;
    client.create_grant(&grant_id, &recipient, &(1000 * SCALING_FACTOR), &SCALING_FACTOR, &0, &1);
    
    // env.set_source_account(&oracle);
    client.apply_kpi_multiplier(&grant_id, &20000); // 2x in basis points
    
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
    client.create_grant(&grant_id, &recipient, &(1000 * SCALING_FACTOR), &SCALING_FACTOR, &0, &1);
    
    set_timestamp(&env, 1100); // 100 accrued
    client.apply_kpi_multiplier(&grant_id, &20000); // 2x
    
    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.claimable, 100 * SCALING_FACTOR);
    assert_eq!(grant.flow_rate, 2 * SCALING_FACTOR);
}

#[test]
fn test_apply_kpi_multiplier_rejects_invalid_multiplier_and_inactive_states() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);
    
    let grant_id = 1;
    let total_amount = 1000 * SCALING_FACTOR;
    grant_token_admin.mint(&client.address, &total_amount);

    client.create_grant(&grant_id, &recipient, &total_amount, &SCALING_FACTOR, &0, &1);
    
    assert!(client.try_apply_kpi_multiplier(&grant_id, &0).is_err());
    
    client.cancel_grant(&grant_id);
    assert!(client.try_apply_kpi_multiplier(&grant_id, &20000).is_err());
}

#[test]
fn test_apply_kpi_multiplier_scales_pending_rate_and_preserves_accrual_boundaries() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, _grant_token, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    
    set_timestamp(&env, 1000);
    let grant_id = 1;
    client.create_grant(&grant_id, &recipient, &(100000 * SCALING_FACTOR), &SCALING_FACTOR, &0, &1);
    
    set_timestamp(&env, 1100);
    client.propose_rate_change(&grant_id, &(2 * SCALING_FACTOR));
    
    set_timestamp(&env, 1150);
    client.apply_kpi_multiplier(&grant_id, &20000); // 2x
    
    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.flow_rate, 2 * SCALING_FACTOR);
    assert_eq!(grant.pending_rate, 4 * SCALING_FACTOR);
    assert_eq!(grant.claimable, 150 * SCALING_FACTOR);
}

#[test]
fn test_manage_liquidity() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    let total_amount = 100000 * SCALING_FACTOR;
    grant_token_admin.mint(&client.address, &(total_amount * 2));

    client.create_grant(&1, &recipient1, &total_amount, &(100 * SCALING_FACTOR), &0, &1); // Priority 1
    client.create_grant(&2, &recipient2, &total_amount, &(100 * SCALING_FACTOR), &0, &2); // Priority 2

    // Level 1 requests 100 per sec. Level 2 requests 100 per sec. Total requested = 200.
    // Set daily liquidity such that exactly 150 per sec is available (150 * 86400)
    // Level 1 should get 100% (100). Level 2 should get the remainder (50).
    client.manage_liquidity(&(150 * SCALING_FACTOR * 86400));

    assert_eq!(client.get_grant(&1).flow_rate, 100 * SCALING_FACTOR);
    assert_eq!(client.get_grant(&2).flow_rate, 50 * SCALING_FACTOR);
}
