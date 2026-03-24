#![cfg(test)]
extern crate std;

use crate::{GrantContract, GrantContractClient, SCALING_FACTOR};
use soroban_sdk::{
    testutils::Address as _,
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

#[test]
fn test_harvest_yield() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, grant_token_addr, treasury, _oracle, _native, client) = setup_test(&env);
    let recipient = Address::generate(&env);
    
    let grant_token_admin = token::StellarAssetClient::new(&env, &grant_token_addr);
    let grant_token = token::Client::new(&env, &grant_token_addr);
    
    let grant_id = 1;
    let total_amount = 1000 * SCALING_FACTOR;
    grant_token_admin.mint(&client.address, &total_amount);
    client.create_grant(&grant_id, &recipient, &total_amount, &SCALING_FACTOR, &0, &1);
    
    assert_eq!(client.get_yield(), 0);
    
    let yield_amount = 50 * SCALING_FACTOR;
    grant_token_admin.mint(&client.address, &yield_amount); // Simulate external Yield accrual
    
    assert_eq!(client.get_yield(), yield_amount);
    
    assert_eq!(client.harvest_yield(), yield_amount);
    assert_eq!(grant_token.balance(&treasury), yield_amount);
    assert_eq!(grant_token.balance(&client.address), total_amount); // Ensures Principal remains uncompromised
}