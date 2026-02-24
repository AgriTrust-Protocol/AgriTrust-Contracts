#![cfg(test)]

use super::{Error, GrantContract, GrantContractClient, GrantStatus, SCALING_FACTOR};
use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction, Ledger},
    Address, Env, InvokeError,
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
    let grant_token = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 1;
    // Flow rates are now scaled by SCALING_FACTOR
    let rate_1: i128 = 10 * SCALING_FACTOR;
    let rate_2: i128 = 25 * SCALING_FACTOR;

    set_timestamp(&env, 1_000);
    client.mock_all_auths().initialize(&admin, &grant_token);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &10_000, &rate_1);

    set_timestamp(&env, 1_100);
    // 100 seconds * 10 tokens/sec = 1000 tokens
    assert_eq!(client.claimable(&grant_id), 1_000);

    client.mock_all_auths().update_rate(&grant_id, &rate_2);

    let grant_after_update = client.get_grant(&grant_id);
    assert_eq!(grant_after_update.claimable, 1_000);
    assert_eq!(grant_after_update.flow_rate, rate_2);
    assert_eq!(grant_after_update.last_update_ts, 1_100);
    assert_eq!(grant_after_update.rate_updated_at, 1_100);

    set_timestamp(&env, 1_140);
    // 1000 + (40 seconds * 25 tokens/sec) = 1000 + 1000 = 2000
    assert_eq!(client.claimable(&grant_id), 1_000 + 40 * 25);

    client.mock_all_auths().withdraw(&grant_id, &700);
    assert_eq!(client.claimable(&grant_id), 1_000 + 40 * 25 - 700);

    set_timestamp(&env, 1_150);
    // 1000 + (50 seconds * 25 tokens/sec) - 700 = 1000 + 1250 - 700 = 1550
    assert_eq!(client.claimable(&grant_id), 1_000 + 50 * 25 - 700);
}

#[test]
fn test_update_rate_requires_admin_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 2;

    set_timestamp(&env, 100);
    client.mock_all_auths().initialize(&admin, &grant_token);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &1_000, &(5 * SCALING_FACTOR));

    client.mock_all_auths().update_rate(&grant_id, &(7 * SCALING_FACTOR));

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
    let grant_token = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 3;

    set_timestamp(&env, 2_000);
    client.mock_all_auths().initialize(&admin, &grant_token);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &5_000, &(4 * SCALING_FACTOR));

    client.mock_all_auths().update_rate(&grant_id, &(9 * SCALING_FACTOR));

    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.claimable, 0);
    assert_eq!(grant.flow_rate, 9 * SCALING_FACTOR);
    assert_eq!(grant.last_update_ts, 2_000);

    set_timestamp(&env, 2_010);
    // 10 seconds * 9 tokens/sec = 90 tokens
    assert_eq!(client.claimable(&grant_id), 90);
}

#[test]
fn test_update_rate_multiple_times_with_time_gaps() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 4;

    set_timestamp(&env, 10);
    client.mock_all_auths().initialize(&admin, &grant_token);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &10_000, &(3 * SCALING_FACTOR));

    set_timestamp(&env, 20);
    client.mock_all_auths().update_rate(&grant_id, &(5 * SCALING_FACTOR));

    set_timestamp(&env, 40);
    client.mock_all_auths().update_rate(&grant_id, &(2 * SCALING_FACTOR));

    set_timestamp(&env, 70);
    // (10 sec * 3) + (20 sec * 5) + (30 sec * 2) = 30 + 100 + 60 = 190
    assert_eq!(client.claimable(&grant_id), 30 + 100 + 60);
}

#[test]
fn test_update_rate_pause_then_resume() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 5;

    set_timestamp(&env, 1_000);
    client.mock_all_auths().initialize(&admin, &grant_token);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &20_000, &(4 * SCALING_FACTOR));

    set_timestamp(&env, 1_050);
    // Pause by setting rate to 0
    client.mock_all_auths().update_rate(&grant_id, &0);
    // 50 seconds * 4 tokens/sec = 200 tokens
    assert_eq!(client.claimable(&grant_id), 200);

    set_timestamp(&env, 1_250);
    // Still 200 since rate is 0
    assert_eq!(client.claimable(&grant_id), 200);

    client.mock_all_auths().update_rate(&grant_id, &(6 * SCALING_FACTOR));

    set_timestamp(&env, 1_300);
    // 200 + (50 seconds * 6 tokens/sec) = 200 + 300 = 500
    assert_eq!(client.claimable(&grant_id), 200 + 50 * 6);
}

