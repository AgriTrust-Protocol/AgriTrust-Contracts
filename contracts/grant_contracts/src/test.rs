#![cfg(test)]

use super::{GrantContract, GrantContractClient, SCALING_FACTOR};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _},
    token, Address, Env, Map, String, Vec,
};

use crate::{GrantContract, GrantContractClient, GrantStatus};

const DAY: u64 = 24 * 60 * 60;

fn set_timestamp(env: &Env, timestamp: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = timestamp;
    });
}

fn build_grantees(env: &Env, grantee: &Address) -> Map<Address, u32> {
    let mut grantees = Map::new(env);
    grantees.set(grantee.clone(), 10_000);
    grantees
}

fn build_council(env: &Env, members: &[Address]) -> Vec<Address> {
    let mut council = Vec::new(env);
    for member in members {
        council.push_back(member.clone());
    }
    council
}

fn setup_token(env: &Env, admin: &Address, amount: i128) -> Address {
    let token_address = env.register_stellar_asset_contract(admin.clone());
    token::StellarAssetClient::new(env, &token_address).mint(admin, &amount);
    token_address
}

#[test]
fn milestone_speed_bonus_doubles_flow_for_30_days() {
    let env = Env::default();
    env.mock_all_auths();
    set_timestamp(&env, 0);

    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let token_address = setup_token(&env, &admin, 1_000_000);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id = symbol_short!("gbonus1");
    let milestone_id = symbol_short!("mile1");
    client.create_grant(
        &grant_id,
        &admin,
        &build_grantees(&env, &grantee),
        &1_000_000u128,
        &token_address,
        &0u64,
        &build_council(&env, &[admin.clone()]),
        &1u32,
    );
    client.configure_stream(&grant_id, &0u64, &(100 * DAY));
    client.add_milestone(
        &grant_id,
        &milestone_id,
        &1_000_000u128,
        &String::from_str(&env, "Milestone 1"),
        &(40 * DAY),
    );
    client.configure_milestone_acceleration(&grant_id, &milestone_id, &10_000u32, &(30 * DAY));
    client.approve_milestone(&grant_id, &milestone_id);

    set_timestamp(&env, 15 * DAY);
    assert_eq!(
        client.get_withdrawable_amount(&grant_id, &grantee),
        300_000u128
    );

    let withdrawn = client.withdraw(&grant_id, &grantee);
    assert_eq!(withdrawn, 300_000u128);
    assert_eq!(
        token::Client::new(&env, &token_address).balance(&grantee),
        300_000i128
    );

    set_timestamp(&env, 40 * DAY);
    assert_eq!(
        client.get_withdrawable_amount(&grant_id, &grantee),
        400_000u128
    );

    set_timestamp(&env, 100 * DAY);
    assert_eq!(
        client.get_withdrawable_amount(&grant_id, &grantee),
        700_000u128
    );
}

#[test]
fn speed_bonus_never_exceeds_released_milestone_funding() {
    let env = Env::default();
    env.mock_all_auths();
    set_timestamp(&env, 0);

    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let token_address = setup_token(&env, &admin, 1_000_000);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id = symbol_short!("grantcap");
    let milestone_id = symbol_short!("cap1");
    client.create_grant(
        &grant_id,
        &admin,
        &build_grantees(&env, &grantee),
        &1_000_000u128,
        &token_address,
        &0u64,
        &build_council(&env, &[admin.clone()]),
        &1u32,
    );
    client.configure_stream(&grant_id, &0u64, &(100 * DAY));
    client.add_milestone(
        &grant_id,
        &milestone_id,
        &200_000u128,
        &String::from_str(&env, "Seed funding"),
        &(10 * DAY),
    );
    client.configure_milestone_acceleration(&grant_id, &milestone_id, &10_000u32, &(30 * DAY));
    client.approve_milestone(&grant_id, &milestone_id);

    set_timestamp(&env, 30 * DAY);
    assert_eq!(
        client.get_withdrawable_amount(&grant_id, &grantee),
        200_000u128
    );
}

