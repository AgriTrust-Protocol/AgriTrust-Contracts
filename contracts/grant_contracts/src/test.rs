#![cfg(test)]

use super::{Error, GrantContract, GrantContractClient, GrantStatus};
use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction, Ledger},
    Address, Env, InvokeError,
};

const RATE_INCREASE_TIMELOCK_SECS: u64 = 48 * 60 * 60;

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
fn test_propose_rate_change_sets_pending_rate_and_effective_timestamp() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 1;

    set_timestamp(&env, 1_000);
    client.mock_all_auths().initialize(&admin, &oracle);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &50_000_000, &10);

    set_timestamp(&env, 1_100);
    client.mock_all_auths().propose_rate_change(&grant_id, &25);

    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.claimable, 1_000);
    assert_eq!(grant.flow_rate, 10);
    assert_eq!(grant.pending_rate, 25);
    assert_eq!(
        grant.effective_timestamp,
        1_100 + RATE_INCREASE_TIMELOCK_SECS
    );
    assert_eq!(grant.last_update_ts, 1_100);

    let just_before_activation = grant.effective_timestamp - 1;
    set_timestamp(&env, just_before_activation);
    let expected = 1_000 + (i128::from(just_before_activation - 1_100) * 10);
    assert_eq!(client.claimable(&grant_id), expected);
}

#[test]
fn test_withdraw_respects_timelock_for_rate_increases() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 2;

    set_timestamp(&env, 0);
    client.mock_all_auths().initialize(&admin, &oracle);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &5_000_000, &1);

    set_timestamp(&env, 10);
    client.mock_all_auths().propose_rate_change(&grant_id, &5);

    set_timestamp(&env, 20);
    assert_eq!(client.claimable(&grant_id), 20);

    assert_contract_error(
        client.mock_all_auths().try_withdraw(&grant_id, &21),
        Error::InvalidAmount,
    );
    client.mock_all_auths().withdraw(&grant_id, &20);

    let effective_timestamp = 10 + RATE_INCREASE_TIMELOCK_SECS;
    set_timestamp(&env, effective_timestamp - 1);
    let before_effective = i128::from((effective_timestamp - 1) - 20);
    assert_eq!(client.claimable(&grant_id), before_effective);

    set_timestamp(&env, effective_timestamp + 10);
    let after_effective = i128::from(effective_timestamp - 20) + (10 * 5);
    assert_eq!(client.claimable(&grant_id), after_effective);
}

#[test]
fn test_propose_rate_change_decrease_applies_immediately_and_clears_pending() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 3;

    set_timestamp(&env, 1_000);
    client.mock_all_auths().initialize(&admin, &oracle);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &50_000_000, &10);

    set_timestamp(&env, 1_100);
    client.mock_all_auths().propose_rate_change(&grant_id, &20);

    set_timestamp(&env, 1_200);
    client.mock_all_auths().propose_rate_change(&grant_id, &4);

    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.flow_rate, 4);
    assert_eq!(grant.pending_rate, 0);
    assert_eq!(grant.effective_timestamp, 0);
    assert_eq!(grant.rate_updated_at, 1_200);
    assert_eq!(grant.claimable, 2_000);

    set_timestamp(&env, 1_210);
    assert_eq!(client.claimable(&grant_id), 2_040);
}

#[test]
fn test_propose_rate_change_requires_admin_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 4;

    set_timestamp(&env, 100);
    client.mock_all_auths().initialize(&admin, &oracle);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &1_000, &5);

    client
        .mock_all_auths()
        .propose_rate_change(&grant_id, &7_i128);

    let auths = env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, admin);
    assert!(matches!(
        auths[0].1.function,
        AuthorizedFunction::Contract((_, _, _))
    ));
}

#[test]
fn test_propose_rate_change_rejects_invalid_rate_and_inactive_states() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    set_timestamp(&env, 0);
    client.mock_all_auths().initialize(&admin, &oracle);

    let negative_rate_grant: u64 = 5;
    client
        .mock_all_auths()
        .create_grant(&negative_rate_grant, &recipient, &1_000, &5);
    assert_contract_error(
        client
            .mock_all_auths()
            .try_propose_rate_change(&negative_rate_grant, &-1_i128),
        Error::InvalidRate,
    );

    let cancelled_grant: u64 = 6;
    client
        .mock_all_auths()
        .create_grant(&cancelled_grant, &recipient, &1_000, &5);
    client.mock_all_auths().cancel_grant(&cancelled_grant);
    assert_contract_error(
        client
            .mock_all_auths()
            .try_propose_rate_change(&cancelled_grant, &8_i128),
        Error::InvalidState,
    );

    let completed_grant: u64 = 7;
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
            .try_propose_rate_change(&completed_grant, &4_i128),
        Error::InvalidState,
    );
}

