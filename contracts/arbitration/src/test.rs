#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};
use soroban_sdk::token::Client as TokenClient;

#[test]
fn test_arbitration() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let funder = Address::generate(&env);
    let grantee = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    
    let token_admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract(token_admin);
    let token_client = token::StellarAssetClient::new(&env, &token_addr);
    token_client.mint(&funder, &1000);

    let contract_id = env.register_contract(None, ArbitrationContract);
    let client = ArbitrationContractClient::new(&env, &contract_id);

    // Register and initialize Treasury Contract (SpeedBumpContract)
    let treasury_id = env.register_contract(None, treasury::SpeedBumpContract);
    let treasury_client = treasury::SpeedBumpContractClient::new(&env, &treasury_id);
    treasury_client.initialize(&admin, &token_addr, &100_000u64);

    client.init(&admin, &token_addr, &treasury_id);
    let dispute_id = client.raise_dispute(&1, &funder, &grantee, &1000, &arbitrator);
    
    // Resolve dispute
    client.resolve_dispute(&dispute_id, &500, &500);
    
    let real_token = token::Client::new(&env, &token_addr);
    assert_eq!(real_token.balance(&funder), 500);
    assert_eq!(real_token.balance(&grantee), 500);
}

#[test]
fn test_arbitration_speed_bump_large_award() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let funder = Address::generate(&env);
    let grantee = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    
    let token_admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract(token_admin);
    let token_client = token::StellarAssetClient::new(&env, &token_addr);
    token_client.mint(&funder, &1000);

    // Register and initialize Treasury Contract (SpeedBumpContract)
    let treasury_id = env.register_contract(None, treasury::SpeedBumpContract);
    let treasury_client = treasury::SpeedBumpContractClient::new(&env, &treasury_id);
    // Initialize with a treasury snapshot of 5,000. 10% threshold = 500 tokens.
    // Our payout of 1000 will exceed the threshold.
    treasury_client.initialize(&admin, &token_addr, &5_000u64);

    let contract_id = env.register_contract(None, ArbitrationContract);
    let client = ArbitrationContractClient::new(&env, &contract_id);

    client.init(&admin, &token_addr, &treasury_id);
    let dispute_id = client.raise_dispute(&1, &funder, &grantee, &1000, &arbitrator);
    
    // Resolve dispute with 1000 award to funder (exceeds 500 threshold)
    client.resolve_dispute(&dispute_id, &1000, &0);
    
    let real_token = token::Client::new(&env, &token_addr);
    // Funder should not have received the award yet because it's queued in the speed bump
    assert_eq!(real_token.balance(&funder), 0);
    // The funds are now in the treasury contract
    assert_eq!(real_token.balance(&treasury_id), 1000);

    // Get the pending transfer ID
    let pending = treasury_client.get_pending_transfers();
    assert_eq!(pending.len(), 1);
    let transfer_id = pending.get(0).unwrap().id;

    // Advance time by 72 hours + 1 second
    env.ledger().with_mut(|li| {
        li.timestamp += 72 * 60 * 60 + 1;
    });

    // Execute the pending transfer
    treasury_client.execute_transfer(&admin, &transfer_id);

    // Funder now has the 1000 tokens
    assert_eq!(real_token.balance(&funder), 1000);
    assert_eq!(real_token.balance(&treasury_id), 0);
}