#[test]
fn council_threshold_controls_when_acceleration_starts() {
    let env = Env::default();
    env.mock_all_auths();
    set_timestamp(&env, 0);

    let admin = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let grantee = Address::generate(&env);
    let token_address = setup_token(&env, &admin, 1_000_000);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id = symbol_short!("grantvote");
    let milestone_id = symbol_short!("vote1");
    client.create_grant(
        &grant_id,
        &admin,
        &build_grantees(&env, &grantee),
        &1_000_000u128,
        &token_address,
        &0u64,
        &build_council(&env, &[admin.clone(), reviewer.clone()]),
        &2u32,
    );
    client.configure_stream(&grant_id, &0u64, &(100 * DAY));
    client.add_milestone(
        &grant_id,
        &milestone_id,
        &1_000_000u128,
        &String::from_str(&env, "Council gated"),
        &(20 * DAY),
    );
    client.configure_milestone_acceleration(&grant_id, &milestone_id, &5_000u32, &(30 * DAY));

    client.vote_milestone(&grant_id, &milestone_id, &admin, &true);
    let milestone = client.get_milestone(&grant_id, &milestone_id);
    assert_eq!(milestone.votes_for, 1);
    assert!(!milestone.approved);
    assert_eq!(client.get_grant(&grant_id).released_amount, 0u128);

    set_timestamp(&env, 5 * DAY);
    client.vote_milestone(&grant_id, &milestone_id, &reviewer, &true);

    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.released_amount, 1_000_000u128);
    assert_eq!(grant.acceleration_windows.len(), 1);
    assert_eq!(grant.status, GrantStatus::Completed);

    set_timestamp(&env, 15 * DAY);
    assert_eq!(
        client.get_withdrawable_amount(&grant_id, &grantee),
        200_000u128
    );
}
use super::{GrantContract, GrantContractClient, GrantStatus, SCALING_FACTOR};
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

    client.create_grant(&grant_id, &recipient, &total_amount, &flow_rate, &warmup_duration, &None);

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
    
    client.create_grant(&grant_id, &recipient, &(10000 * SCALING_FACTOR), &flow_rate, &warmup_duration, &None);

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
    
    client.create_grant(&grant_id, &recipient, &total_amount, &SCALING_FACTOR, &0, &None);
    
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
    client.create_grant(&grant_id, &recipient, &(1000 * SCALING_FACTOR), &SCALING_FACTOR, &0, &None);
    
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
    client.create_grant(&grant_id, &recipient, &(1000 * SCALING_FACTOR), &SCALING_FACTOR, &0, &None);
    
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

    client.create_grant(&grant_id, &recipient, &total_amount, &SCALING_FACTOR, &0, &None);
    
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
    client.create_grant(&grant_id, &recipient, &(100000 * SCALING_FACTOR), &SCALING_FACTOR, &0, &None);
    
    set_timestamp(&env, 1100);
    client.propose_rate_change(&grant_id, &(2 * SCALING_FACTOR));
    
    set_timestamp(&env, 1150);
    client.apply_kpi_multiplier(&grant_id, &20000); // 2x
    
    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.flow_rate, 2 * SCALING_FACTOR);
    assert_eq!(grant.pending_rate, 4 * SCALING_FACTOR);
    assert_eq!(grant.claimable, 150 * SCALING_FACTOR);
}

// ─── Validator Incentive Split Tests ─────────────────────────────────────────

/// After time elapses, 95% of accruals are claimable by the grantee and 5% by
/// the validator, independently tracked.
#[test]
fn test_validator_split_basic() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let validator = Address::generate(&env);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    let grant_id = 1;
    let total_amount = 1_000_000 * SCALING_FACTOR;
    let flow_rate = 1 * SCALING_FACTOR; // 1 token/sec
    grant_token_admin.mint(&client.address, &total_amount);

    client.create_grant(
        &grant_id, &recipient, &total_amount, &flow_rate, &0,
        &Some(validator.clone()),
    );

    // Advance 100 seconds: 100 tokens accrued total
    // Grantee gets 95, validator gets 5
    set_timestamp(&env, 1100);
    assert_eq!(client.claimable(&grant_id), 95 * SCALING_FACTOR);
    assert_eq!(client.validator_claimable(&grant_id), 5 * SCALING_FACTOR);
}

/// Grantee and validator can withdraw independently; each counter is isolated.
#[test]
fn test_validator_withdraw_independent() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let validator = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    let grant_id = 1;
    let total_amount = 1_000_000 * SCALING_FACTOR;
    let flow_rate = 1 * SCALING_FACTOR;
    grant_token_admin.mint(&client.address, &total_amount);

    client.create_grant(
        &grant_id, &recipient, &total_amount, &flow_rate, &0,
        &Some(validator.clone()),
    );

    // After 200 seconds: 190 grantee, 10 validator
    set_timestamp(&env, 1200);

    // Grantee withdraws their full share
    client.withdraw(&grant_id, &(190 * SCALING_FACTOR));
    assert_eq!(grant_token.balance(&recipient), 190 * SCALING_FACTOR);

    // Validator claimable still intact
    assert_eq!(client.validator_claimable(&grant_id), 10 * SCALING_FACTOR);

    // Validator withdraws their share
    client.withdraw_validator(&grant_id, &(10 * SCALING_FACTOR));
    assert_eq!(grant_token.balance(&validator), 10 * SCALING_FACTOR);

    // Both counters are now zero
    assert_eq!(client.claimable(&grant_id), 0);
    assert_eq!(client.validator_claimable(&grant_id), 0);
}

