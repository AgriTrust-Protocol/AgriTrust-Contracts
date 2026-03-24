#![cfg(test)]
extern crate std;

use crate::{Error, GrantContract, GrantContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn create_client(env: &Env) -> GrantContractClient<'_> {
    let contract_id = env.register(GrantContract, ());
    GrantContractClient::new(env, &contract_id)
}

#[test]
fn test_adjust_for_inflation_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    let client = create_client(&env);

    let admin = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);

    client.initialize(&admin, &grant_token, &treasury, &oracle, &native_token);

    let grant_id = 1;
    let recipient = Address::generate(&env);
    // Base flow_rate of 1000
    client.create_grant(&grant_id, &recipient, &100_000_000, &1000, &0, &1);

    // Test 1: A 4% index increase (100 -> 104) should FAIL since it's under the 5% threshold
    let res = client.try_adjust_for_inflation(&grant_id, &100, &104);
    assert_eq!(res, Err(Ok(Error::ThresholdNotMet)));

    // Test 2: A 5% index increase (100 -> 105) should SUCCEED
    client.adjust_for_inflation(&grant_id, &100, &105);
    let grant = client.get_grant(&grant_id);
    
    // The new rate should be strictly proportionate: 1000 * 105 / 100 = 1050
    assert_eq!(grant.flow_rate, 1050);
}

#[test]
fn test_adjust_for_inflation_max_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let client = create_client(&env);

    let admin = Address::generate(&env);
    let grant_token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);

    client.initialize(&admin, &grant_token, &treasury, &oracle, &native_token);

    let grant_id = 1;
    let recipient = Address::generate(&env);
    client.create_grant(&grant_id, &recipient, &100_000_000, &1000, &0, &1);

    client.set_max_flow_rate(&grant_id, &1500);

    // Trigger a 100% hyper-inflation spike (Index goes from 100 to 200)
    client.adjust_for_inflation(&grant_id, &100, &200);
    
    // The flow rate should logically be 2000, but is intercepted and capped at 1500
    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.flow_rate, 1500);
}