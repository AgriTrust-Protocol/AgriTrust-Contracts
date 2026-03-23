#![cfg(test)]

use super::*;
use soroban_sdk::{Address, Symbol, Env, String, Vec};

#[test]
fn test_linear_vesting() {
    let env = Env::default();
    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    // Test linear vesting calculation
    let total = 1000000u128;
    let start = 1000u64;
    let duration = 1000u64;

    // At start: 0 vested
    let vested_at_start = grant::compute_claimable_balance(total, start, start, duration);
    assert_eq!(vested_at_start, 0);

    // Halfway: 50% vested
    let vested_half = grant::compute_claimable_balance(total, start, start + duration/2, duration);
    assert_eq!(vested_half, total / 2);

    // At end: 100% vested
    let vested_end = grant::compute_claimable_balance(total, start, start + duration, duration);
    assert_eq!(vested_end, total);
}

#[test]
fn test_exponential_vesting() {
    let env = Env::default();
    
    let total = 1000000u128;
    let start = 1000u64;
    let duration = 1000u64;
    let factor = 2000u32; // 2x factor

    // At start: 0 vested
    let vested_at_start = grant::compute_exponential_vesting(total, start, start, duration, factor);
    assert_eq!(vested_at_start, 0);

    // Should be less than linear at the beginning
    let vested_early = grant::compute_exponential_vesting(total, start, start + 100, duration, factor);
    let linear_early = grant::compute_claimable_balance(total, start, start + 100, duration);
    assert!(vested_early <= linear_early);

    // At end: 100% vested
    let vested_end = grant::compute_exponential_vesting(total, start, start + duration, duration, factor);
    assert_eq!(vested_end, total);
}

#[test]
fn test_logarithmic_vesting() {
    let env = Env::default();
    
    let total = 1000000u128;
    let start = 1000u64;
    let duration = 1000u64;
    let factor = 2000u32; // 2x factor

    // At start: 0 vested
    let vested_at_start = grant::compute_logarithmic_vesting(total, start, start, duration, factor);
    assert_eq!(vested_at_start, 0);

    // Should be more than linear at the beginning (front-loaded)
    let vested_early = grant::compute_logarithmic_vesting(total, start, start + 100, duration, factor);
    let linear_early = grant::compute_claimable_balance(total, start, start + 100, duration);
    assert!(vested_early >= linear_early);

    // At end: 100% vested
    let vested_end = grant::compute_logarithmic_vesting(total, start, start + duration, duration, factor);
    assert_eq!(vested_end, total);
}

#[test]
fn test_stream_pool_isolation() {
    let env = Env::default();
    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.set_admin(&admin);

    // Create stream pool
    let pool_id = Symbol::new(&env, "pool1");
    let asset = Asset {
        code: String::from_str(&env, "USDC"),
        issuer: None,
    };
    client.create_stream_pool(&pool_id, &asset, &10000);

    // Create isolated sub-pools
    let grant1 = Symbol::new(&env, "grant1");
    let grant2 = Symbol::new(&env, "grant2");
    
    client.create_isolated_sub_pool(&pool_id, &grant1, &5000);
    client.create_isolated_sub_pool(&pool_id, &grant2, &3000);

    // Check pool summary
    let (total_allocated, total_consumed, active_pools) = client.get_pool_summary(&pool_id);
    assert_eq!(total_allocated, 8000);
    assert_eq!(total_consumed, 0);
    assert_eq!(active_pools, 2);

    // Consume from sub-pool 1
    client.consume_from_sub_pool(&pool_id, &grant1, &1000);
    
    let (total_allocated, total_consumed, _) = client.get_pool_summary(&pool_id);
    assert_eq!(total_consumed, 1000);

    // Try to consume more than allocated - should fail
    let result = client.try_consume_from_sub_pool(&pool_id, &grant1, &5000);
    assert!(result.is_err());

    // Check sub-pool info
    let sub_pool_info = client.get_sub_pool_info(&pool_id, &grant1);
    assert_eq!(sub_pool_info.allocated_amount, 5000);
    assert_eq!(sub_pool_info.consumed_amount, 1000);
}

