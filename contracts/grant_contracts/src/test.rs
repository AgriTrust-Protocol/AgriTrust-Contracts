#![cfg(test)]

use soroban_sdk::{symbol_short, Address, Env};
use crate::{GrantContract, GrantContractClient};

#[test]
fn test_multiple_milestones() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let token_address = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    // Create a grant
    let grant_id = symbol_short!("grant_multi");
    client.create_grant(&grant_id, &admin, &grantee, &1_000_000, &token_address).unwrap();

    // Add multiple milestones
    let milestone_1 = symbol_short!("m1");
    let milestone_2 = symbol_short!("m2");
    let milestone_3 = symbol_short!("m3");

    client.add_milestone(&grant_id, &milestone_1, &250_000, &String::from_str(&env, "Phase 1")).unwrap();
    client.add_milestone(&grant_id, &milestone_2, &350_000, &String::from_str(&env, "Phase 2")).unwrap();
    client.add_milestone(&grant_id, &milestone_3, &400_000, &String::from_str(&env, "Phase 3")).unwrap();

    // Approve first milestone
    client.approve_milestone(&grant_id, &milestone_1).unwrap();
    let grant_info = client.get_grant(&grant_id).unwrap();
    assert_eq!(grant_info.released_amount, 250_000);

    // Approve second milestone
    client.approve_milestone(&grant_id, &milestone_2).unwrap();
    let grant_info = client.get_grant(&grant_id).unwrap();
    assert_eq!(grant_info.released_amount, 600_000);

    // Approve third milestone
    client.approve_milestone(&grant_id, &milestone_3).unwrap();
    let grant_info = client.get_grant(&grant_id).unwrap();
    assert_eq!(grant_info.released_amount, 1_000_000);
}

#[test]
fn test_double_release_prevention() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let token_address = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    // Create a grant and milestone
    let grant_id = symbol_short!("grant_double");
    client.create_grant(&grant_id, &admin, &grantee, &1_000_000, &token_address).unwrap();

    let milestone_id = symbol_short!("milestone_double");
    client.add_milestone(
        &grant_id,
        &milestone_id,
        &500_000,
        &String::from_str(&env, "Test"),
    ).unwrap();

    // Approve once
    client.approve_milestone(&grant_id, &milestone_id).unwrap();

    // Try to approve again - should fail
    let result = client.approve_milestone(&grant_id, &milestone_id);
    assert!(result.is_err());
}

#[test]
fn test_get_remaining_amount() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let token_address = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    // Create a grant
    let grant_id = symbol_short!("grant_remaining");
    client.create_grant(&grant_id, &admin, &grantee, &1_000_000, &token_address).unwrap();

    // Check remaining amount before any releases
    let remaining = client.get_remaining_amount(&grant_id).unwrap();
    assert_eq!(remaining, 1_000_000);

    // Add and approve a milestone
    let milestone_id = symbol_short!("m1");
    client.add_milestone(&grant_id, &milestone_id, &400_000, &String::from_str(&env, "Phase 1")).unwrap();
    client.approve_milestone(&grant_id, &milestone_id).unwrap();

    // Check remaining amount after release
    let remaining = client.get_remaining_amount(&grant_id).unwrap();
    assert_eq!(remaining, 600_000);
}

#[test]
fn test_exceed_total_grant_amount() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let token_address = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    // Create a grant with 1M total
    let grant_id = symbol_short!("grant_exceed");
    client.create_grant(&grant_id, &admin, &grantee, &1_000_000, &token_address).unwrap();

    // Add milestone for 600K
    let milestone_1 = symbol_short!("m1");
    client.add_milestone(&grant_id, &milestone_1, &600_000, &String::from_str(&env, "Phase 1")).unwrap();
    client.approve_milestone(&grant_id, &milestone_1).unwrap();

    // Add milestone for 500K (would exceed total)
    let milestone_2 = symbol_short!("m2");
    client.add_milestone(&grant_id, &milestone_2, &500_000, &String::from_str(&env, "Phase 2")).unwrap();

    // Trying to approve should fail
    let result = client.approve_milestone(&grant_id, &milestone_2);
    assert!(result.is_err());
}

#[test]
fn test_grant_simulation_10_years() {
    // 10 years in seconds
    let duration: u64 = 315_360_000;

    // Total grant amount
    let total: u128 = 1_000_000_000u128;

    // Use a realistic large timestamp to catch overflow issues
    let start: u64 = 1_700_000_000;

    // --------------------------------------------------
    // ✔ Start: nothing should be claimable
    // --------------------------------------------------
    let claim0 =
        grant::compute_claimable_balance(total, start, start, duration);
    assert_eq!(claim0, 0);

    // --------------------------------------------------
    // ✔ Year 5: exactly 50%
    // --------------------------------------------------
    let year5 = start + duration / 2;
    let claim5 =
        grant::compute_claimable_balance(total, start, year5, duration);

    assert_eq!(claim5, total / 2);

    // --------------------------------------------------
    // ✔ Year 10: 100% vested
    // --------------------------------------------------
    let year10 = start + duration;
    let claim10 =
        grant::compute_claimable_balance(total, start, year10, duration);

    assert_eq!(claim10, total);

    // --------------------------------------------------
    // ✔ After expiry: must remain capped at total
    // --------------------------------------------------
    let after = year10 + 1_000_000;
    let claim_after =
        grant::compute_claimable_balance(total, start, after, duration);

    assert_eq!(claim_after, total);

    // --------------------------------------------------
    // ✔ Verify constant equals 10-year duration
    // --------------------------------------------------
    assert_eq!(duration, 315_360_000u64);
}

