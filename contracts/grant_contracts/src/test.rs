#![cfg(test)]

use super::{Error, GrantContract, GrantContractClient, GrantStatus, StreamType};
use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction, Ledger},
    token, Address, Env, InvokeError,
};

fn set_timestamp(env: &Env, timestamp: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = timestamp;
    });
}

fn assert_contract_error<T, C>(
    result: Result<Result<T, C>, Result<Error, InvokeError>>,
    expected: Error,
) {
    assert!(matches!(result, Err(Ok(err)) if err == expected));
}

#[test]
fn test_update_rate_settles_before_changing_rate() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 1;
    let rate_1: i128 = 10;
    let rate_2: i128 = 25;
    let native_token = Address::generate(&env);

    set_timestamp(&env, 1_000);
    client.mock_all_auths().initialize(&admin, &native_token);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &10_000, &rate_1);

    set_timestamp(&env, 1_100);
    assert_eq!(client.claimable(&grant_id), 1_000);

    client.mock_all_auths().update_rate(&grant_id, &rate_2);

    let grant_after_update = client.get_grant(&grant_id);
    assert_eq!(grant_after_update.claimable, 1_000);
    assert_eq!(grant_after_update.flow_rate, rate_2);
    assert_eq!(grant_after_update.last_update_ts, 1_100);
    assert_eq!(grant_after_update.rate_updated_at, 1_100);

    set_timestamp(&env, 1_140);
    assert_eq!(client.claimable(&grant_id), 1_000 + (40 * rate_2));

    client.mock_all_auths().withdraw(&grant_id, &700);
    assert_eq!(client.claimable(&grant_id), 1_000 + (40 * rate_2) - 700);

    set_timestamp(&env, 1_150);
    assert_eq!(client.claimable(&grant_id), 1_000 + (50 * rate_2) - 700);
}

#[test]
fn test_update_rate_requires_admin_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 2;
    let native_token = Address::generate(&env);

    set_timestamp(&env, 100);
    client.mock_all_auths().initialize(&admin, &native_token);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &1_000, &5);

    client.mock_all_auths().update_rate(&grant_id, &7_i128);

    let auths = env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, admin);
    assert!(matches!(
        auths[0].1.function,
        AuthorizedFunction::Contract((_, _, _))
    ));
}

#[test]
fn test_update_rate_immediately_after_creation() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 3;
    let native_token = Address::generate(&env);

    set_timestamp(&env, 2_000);
    client.mock_all_auths().initialize(&admin, &native_token);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &5_000, &4);

    client.mock_all_auths().update_rate(&grant_id, &9);

    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.claimable, 0);
    assert_eq!(grant.flow_rate, 9);
    assert_eq!(grant.last_update_ts, 2_000);

    set_timestamp(&env, 2_010);
    assert_eq!(client.claimable(&grant_id), 90);
}

#[test]
fn test_update_rate_multiple_times_with_time_gaps() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 4;
    let native_token = Address::generate(&env);

    set_timestamp(&env, 10);
    client.mock_all_auths().initialize(&admin, &native_token);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &10_000, &3);

    set_timestamp(&env, 20);
    client.mock_all_auths().update_rate(&grant_id, &5);

    set_timestamp(&env, 40);
    client.mock_all_auths().update_rate(&grant_id, &2);

    set_timestamp(&env, 70);
    assert_eq!(client.claimable(&grant_id), 30 + 100 + 60);
}

#[test]
fn test_update_rate_pause_then_resume() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 5;
    let native_token = Address::generate(&env);

    set_timestamp(&env, 1_000);
    client.mock_all_auths().initialize(&admin, &native_token);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &20_000, &4);

    set_timestamp(&env, 1_050);
    client.mock_all_auths().update_rate(&grant_id, &0);
    assert_eq!(client.claimable(&grant_id), 200);

    set_timestamp(&env, 1_250);
    assert_eq!(client.claimable(&grant_id), 200);

    client.mock_all_auths().update_rate(&grant_id, &6);

    set_timestamp(&env, 1_300);
    assert_eq!(client.claimable(&grant_id), 200 + (50 * 6));
}