#[test]
fn test_update_rate_rejects_invalid_rate_and_inactive_states() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    set_timestamp(&env, 0);
    client.mock_all_auths().initialize(&admin, &grant_token);

    let negative_rate_grant: u64 = 6;
    client
        .mock_all_auths()
        .create_grant(&negative_rate_grant, &recipient, &1_000, &(5 * SCALING_FACTOR));
    assert_contract_error(
        client
            .mock_all_auths()
            .try_update_rate(&negative_rate_grant, &-1_i128),
        Error::InvalidRate,
    );

    let cancelled_grant: u64 = 7;
    client
        .mock_all_auths()
        .create_grant(&cancelled_grant, &recipient, &1_000, &(5 * SCALING_FACTOR));
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
        .create_grant(&completed_grant, &recipient, &100, &(10 * SCALING_FACTOR));
    set_timestamp(&env, 10);
    // 10 seconds * 10 tokens/sec = 100 tokens (full amount)
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
    let grant_token = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 9;

    set_timestamp(&env, 0);
    client.mock_all_auths().initialize(&admin, &grant_token);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &1_000, &(10 * SCALING_FACTOR));

    set_timestamp(&env, 20);
    client.mock_all_auths().update_rate(&grant_id, &(5 * SCALING_FACTOR));

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
fn test_rescue_tokens_requires_admin_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let to = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    client.mock_all_auths().initialize(&admin, &grant_token);
    // Calling rescue_tokens without admin auth must fail (auth or NotAuthorized).
    assert!(client.try_rescue_tokens(&grant_token, &100, &to).is_err());
}

#[test]
fn test_rescue_tokens_rejects_invalid_amount() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let to = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    client.mock_all_auths().initialize(&admin, &grant_token);
    assert_contract_error(
        client
            .mock_all_auths()
            .try_rescue_tokens(&grant_token, &0, &to),
        Error::InvalidAmount,
    );
    assert_contract_error(
        client
            .mock_all_auths()
            .try_rescue_tokens(&grant_token, &-1_i128, &to),
        Error::InvalidAmount,
    );
}
/// Tests for low-decimal tokens (Issue #18: High-Precision Flow Rates)
/// These tests verify that the scaling factor prevents zero flow rates
/// when dealing with tokens that have few decimal places.

#[test]
fn test_low_decimal_token_2_decimals_1_year() {
    // Scenario: 100 tokens with 2 decimals over 1 year
    // Without scaling: 10000 / 31536000 = 0 (integer division)
    // With scaling: (10000 * 1e7) / 31536000 = 3170 scaled rate
    // This allows proper accrual over time
    
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 100;
    let total_amount: i128 = 10_000; // 100 tokens with 2 decimals = 10000 base units
    let duration_seconds: u64 = 31_536_000; // 1 year in seconds
    
    // Calculate scaled flow rate: (amount * SCALING_FACTOR) / duration
    // This gives us a non-zero rate even for small amounts over long durations
    let scaled_flow_rate: i128 = (total_amount * SCALING_FACTOR) / (duration_seconds as i128);
    
    // Verify the scaled rate is non-zero (this would be 0 without scaling)
    assert!(scaled_flow_rate > 0, "Scaled flow rate should be non-zero");

    set_timestamp(&env, 0);
    client.mock_all_auths().initialize(&admin);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &total_amount, &scaled_flow_rate);

    // After 6 months (half the duration), should have ~50% vested
    let six_months: u64 = duration_seconds / 2;
    set_timestamp(&env, six_months);
    
    let claimable_at_6_months = client.claimable(&grant_id);
    // Allow some tolerance due to integer division
    let expected_half = total_amount / 2;
    let tolerance: i128 = 10; // Small tolerance for rounding
    assert!(
        (claimable_at_6_months - expected_half).abs() <= tolerance,
        "At 6 months, claimable {} should be close to {} (tolerance {})",
        claimable_at_6_months,
        expected_half,
        tolerance
    );

    // After full year, should have 100% vested (capped at total_amount)
    set_timestamp(&env, duration_seconds);
    let claimable_at_1_year = client.claimable(&grant_id);
    assert!(
        claimable_at_1_year >= total_amount - tolerance && claimable_at_1_year <= total_amount,
        "At 1 year, claimable {} should be close to total {}",
        claimable_at_1_year,
        total_amount
    );
}