#[test]
fn test_auto_refill_functionality() {
    let env = Env::default();
    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.set_admin(&admin);

    // Configure treasury
    let reserve_wallet = Address::generate(&env);
    let treasury_config = TreasuryConfig {
        reserve_wallet,
        low_water_mark_days: 7,
        refill_amount: 1000,
        max_auto_refill: 5000,
        total_auto_refilled: 0,
        allowance_active: true,
    };
    client.configure_treasury(&treasury_config);

    // Create stream pool
    let pool_id = Symbol::new(&env, "pool1");
    let asset = Asset {
        code: String::from_str(&env, "USDC"),
        issuer: None,
    };
    client.create_stream_pool(&pool_id, &asset, &500);

    // Check and auto-refill (should trigger since balance is low)
    client.check_and_auto_refill(&pool_id);

    let updated_pool = client.get_stream_pool(&pool_id);
    assert!(updated_pool.balance > 500); // Should have been refilled

    let updated_treasury = client.get_treasury_config();
    assert!(updated_treasury.total_auto_refilled > 0);
}

#[test]
fn test_emergency_drain_multi_sig() {
    let env = Env::default();
    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.set_admin(&admin);

    // Setup multi-sig council
    let council_member1 = Address::generate(&env);
    let council_member2 = Address::generate(&env);
    let council_member3 = Address::generate(&env);
    let cold_storage = Address::generate(&env);

    let mut council = Vec::new(&env);
    council.push_back(council_member1.clone());
    council.push_back(council_member2.clone());
    council.push_back(council_member3.clone());

    let multi_sig_config = MultiSigConfig {
        council_members: council,
        required_signatures: 3, // 100% consensus
        cold_storage_vault: cold_storage,
        emergency_drain_active: true,
        drain_signatures: Map::new(&env),
        drain_proposal_expiry: 0,
    };
    client.configure_multi_sig(&multi_sig_config);

    // Create emergency drain proposal
    let proposal_id = Symbol::new(&env, "drain_proposal");
    let reason = String::from_str(&env, "Critical security vulnerability detected");
    
    // Only council member can create proposal
    client.create_emergency_drain_proposal(&proposal_id, &council_member1, &reason);

    // Get initial signature count
    let initial_count = client.get_signature_count();
    assert_eq!(initial_count, 0);

    // Sign the proposal
    client.sign_emergency_drain(&proposal_id, &council_member1);
    client.sign_emergency_drain(&proposal_id, &council_member2);
    
    let partial_count = client.get_signature_count();
    assert_eq!(partial_count, 2);

    // Final signature should trigger execution
    client.sign_emergency_drain(&proposal_id, &council_member3);

    let proposal = client.get_emergency_proposal(&proposal_id);
    assert!(proposal.executed);
}

#[test]
fn test_max_supply_enforcement() {
    let env = Env::default();
    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);

    // Create grant with max supply limit
    let grant_id = Symbol::new(&env, "grant_supply");
    let pool_id = Symbol::new(&env, "pool_supply");
    let asset = Asset {
        code: String::from_str(&env, "TOKEN"),
        issuer: None,
    };
    
    client.create_stream_pool(&pool_id, &asset, &5000);
    client.create_isolated_sub_pool(&pool_id, &grant_id, &5000);

    let vesting_curve = VestingCurve::Linear;
    client.create_grant(
        &grant_id,
        &admin,
        &grantee,
        &3000, // total_amount
        &1000,  // start_time
        &1000,  // duration
        &vesting_curve,
        &5000,  // max_supply
        &pool_id,
    );

    // Add milestone that would exceed max supply if released
    let milestone_id = Symbol::new(&env, "milestone_oversupply");
    client.add_milestone(&grant_id, &milestone_id, &6000, &String::from_str(&env, "Too much"));

    // Should fail to release due to max supply constraint
    let result = client.try_release_milestone(&grant_id, &milestone_id);
    assert!(result.is_err());
}

#[test]
fn test_multiple_milestones() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    // Create a grant
    let grant_id = Symbol::new(&env, "grant_multi");
    let pool_id = Symbol::new(&env, "pool_multi");
    let asset = Asset {
        code: String::from_str(&env, "USDC"),
        issuer: None,
    };
    
    client.create_stream_pool(&pool_id, &asset, &2000000);
    client.create_isolated_sub_pool(&pool_id, &grant_id, &1000000);

    let vesting_curve = VestingCurve::Linear;
    client.create_grant(
        &grant_id,
        &admin,
        &grantee,
        &1000000,
        &1000,
        &1000,
        &vesting_curve,
        &1000000,
        &pool_id,
    );

    // Add multiple milestones
    let milestone_1 = Symbol::new(&env, "m1");
    let milestone_2 = Symbol::new(&env, "m2");
    let milestone_3 = Symbol::new(&env, "m3");

    client.add_milestone(&grant_id, &milestone_1, &250_000, &String::from_str(&env, "Phase 1"));
    client.add_milestone(&grant_id, &milestone_2, &350_000, &String::from_str(&env, "Phase 2"));
    client.add_milestone(&grant_id, &milestone_3, &400_000, &String::from_str(&env, "Phase 3"));

    // Approve first milestone
    client.approve_milestone(&grant_id, &milestone_1);
    let grant_info = client.get_grant(&grant_id);
    assert_eq!(grant_info.released_amount, 250_000);

    // Approve second milestone
    client.approve_milestone(&grant_id, &milestone_2);
    client.release_milestone(&grant_id, &milestone_2);
    let grant_info = client.get_grant(&grant_id);
    assert_eq!(grant_info.released_amount, 600_000);

    // Approve third milestone
    client.approve_milestone(&grant_id, &milestone_3);
    client.release_milestone(&grant_id, &milestone_3);
    let grant_info = client.get_grant(&grant_id);
    assert_eq!(grant_info.released_amount, 1_000_000);
}