#[test]
fn test_update_rate_rejects_invalid_rate_and_inactive_states() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let native_token = Address::generate(&env);
    set_timestamp(&env, 0);
    client.mock_all_auths().initialize(&admin, &native_token);

    let negative_rate_grant: u64 = 6;
    client
        .mock_all_auths()
        .create_grant(&negative_rate_grant, &recipient, &1_000, &5);
    assert_contract_error(
        client
            .mock_all_auths()
            .try_update_rate(&negative_rate_grant, &-1_i128),
        Error::InvalidRate,
    );

    let cancelled_grant: u64 = 7;
    client
        .mock_all_auths()
        .create_grant(&cancelled_grant, &recipient, &1_000, &5);
    client.mock_all_auths().cancel_grant(&cancelled_grant);
    assert_contract_error(
        client
            .mock_all_auths()
            .try_update_rate(&cancelled_grant, &8_i128),
        Error::InvalidState,
    );

    let completed_grant: u64 = 8;
    client
        .mock_all_auths()
        .create_grant(&completed_grant, &recipient, &100, &10);
    set_timestamp(&env, 10);
    client.mock_all_auths().withdraw(&completed_grant, &100);

    let completed = client.get_grant(&completed_grant);
    assert_eq!(completed.status, GrantStatus::Completed);

    assert_contract_error(
        client
            .mock_all_auths()
            .try_update_rate(&completed_grant, &4_i128),
        Error::InvalidState,
    );
}

#[test]
fn test_withdraw_after_rate_updates_no_extra_withdrawal() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 9;
    let native_token = Address::generate(&env);

    set_timestamp(&env, 0);
    client.mock_all_auths().initialize(&admin, &native_token);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &1_000, &10);

    set_timestamp(&env, 20);
    client.mock_all_auths().update_rate(&grant_id, &5);

    set_timestamp(&env, 60);
    assert_eq!(client.claimable(&grant_id), 400);

    client.mock_all_auths().withdraw(&grant_id, &400);
    assert_eq!(client.claimable(&grant_id), 0);

    assert_contract_error(
        client.mock_all_auths().try_withdraw(&grant_id, &1),
        Error::InvalidAmount,
    );

    set_timestamp(&env, 180);
    assert_eq!(client.claimable(&grant_id), 600);

    client.mock_all_auths().withdraw(&grant_id, &600);
    assert_eq!(client.claimable(&grant_id), 0);

    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.withdrawn, 1_000);
    assert_eq!(grant.status, GrantStatus::Completed);

    assert_contract_error(
        client.mock_all_auths().try_withdraw(&grant_id, &1),
        Error::InvalidAmount,
    );
}

#[test]
fn test_sbt_minting_and_metadata() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id_1: u64 = 101;
    let total_amount_1: i128 = 1000;
    let native_token = Address::generate(&env);

    set_timestamp(&env, 1000);
    client.mock_all_auths().initialize(&admin, &native_token);

    // Create first grant
    client.mock_all_auths().create_grant(&grant_id_1, &recipient, &total_amount_1, &10);

    // Verify SBT minted (grant ID in recipient's list)
    let grants = client.get_recipient_grants(&recipient);
    assert_eq!(grants.len(), 1);
    assert_eq!(grants.get(0).unwrap(), grant_id_1);

    // Verify Metadata (via get_grant)
    let grant_info = client.get_grant(&grant_id_1);
    assert_eq!(grant_info.total_amount, total_amount_1);
    assert_eq!(grant_info.status, GrantStatus::Active);
}

#[test]
fn test_extreme_network_congestion_6_months() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 42;
    let total_amount: i128 = 1_000_000_000_000_000; // 100M tokens
    let flow_rate: i128 = 10_000_000; // 1 token/sec
    let native_token = Address::generate(&env);

    let start_ts = 1_000_000;
    set_timestamp(&env, start_ts);

    client.mock_all_auths().initialize(&admin, &native_token);
    client.mock_all_auths().create_grant(&grant_id, &recipient, &total_amount, &flow_rate);

    // Simulate 6 months gap (182 days = 15,724,800 seconds)
    let gap_seconds = 15_724_800;
    let new_timestamp = start_ts + gap_seconds;
    set_timestamp(&env, new_timestamp);

    let claimable = client.claimable(&grant_id);
    let expected = flow_rate * (gap_seconds as i128);

    // Assert precision: Integer math ensures 0 precision loss here
    assert_eq!(claimable, expected);
    // Implicitly asserts loss < 0.00001% since it is 0.
}