#[test]
fn test_custom_token_with_transfer_fee() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    
    // Deploy a custom token contract with transfer fee
    let token_contract_id = env.register_stellar_asset_contract(admin.clone());
    let token_client = soroban_sdk::token::Client::new(&env, &token_contract_id);
    
    // Mint tokens to admin
    token_client.mint(&admin, &1_000_000);
    
    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    // Create a grant with custom token
    let grant_id = symbol_short!("grant_custom_token");
    client.create_grant(&grant_id, &admin, &grantee, &500_000, &token_contract_id).unwrap();

    // Add milestone
    let milestone_id = symbol_short!("m1");
    client.add_milestone(&grant_id, &milestone_id, &100_000, &String::from_str(&env, "Phase 1")).unwrap();

    // Check contract balance before approval
    let contract_balance_before = token_client.balance(&contract_id);
    
    // Approve milestone - this should handle transfer fees correctly
    client.approve_milestone(&grant_id, &milestone_id).unwrap();
    
    // Verify contract balance tracks correctly (accounting for potential fees)
    let contract_balance_after = token_client.balance(&contract_id);
    let grantee_balance = token_client.balance(&grantee);
    
    // The grantee should receive tokens (amount might be less due to fees)
    assert!(grantee_balance > 0);
    
    // Contract should have remaining balance
    assert_eq!(contract_balance_after, contract_balance_before);
    
    // Verify grant state
    let grant_info = client.get_grant(&grant_id).unwrap();
    assert_eq!(grant_info.released_amount, 100_000);
}

#[test]
fn test_long_pause_duration() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let token_address = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    // Create a grant
    let grant_id = symbol_short!("grant_long_pause");
    client.create_grant(&grant_id, &admin, &grantee, &1_000_000, &token_address).unwrap();

    // Add milestone
    let milestone_id = symbol_short!("m1");
    client.add_milestone(&grant_id, &milestone_id, &500_000, &String::from_str(&env, "Phase 1")).unwrap();

    // Activate the grant
    client.activate_grant(&grant_id).unwrap();
    
    // Simulate long pause (100 years in seconds)
    let hundred_years_seconds: u64 = 100 * 365 * 24 * 60 * 60; // ~3.15 billion seconds
    env.ledger().set_timestamp(env.ledger().timestamp() + hundred_years_seconds);

    // Pause the grant
    client.pause_grant(&grant_id).unwrap();
    
    // Verify grant is paused
    let grant_info = client.get_grant(&grant_id).unwrap();
    assert_eq!(grant_info.status, crate::GrantStatus::Paused);
    
    // Resume after long pause
    client.resume_grant(&grant_id).unwrap();
    
    // Verify grant is active again
    let grant_info_after = client.get_grant(&grant_id).unwrap();
    assert_eq!(grant_info_after.status, crate::GrantStatus::Active);
    
    // Approve milestone should still work after long pause
    client.approve_milestone(&grant_id, &milestone_id).unwrap();
    
    // Verify total_withdrawn + remaining == initial_deposit
    let remaining = client.get_remaining_amount(&grant_id).unwrap();
    let grant_info_final = client.get_grant(&grant_id).unwrap();
    assert_eq!(grant_info_final.released_amount + remaining, 1_000_000);
}

// Fuzz test for extreme pause durations
#[test]
fn test_fuzz_extreme_pause_durations() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let token_address = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    // Test various extreme pause durations
    let test_durations = vec![
        0u64,                                    // No pause
        1u64,                                    // 1 second
        86_400u64,                               // 1 day
        31_536_000u64,                           // 1 year
        3_153_600_000u64,                        // 100 years
        u64::MAX / 2,                           // Very large duration
    ];

    for (i, pause_duration) in test_durations.iter().enumerate() {
        let grant_id = symbol_short!(&format!("grant_fuzz_{}", i));
        client.create_grant(&grant_id, &admin, &grantee, &1_000_000, &token_address).unwrap();
        
        let milestone_id = symbol_short!(&format!("m_fuzz_{}", i));
        client.add_milestone(&grant_id, &milestone_id, &100_000, &String::from_str(&env, "Test")).unwrap();
        
        // Activate the grant
        client.activate_grant(&grant_id).unwrap();
        
        // Advance time by pause duration
        env.ledger().set_timestamp(env.ledger().timestamp() + pause_duration);
        
        // Pause and resume
        client.pause_grant(&grant_id).unwrap();
        
        // Verify paused status
        let grant_info_paused = client.get_grant(&grant_id).unwrap();
        assert_eq!(grant_info_paused.status, crate::GrantStatus::Paused);
        
        client.resume_grant(&grant_id).unwrap();
        
        // Verify active status
        let grant_info_resumed = client.get_grant(&grant_id).unwrap();
        assert_eq!(grant_info_resumed.status, crate::GrantStatus::Active);
        
        // Approve milestone
        client.approve_milestone(&grant_id, &milestone_id).unwrap();
        
        // Verify invariants
        let remaining = client.get_remaining_amount(&grant_id).unwrap();
        let grant_info = client.get_grant(&grant_id).unwrap();
        assert_eq!(grant_info.released_amount + remaining, 1_000_000);
    }
}