#[test]
fn test_double_release_prevention() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    // Create a grant and milestone
    let grant_id = Symbol::new(&env, "grant_double");
    let pool_id = Symbol::new(&env, "pool_double");
    let asset = Asset {
        code: String::from_str(&env, "USDC"),
        issuer: None,
    };
    
    client.create_stream_pool(&pool_id, &asset, &1000000);
    client.create_isolated_sub_pool(&pool_id, &grant_id, &1000000);

    let vesting_curve = VestingCurve::Linear;
    client.create_grant(
        &grant_id,
        &admin,
        &grantee,
        &1000000,
        &1000,
        &1000,
        &vesting_curve,
        &1000000,
        &pool_id,
    );

    let milestone_id = Symbol::new(&env, "milestone_double");
    client.add_milestone(
        &grant_id,
        &milestone_id,
        &500_000,
        &String::from_str(&env, "Test"),
    );

    // Approve once
    client.approve_milestone(&grant_id, &milestone_id);
    client.release_milestone(&grant_id, &milestone_id);

    // Try to release again - should fail
    let result = client.try_release_milestone(&grant_id, &milestone_id);
    assert!(result.is_err());
}

#[test]
fn test_get_remaining_amount() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    // Create a grant
    let grant_id = Symbol::new(&env, "grant_remaining");
    let pool_id = Symbol::new(&env, "pool_remaining");
    let asset = Asset {
        code: String::from_str(&env, "USDC"),
        issuer: None,
    };
    
    client.create_stream_pool(&pool_id, &asset, &2000000);
    client.create_isolated_sub_pool(&pool_id, &grant_id, &1000000);

    let vesting_curve = VestingCurve::Linear;
    client.create_grant(
        &grant_id,
        &admin,
        &grantee,
        &1000000,
        &1000,
        &1000,
        &vesting_curve,
        &1000000,
        &pool_id,
    );

    // Check remaining amount before any releases
    let remaining = client.get_remaining_amount(&grant_id);
    assert_eq!(remaining, 1000000);

    // Add and approve a milestone
    let milestone_id = Symbol::new(&env, "m1");
    client.add_milestone(&grant_id, &milestone_id, &400_000, &String::from_str(&env, "Phase 1"));
    client.approve_milestone(&grant_id, &milestone_id);
    client.release_milestone(&grant_id, &milestone_id);

    // Check remaining amount after release
    let remaining = client.get_remaining_amount(&grant_id);
    assert_eq!(remaining, 600_000);
}

#[test]
fn test_exceed_total_grant_amount() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    // Create a grant with 1M total
    let grant_id = Symbol::new(&env, "grant_exceed");
    let pool_id = Symbol::new(&env, "pool_exceed");
    let asset = Asset {
        code: String::from_str(&env, "USDC"),
        issuer: None,
    };
    
    client.create_stream_pool(&pool_id, &asset, &2000000);
    client.create_isolated_sub_pool(&pool_id, &grant_id, &1000000);

    let vesting_curve = VestingCurve::Linear;
    client.create_grant(
        &grant_id,
        &admin,
        &grantee,
        &1000000,
        &1000,
        &1000,
        &vesting_curve,
        &1000000,
        &pool_id,
    );

    // Add milestone for 600K
    let milestone_1 = Symbol::new(&env, "m1");
    client.add_milestone(&grant_id, &milestone_1, &600_000, &String::from_str(&env, "Phase 1"));
    client.approve_milestone(&grant_id, &milestone_1);
    client.release_milestone(&grant_id, &milestone_1);

    // Add milestone for 500K (would exceed total)
    let milestone_2 = Symbol::new(&env, "m2");
    client.add_milestone(&grant_id, &milestone_2, &500_000, &String::from_str(&env, "Phase 2"));

    // Trying to release should fail
    let result = client.try_release_milestone(&grant_id, &milestone_2);
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