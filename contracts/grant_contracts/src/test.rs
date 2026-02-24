#![cfg(test)]

use super::{Error, GrantContract, GrantContractClient, GrantStatus};
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
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 1;
    let rate_1: i128 = 10;
    let rate_2: i128 = 25;

    set_timestamp(&env, 1_000);
    client.mock_all_auths().initialize(&admin, &grant_token, &treasury);
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
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 2;

    set_timestamp(&env, 100);
    client.mock_all_auths().initialize(&admin, &grant_token, &treasury);
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
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 3;

    set_timestamp(&env, 2_000);
    client.mock_all_auths().initialize(&admin, &grant_token, &treasury);
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
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 4;

    set_timestamp(&env, 10);
    client.mock_all_auths().initialize(&admin, &grant_token, &treasury);
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
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 5;

    set_timestamp(&env, 1_000);
    client.mock_all_auths().initialize(&admin, &grant_token, &treasury);
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
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    set_timestamp(&env, 0);
    client.mock_all_auths().initialize(&admin, &grant_token, &treasury);

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
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 9;

    set_timestamp(&env, 0);
    client.mock_all_auths().initialize(&admin, &grant_token, &treasury);
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
fn test_rescue_tokens_requires_admin_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let to = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    client.mock_all_auths().initialize(&admin, &grant_token, &treasury);
    // Calling rescue_tokens without admin auth must fail (auth or NotAuthorized).
    assert!(client.try_rescue_tokens(&grant_token, &100, &to).is_err());
}

#[test]
fn test_rescue_tokens_rejects_invalid_amount() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let to = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    client.mock_all_auths().initialize(&admin, &grant_token, &treasury);
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

#[test]
fn test_slash_inactive_grant_reverts_if_less_than_90_days() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 10;
    set_timestamp(&env, 1_000);
    client.mock_all_auths().initialize(&admin, &grant_token, &treasury);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &5_000, &10);

    // 89 days later (less than 90) – slash should revert
    const SECS_89_DAYS: u64 = 89 * 24 * 60 * 60;
    set_timestamp(&env, 1_000 + SECS_89_DAYS);
    assert_contract_error(
        client.try_slash_inactive_grant(&grant_id),
        Error::GrantNotInactive,
    );

    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.status, GrantStatus::Active);
}

#[test]
fn test_slash_inactive_grant_reverts_if_not_active() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 11;
    set_timestamp(&env, 1_000);
    client.mock_all_auths().initialize(&admin, &grant_token, &treasury);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &1_000, &5);
    client.mock_all_auths().cancel_grant(&grant_id);

    assert_contract_error(
        client.try_slash_inactive_grant(&grant_id),
        Error::InvalidState,
    );
}

#[test]
fn test_slash_inactive_grant_updates_last_claim_time_on_withdraw() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 12;
    set_timestamp(&env, 1_000);
    client.mock_all_auths().initialize(&admin, &grant_token, &treasury);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &10_000, &10);

    set_timestamp(&env, 1_100);
    client.mock_all_auths().withdraw(&grant_id, &1_000);

    // 89 days after the withdraw (so last_claim_time = 1_100) – still not 90 days inactive
    const SECS_89_DAYS: u64 = 89 * 24 * 60 * 60;
    set_timestamp(&env, 1_100 + SECS_89_DAYS);
    assert_contract_error(
        client.try_slash_inactive_grant(&grant_id),
        Error::GrantNotInactive,
    );
}
