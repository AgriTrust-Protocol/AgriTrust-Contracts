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

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 1;
    let rate_1: i128 = 10;
    let rate_2: i128 = 25;

    set_timestamp(&env, 1_000);
    client.mock_all_auths().initialize(&admin);
    let asset = Address::generate(&env);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &admin, &recipient, &asset, &10_000, &rate_1, &None, &false);

    set_timestamp(&env, 1_100);
    assert_eq!(client.claimable(&grant_id).unwrap(), 1_000);

    client.mock_all_auths().update_rate(&grant_id, &rate_2);

    let grant_after_update = client.get_grant(&grant_id);
    assert_eq!(grant_after_update.claimable, 1_000);
    assert_eq!(grant_after_update.flow_rate, rate_2);
}

#[test]
fn test_update_rate_requires_admin_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id: u64 = 2;

    set_timestamp(&env, 100);
    client.mock_all_auths().initialize(&admin);
    let asset = Address::generate(&env);
    client
        .mock_all_auths()
        .create_grant(&grant_id, &admin, &recipient, &asset, &1_000, &5, &None, &false);

    client.mock_all_auths().update_rate(&grant_id, &7_i128);

    let auths = env.auths();
    assert_eq!(auths.len(), 1);
}

#[test]
fn test_health_factor() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let asset = Address::generate(&env);
    
    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);
    
    client.mock_all_auths().initialize(&admin);
    
    assert_eq!(client.calculate_pool_health(&asset, &100).unwrap(), 10000);
    
    client.mock_all_auths().create_grant(&1, &admin, &recipient, &asset, &100_000, &10, &None, &false);
    
    assert_eq!(client.calculate_pool_health(&asset, &1000).unwrap(), 90000);
}

#[test]
fn test_optimistic_governance() {
    let env = Env::default();
    let submitter = Address::generate(&env);
    let recipient = Address::generate(&env);
    let challenger = Address::generate(&env);
    
    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);
    
    set_timestamp(&env, 1000);
    client.mock_all_auths().submit_optimistic_grant(&1, &recipient, &400, &submitter);
    
    assert_contract_error(
        client.mock_all_auths().try_submit_optimistic_grant(&2, &recipient, &600, &submitter),
        Error::InvalidAmount
    );
    
    set_timestamp(&env, 1000 + 3600);
    client.mock_all_auths().challenge_optimistic_grant(&1, &challenger);
    
    set_timestamp(&env, 1000 + 200000);
    client.mock_all_auths().submit_optimistic_grant(&3, &recipient, &100, &submitter);
    assert_contract_error(
        client.mock_all_auths().try_challenge_optimistic_grant(&3, &challenger),
        Error::InvalidState
    );
}

#[test]
fn test_joint_grant_dual_signatures() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let partner = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    client.mock_all_auths().initialize(&admin);
    
    client.mock_all_auths().set_protocol_config(
        &Address::generate(&env),
        &Address::generate(&env),
        &Address::generate(&env),
        &2000,
    );

    let grant_id = 100;
    let asset = Address::generate(&env);
    client.mock_all_auths().create_grant(
        &grant_id,
        &admin,
        &recipient,
        &asset,
        &5000,
        &10,
        &Some(partner.clone()),
        &false,
    );

    set_timestamp(&env, 100);
    client.mock_all_auths().withdraw(&grant_id, &500);
    
    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.withdrawn, 500);
}