#[test]
fn test_low_decimal_token_very_small_amount() {
    // Scenario: 1 token with 2 decimals (100 base units) over 1 day
    // Tests precision with very small amounts
    
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 101;
    let total_amount: i128 = 100; // 1 token with 2 decimals
    let duration_seconds: u64 = 86_400; // 1 day in seconds
    
    let scaled_flow_rate: i128 = (total_amount * SCALING_FACTOR) / (duration_seconds as i128);
    assert!(scaled_flow_rate > 0, "Scaled flow rate should be non-zero");

    set_timestamp(&env, 0);
    client.mock_all_auths().initialize(&admin);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &total_amount, &scaled_flow_rate);

    // After 12 hours, should have ~50 base units
    set_timestamp(&env, 43_200);
    let claimable = client.claimable(&grant_id);
    assert!(
        claimable >= 45 && claimable <= 55,
        "At 12 hours, claimable {} should be around 50",
        claimable
    );

    // After full day
    set_timestamp(&env, 86_400);
    let claimable_full = client.claimable(&grant_id);
    assert!(
        claimable_full >= 95 && claimable_full <= 100,
        "At 1 day, claimable {} should be close to 100",
        claimable_full
    );
}

#[test]
fn test_high_precision_long_duration_10_years() {
    // Scenario: Large grant over 10 years
    // Tests that precision is maintained over very long durations
    
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 102;
    let total_amount: i128 = 1_000_000_000; // 1 billion base units
    let duration_seconds: u64 = 315_360_000; // 10 years in seconds
    
    let scaled_flow_rate: i128 = (total_amount * SCALING_FACTOR) / (duration_seconds as i128);

    set_timestamp(&env, 0);
    client.mock_all_auths().initialize(&admin);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &total_amount, &scaled_flow_rate);

    // After 5 years
    let five_years: u64 = duration_seconds / 2;
    set_timestamp(&env, five_years);
    let claimable_5y = client.claimable(&grant_id);
    let expected_5y = total_amount / 2;
    let tolerance: i128 = 1000; // Allow small tolerance for large numbers
    assert!(
        (claimable_5y - expected_5y).abs() <= tolerance,
        "At 5 years, claimable {} should be close to {}",
        claimable_5y,
        expected_5y
    );

    // After 10 years
    set_timestamp(&env, duration_seconds);
    let claimable_10y = client.claimable(&grant_id);
    assert!(
        claimable_10y >= total_amount - tolerance && claimable_10y <= total_amount,
        "At 10 years, claimable {} should equal total {}",
        claimable_10y,
        total_amount
    );
}

#[test]
fn test_withdraw_converts_to_correct_decimals() {
    // Verify that withdraw returns amounts in correct token decimals
    // (not scaled values)
    
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 103;
    let total_amount: i128 = 1_000; // 10 tokens with 2 decimals
    
    // Simple rate: 10 tokens per second (scaled)
    let scaled_flow_rate: i128 = 10 * SCALING_FACTOR;

    set_timestamp(&env, 0);
    client.mock_all_auths().initialize(&admin);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &total_amount, &scaled_flow_rate);

    // After 50 seconds, should have 500 base units claimable
    set_timestamp(&env, 50);
    assert_eq!(client.claimable(&grant_id), 500);

    // Withdraw 300 base units
    client.mock_all_auths().withdraw(&grant_id, &300);
    
    let grant = client.get_grant(&grant_id);
    // Withdrawn should be in original token units, not scaled
    assert_eq!(grant.withdrawn, 300);
    assert_eq!(grant.claimable, 200);

    // Withdraw remaining
    client.mock_all_auths().withdraw(&grant_id, &200);
    
    let grant_after = client.get_grant(&grant_id);
    assert_eq!(grant_after.withdrawn, 500);
    assert_eq!(grant_after.claimable, 0);
}