#[test]
fn test_streaming_to_staking_redirect() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let staking_pool = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 50;
    let total_amount: i128 = 1000;
    let flow_rate: i128 = 10;
    let native_token = Address::generate(&env);

    set_timestamp(&env, 1000);
    client.mock_all_auths().initialize(&admin, &native_token);
    client.mock_all_auths().create_grant(&grant_id, &recipient, &total_amount, &flow_rate);

    // Set redirect
    client.mock_all_auths().set_redirect(&grant_id, &Some(staking_pool.clone()));

    // Verify redirect is set
    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.redirect, Some(staking_pool.clone()));

    // Advance time and withdraw
    set_timestamp(&env, 1050); // 50 seconds * 10 = 500 claimable
    
    // Withdraw should succeed (logic only in this mock)
    client.mock_all_auths().withdraw(&grant_id, &500);

    let grant_after = client.get_grant(&grant_id);
    assert_eq!(grant_after.withdrawn, 500);
    assert_eq!(grant_after.claimable, 0);
    
    // Clear redirect
    client.mock_all_auths().set_redirect(&grant_id, &None);
    let grant_cleared = client.get_grant(&grant_id);
    assert_eq!(grant_cleared.redirect, None);
}

#[test]
fn test_minimum_balance_fail_safe() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    // 1. Setup native token (XLM) and mint to contract
    let native_token_id = env.register_stellar_asset_contract(Address::generate(&env));
    let native_token_client = token::Client::new(&env, &native_token_id);

    const XLM_DECIMALS: u32 = 7;
    const RENT_RESERVE_XLM: i128 = 5 * 10i128.pow(XLM_DECIMALS); // 5 XLM
    let initial_balance = 10 * 10i128.pow(XLM_DECIMALS); // 10 XLM

    // Initialize contract
    client.initialize(&admin, &native_token_id);

    // Fund the contract
    native_token_client.mint(&contract_id, &initial_balance);
    assert_eq!(native_token_client.balance(&contract_id), initial_balance);

    // 2. Try to withdraw more than allowed (breaching reserve)
    let excessive_withdraw_amount = initial_balance - RENT_RESERVE_XLM + 1; // withdraw 5 XLM + 1 stroop
    let result = client.try_admin_withdraw(&excessive_withdraw_amount);
    assert_contract_error(result, Error::InsufficientReserve);

    // 3. Withdraw up to the limit
    let valid_withdraw_amount = initial_balance - RENT_RESERVE_XLM; // withdraw 5 XLM
    client.admin_withdraw(&valid_withdraw_amount);

    // 4. Verify balances
    assert_eq!(native_token_client.balance(&contract_id), RENT_RESERVE_XLM);
    assert_eq!(native_token_client.balance(&admin), valid_withdraw_amount);

    // 5. Try to withdraw anything more, should fail
    let result_after = client.try_admin_withdraw(&1);
    assert_contract_error(result_after, Error::InsufficientReserve);
}

#[test]
fn test_fixed_end_date_stream() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let native_token = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 200;
    let flow_rate: i128 = 100;

    set_timestamp(&env, 1000);
    client.mock_all_auths().initialize(&admin, &native_token);

    let end_ts = 1000 + 3600; // 1 hour later

    client.mock_all_auths().create_grant_until(&grant_id, &recipient, &flow_rate, &end_ts);

    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.stream_type, StreamType::FixedEndDate);
    assert_eq!(grant.total_amount, 3600 * 100);

    // Check claiming
    set_timestamp(&env, 1000 + 10);
    let claimable = client.claimable(&grant_id);
    assert_eq!(claimable, 1000);
}