#[test]
fn test_update_rate_uses_timelocked_behavior() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 8;

    set_timestamp(&env, 0);
    client.mock_all_auths().initialize(&admin, &oracle);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &5_000_000, &2);

    set_timestamp(&env, 10);
    client.mock_all_auths().update_rate(&grant_id, &6);

    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.flow_rate, 2);
    assert_eq!(grant.pending_rate, 6);
    assert_eq!(grant.effective_timestamp, 10 + RATE_INCREASE_TIMELOCK_SECS);

    set_timestamp(&env, 20);
    assert_eq!(client.claimable(&grant_id), 40);
}

#[test]
fn test_apply_kpi_multiplier_requires_oracle_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 9;

    set_timestamp(&env, 0);
    client.mock_all_auths().initialize(&admin, &oracle);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &5_000_000, &2);

    set_timestamp(&env, 10);
    client.mock_all_auths().apply_kpi_multiplier(&grant_id, &2);

    let auths = env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, oracle);
    assert!(matches!(
        auths[0].1.function,
        AuthorizedFunction::Contract((_, _, _))
    ));
}

#[test]
fn test_apply_kpi_multiplier_settles_before_updating_rate() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 10;

    set_timestamp(&env, 0);
    client.mock_all_auths().initialize(&admin, &oracle);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &50_000_000, &10);

    set_timestamp(&env, 100);
    client.mock_all_auths().apply_kpi_multiplier(&grant_id, &3);

    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.claimable, 1_000);
    assert_eq!(grant.flow_rate, 30);
    assert_eq!(grant.last_update_ts, 100);

    set_timestamp(&env, 110);
    assert_eq!(client.claimable(&grant_id), 1_300);
}

#[test]
fn test_apply_kpi_multiplier_rejects_invalid_multiplier_and_inactive_states() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    set_timestamp(&env, 0);
    client.mock_all_auths().initialize(&admin, &oracle);

    let invalid_multiplier_grant: u64 = 11;
    client
        .mock_all_auths()
        .create_grant(&invalid_multiplier_grant, &recipient, &1_000, &5);
    assert_contract_error(
        client
            .mock_all_auths()
            .try_apply_kpi_multiplier(&invalid_multiplier_grant, &0_i128),
        Error::InvalidRate,
    );
    assert_contract_error(
        client
            .mock_all_auths()
            .try_apply_kpi_multiplier(&invalid_multiplier_grant, &-1_i128),
        Error::InvalidRate,
    );

    let cancelled_grant: u64 = 12;
    client
        .mock_all_auths()
        .create_grant(&cancelled_grant, &recipient, &1_000, &5);
    client.mock_all_auths().cancel_grant(&cancelled_grant);
    assert_contract_error(
        client
            .mock_all_auths()
            .try_apply_kpi_multiplier(&cancelled_grant, &2_i128),
        Error::InvalidState,
    );

    let completed_grant: u64 = 13;
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
            .try_apply_kpi_multiplier(&completed_grant, &2_i128),
        Error::InvalidState,
    );
}

#[test]
fn test_apply_kpi_multiplier_scales_pending_rate_and_preserves_accrual_boundaries() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 14;

    set_timestamp(&env, 0);
    client.mock_all_auths().initialize(&admin, &oracle);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &recipient, &500_000_000, &10);

    set_timestamp(&env, 100);
    client.mock_all_auths().propose_rate_change(&grant_id, &20);

    set_timestamp(&env, 150);
    client.mock_all_auths().apply_kpi_multiplier(&grant_id, &2);

    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.claimable, 1_500);
    assert_eq!(grant.flow_rate, 20);
    assert_eq!(grant.pending_rate, 40);
    assert_eq!(grant.last_update_ts, 150);
    assert_eq!(
        grant.effective_timestamp,
        100 + RATE_INCREASE_TIMELOCK_SECS
    );

    let just_before_activation = grant.effective_timestamp - 1;
    set_timestamp(&env, just_before_activation);
    let expected_before = 1_500 + (i128::from(just_before_activation - 150) * 20);
    assert_eq!(client.claimable(&grant_id), expected_before);

    set_timestamp(&env, grant.effective_timestamp + 10);
    let expected_after = 1_500
        + (i128::from(grant.effective_timestamp - 150) * 20)
        + (i128::from(10_u64) * 40);
    assert_eq!(client.claimable(&grant_id), expected_after);
}
