#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::Address as _, Address, Env, Vec,
};

#[test]
fn test_batch_init_success() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let grant_token = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);

    // Initialize contract
    env.as_contract(&contract_id, || {
        GrantContract::initialize(
            env.clone(),
            admin,
            grant_token.clone(),
            treasury,
            oracle,
            native_token,
        ).unwrap();

        // Create test grantees
        let recipient1 = Address::generate(&env);
        let recipient2 = Address::generate(&env);
        let recipient3 = Address::generate(&env);

        let mut grantee_configs = Vec::new(&env);
        
        // Grant 1: 1000 tokens, 1 token/second, 30 day warmup
        grantee_configs.push_back(GranteeConfig {
            recipient: recipient1.clone(),
            total_amount: 1000_0000000, // 1000 tokens (7 decimals)
            flow_rate: 1_0000000,       // 1 token/second
            asset: grant_token.clone(),
            warmup_duration: 30 * 24 * 60 * 60, // 30 days
            validator: None,
        });

        // Grant 2: 2000 tokens, 2 tokens/second, no warmup
        grantee_configs.push_back(GranteeConfig {
            recipient: recipient2.clone(),
            total_amount: 2000_0000000,
            flow_rate: 2_0000000,
            asset: grant_token.clone(),
            warmup_duration: 0,
            validator: None,
        });

        // Grant 3: 500 tokens, 0.5 tokens/second, 7 day warmup, with validator
        let validator = Address::generate(&env);
        grantee_configs.push_back(GranteeConfig {
            recipient: recipient3.clone(),
            total_amount: 500_0000000,
            flow_rate: 5000000, // 0.5 tokens/second
            asset: grant_token.clone(),
            warmup_duration: 7 * 24 * 60 * 60, // 7 days
            validator: Some(validator.clone()),
        });

        // Execute batch initialization
        let result = GrantContract::batch_init(env.clone(), grantee_configs, 1000).unwrap();

        // Verify results
        assert_eq!(result.grants_created, 3);
        assert_eq!(result.successful_grants.len(), 3);
        assert_eq!(result.failed_grants.len(), 0);
        assert_eq!(result.total_deposited, 3500_0000000); // 1000 + 2000 + 500
        assert_eq!(result.successful_grants.get(0).unwrap(), 1000);
        assert_eq!(result.successful_grants.get(1).unwrap(), 1001);
        assert_eq!(result.successful_grants.get(2).unwrap(), 1002);

        // Verify individual grants were created correctly
        let grant1 = GrantContract::get_grant(env.clone(), 1000).unwrap();
        assert_eq!(grant1.recipient, recipient1);
        assert_eq!(grant1.total_amount, 1000_0000000);
        assert_eq!(grant1.flow_rate, 1_0000000);
        assert_eq!(grant1.warmup_duration, 30 * 24 * 60 * 60);
        assert_eq!(grant1.validator, None);

        let grant2 = GrantContract::get_grant(env.clone(), 1001).unwrap();
        assert_eq!(grant2.recipient, recipient2);
        assert_eq!(grant2.total_amount, 2000_0000000);
        assert_eq!(grant2.flow_rate, 2_0000000);
        assert_eq!(grant2.warmup_duration, 0);

        let grant3 = GrantContract::get_grant(env.clone(), 1002).unwrap();
        assert_eq!(grant3.recipient, recipient3);
        assert_eq!(grant3.total_amount, 500_0000000);
        assert_eq!(grant3.flow_rate, 5000000);
        assert_eq!(grant3.warmup_duration, 7 * 24 * 60 * 60);
        assert_eq!(grant3.validator, Some(validator));
    });
}

#[test]
fn test_batch_init_empty_configs() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let grant_token = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);

    env.as_contract(&contract_id, || {
        GrantContract::initialize(
            env.clone(),
            admin,
            grant_token,
            treasury,
            oracle,
            native_token,
        ).unwrap();

        let empty_configs = Vec::new(&env);
        let result = GrantContract::batch_init(env.clone(), empty_configs, 1000);
        
        assert_eq!(result, Err(Error::InvalidAmount));
    });
}

#[test]
fn test_batch_init_invalid_amounts() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let grant_token = Address::generate(&env);

    let contract_id = env.register_contract(None, GrantContract);

    env.as_contract(&contract_id, || {
        GrantContract::initialize(
            env.clone(),
            admin,
            grant_token.clone(),
            treasury,
            oracle,
            native_token,
        ).unwrap();

        let recipient = Address::generate(&env);
        let mut grantee_configs = Vec::new(&env);
        
        // Invalid: zero total amount
        grantee_configs.push_back(GranteeConfig {
            recipient,
            total_amount: 0,
            flow_rate: 1_0000000,
            asset: grant_token,
            warmup_duration: 0,
            validator: None,
        });

        let result = GrantContract::batch_init(env.clone(), grantee_configs, 1000);
        assert_eq!(result, Err(Error::InvalidAmount));
    });
}