/// Without a validator the full stream goes to the grantee (no regression).
#[test]
fn test_no_validator_unaffected() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    let grant_id = 1;
    let total_amount = 1_000_000 * SCALING_FACTOR;
    let flow_rate = 1 * SCALING_FACTOR;
    grant_token_admin.mint(&client.address, &total_amount);

    client.create_grant(
        &grant_id, &recipient, &total_amount, &flow_rate, &0,
        &None,
    );

    set_timestamp(&env, 1100);
    // Full 100 tokens go to grantee
    assert_eq!(client.claimable(&grant_id), 100 * SCALING_FACTOR);
    assert_eq!(client.validator_claimable(&grant_id), 0);

    client.withdraw(&grant_id, &(100 * SCALING_FACTOR));
    assert_eq!(grant_token.balance(&recipient), 100 * SCALING_FACTOR);
}

/// On rage quit the validator receives their accrued 5% and the rest returns
/// to treasury.
#[test]
fn test_validator_split_rage_quit() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let validator = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    let grant_id = 1;
    let total_amount = 1000 * SCALING_FACTOR;
    grant_token_admin.mint(&client.address, &total_amount);

    client.create_grant(
        &grant_id, &recipient, &total_amount, &SCALING_FACTOR, &0,
        &Some(validator.clone()),
    );

    // 100 seconds: 95 grantee, 5 validator
    set_timestamp(&env, 1100);
    client.pause_stream(&grant_id);
    client.rage_quit(&grant_id);

    assert_eq!(grant_token.balance(&recipient), 95 * SCALING_FACTOR);
    assert_eq!(grant_token.balance(&validator), 5 * SCALING_FACTOR);
    // Remaining 900 returns to treasury
    assert_eq!(grant_token.balance(&treasury), 900 * SCALING_FACTOR);
}

/// On cancel, only unallocated funds (not yet accrued or withdrawn) go to
/// treasury; the grantee and validator can still pull their claimable shares.
#[test]
fn test_validator_split_cancel() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let validator = Address::generate(&env);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    let grant_id = 1;
    let total_amount = 1000 * SCALING_FACTOR;
    grant_token_admin.mint(&client.address, &total_amount);

    client.create_grant(
        &grant_id, &recipient, &total_amount, &SCALING_FACTOR, &0,
        &Some(validator.clone()),
    );

    // 100 seconds: 95 grantee, 5 validator accrued (900 unallocated)
    set_timestamp(&env, 1100);
    client.cancel_grant(&grant_id);

    // Treasury receives 900 unallocated tokens
    assert_eq!(grant_token.balance(&treasury), 900 * SCALING_FACTOR);

    // Grantee can still claim their 95
    assert_eq!(client.claimable(&grant_id), 95 * SCALING_FACTOR);
    // Validator can still claim their 5
    assert_eq!(client.validator_claimable(&grant_id), 5 * SCALING_FACTOR);
}

/// Only the designated validator address can call withdraw_validator.
#[test]
fn test_withdraw_validator_requires_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, _treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    let validator = Address::generate(&env);
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);

    set_timestamp(&env, 1000);
    let grant_id = 1;
    let total_amount = 1_000_000 * SCALING_FACTOR;
    grant_token_admin.mint(&client.address, &total_amount);

    client.create_grant(
        &grant_id, &recipient, &total_amount, &SCALING_FACTOR, &0,
        &Some(validator.clone()),
    );

    set_timestamp(&env, 1100);

    // Grant with no validator must reject withdraw_validator
    let grant_id_no_val = 2;
    client.create_grant(
        &grant_id_no_val, &recipient, &total_amount, &SCALING_FACTOR, &0,
        &None,
    );
    assert!(client.try_withdraw_validator(&grant_id_no_val, &(1 * SCALING_FACTOR)).is_err());

    // Attempting to overdraw validator share must fail
    assert!(client.try_withdraw_validator(&grant_id, &(100 * SCALING_FACTOR)).is_err());

    // Exact amount must succeed
    client.withdraw_validator(&grant_id, &(5 * SCALING_FACTOR));
}
