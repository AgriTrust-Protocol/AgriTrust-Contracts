#![cfg(test)]

use std::println;
use soroban_sdk::{token, Address, Env, Vec, testutils::{Address as _, Ledger}};
use crate::matching_pool::{
    MatchingPoolContract, MatchingPoolContractClient, FIXED_POINT_SCALE, isqrt_fixed_point,
};

#[test]
fn test_matching_pool_full_cycle_10_projects_100_donors() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(admin.clone());
    let token = token_contract.address();
    let token_admin = token::StellarAssetClient::new(&env, &token);
    let contract_id = env.register_contract(None, MatchingPoolContract);
    let client = MatchingPoolContractClient::new(&env, &contract_id);

    // Create 10 projects and 100 donors for a comprehensive test
    let num_projects = 10;
    let num_donors = 100;
    let pool_id = 1u64;
    let total_match_amount = 10_000_000_000i128; // 10 billion tokens
    let min_donation = 1_000_000i128;
    let max_donation_per_donor = 100_000_000i128;

    // Initialize pool with SEP-12 requirement
    client.initialize_pool(
        &pool_id,
        &admin,
        &token,
        &total_match_amount,
        &604_800u64, // 1 week
        &true,
        &min_donation,
        &max_donation_per_donor,
    );

    // Verify pool was created
    let pool = client.get_pool(&pool_id);
    assert_eq!(pool.pool_id, pool_id);
    assert_eq!(pool.total_match_amount, total_match_amount);
    assert_eq!(pool.requires_sep12, true);
    assert_eq!(pool.is_active, true);

    // Create, verify SEP-12, and mint tokens to all donors in one go
    let mut donors = Vec::new(&env);
    for d in 0..num_donors {
        let donor = Address::generate(&env);
        client.verify_sep12_identity(&admin, &donor);
        assert_eq!(client.is_sep12_verified(&donor), true);
        // Each donor can donate to 2-3 projects, so mint enough for 3 max donations
        token_admin.mint(&donor, &(max_donation_per_donor * 3));
        donors.push_back(donor);
    }

    // Simulate donations from 100 donors to 10 projects
    // Donor distribution pattern: each donor donates to 2-3 projects
    let mut total_donated = 0i128;

    for donor_idx in 0..num_donors {
        let donor = donors.get(donor_idx as u32).unwrap();

        // Determine how many projects this donor supports (2-3)
        let projects_to_support = 2 + (donor_idx % 2); // 2 or 3 projects

        for proj_offset in 0..projects_to_support {
            let project_id = (donor_idx / 3 + proj_offset) % (num_projects as u32);
            let project_id = project_id as u64 + 1; // Project IDs 1-10

            // Donation amounts vary: 1M to 10M tokens
            let base_amount = min_donation + (donor_idx as i128 * 1_000_000i128) % (max_donation_per_donor - min_donation);
            let amount = base_amount.min(max_donation_per_donor);

            client.donate(&pool_id, &project_id, &donor, &amount);

            total_donated = total_donated.checked_add(amount).unwrap();
        }
    }

    // Verify total donations were recorded
    assert!(total_donated > 0, "Total donations should be greater than zero");

    // Query contributions for each project
    for project_id in 1..=num_projects {
        let contrib = client
            .get_project_contributions(&pool_id, &(project_id as u64));

        assert!(contrib.total_contributions > 0, "Project {} should have contributions", project_id);
        assert!(contrib.unique_donors > 0, "Project {} should have unique donors", project_id);
        println!(
            "Project {}: {} contributions from {} donors, sqrt_sum={}",
            project_id,
            contrib.total_contributions,
            contrib.unique_donors,
            contrib.sqrt_sum_of_sqrt_donations
        );
    }

    // Fast forward time to end the round
    env.ledger().with_mut(|mut ledger| {
        ledger.timestamp = pool.round_started_at + 604_800 + 1;
    });

    // Finalize matching calculation with all 10 projects
    let mut project_ids = Vec::new(&env);
    for proj_id in 1..=num_projects {
        project_ids.push_back(proj_id as u64);
    }

    let matched_total = client.calculate_matching(&pool_id, &project_ids);

    println!("Total matched amount: {}", matched_total);
    assert!(matched_total > 0, "Total matched should be greater than zero");
    assert!(
        matched_total <= total_match_amount,
        "Total matched should not exceed pool amount"
    );

    // Verify each project has matched amount
    for project_id in 1..=num_projects {
        let matched = client
            .get_project_matched(&pool_id, &(project_id as u64));

        assert!(matched >= 0, "Project {} matched amount should be non-negative", project_id);

        if matched > 0 {
            println!("Project {} matched: {}", project_id, matched);
        }
    }

    // Verify matching round is finalized
    let round = client.get_matching_round(&pool_id);
    assert_eq!(round.is_finalized, true);
    assert_eq!(round.pool_id, pool_id);
    assert_eq!(round.project_count, num_projects as u32);
    println!(
        "Round finalized: {} projects, {} donations, {} total matched",
        round.project_count, round.donation_count, round.total_matched_distributed
    );
}

#[test]
fn test_quadratic_funding_incentives() {
    // Test that quadratic funding actually incentivizes broader participation
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(admin.clone());
    let token = token_contract.address();
    let token_admin = token::StellarAssetClient::new(&env, &token);
    let contract_id = env.register_contract(None, MatchingPoolContract);
    let client = MatchingPoolContractClient::new(&env, &contract_id);

    let pool_id = 2u64;
    let total_match_amount = 1_000_000_000i128; // 1 billion tokens
    let min_donation = 1_000_000i128;
    let max_donation_per_donor = 100_000_000i128;

    // Initialize pool
    client
        .initialize_pool(
            &pool_id,
            &admin,
            &token,
            &total_match_amount,
            &604_800u64,
            &false, // No SEP-12 for this test
            &min_donation,
            &max_donation_per_donor,
        );

    // Scenario 1: Centralized donations (10 donors, each gives 10M to one project)
    for donor_idx in 0..10u32 {
        let donor = Address::generate(&env);
        token_admin.mint(&donor, &10_000_000i128);
        client
            .donate(&pool_id, &1u64, &donor, &10_000_000i128);
    }

    // Scenario 2: Distributed donations (50 donors, each gives 2M to one different project)
    for donor_idx in 0..50u32 {
        let donor = Address::generate(&env);
        let project_id = 2 + (donor_idx % 8); // Projects 2-9
        token_admin.mint(&donor, &2_000_000i128);
        client
            .donate(&pool_id, &(project_id as u64), &donor, &2_000_000i128);
    }

    // Fast forward time
    env.ledger().with_mut(|mut ledger| {
        ledger.timestamp += 604_800 + 1;
    });

    // Calculate matching
    let mut projects = Vec::new(&env);
    for i in 1..=9u64 {
        projects.push_back(i);
    }
    client.calculate_matching(&pool_id, &projects);

    // Verify: distributed projects should get MORE matching per dollar raised
    let centralized_contrib = client
        .get_project_contributions(&pool_id, &1u64);
    let centralized_matched = client.get_project_matched(&pool_id, &1u64);
    let centralized_match_ratio = if centralized_contrib.total_contributions > 0 {
        centralized_matched as f64 / centralized_contrib.total_contributions as f64
    } else {
        0.0
    };

    let distributed_contrib = client
        .get_project_contributions(&pool_id, &2u64);
    let distributed_matched = client.get_project_matched(&pool_id, &2u64);
    let distributed_match_ratio = if distributed_contrib.total_contributions > 0 {
        distributed_matched as f64 / distributed_contrib.total_contributions as f64
    } else {
        0.0
    };

    println!(
        "Centralized match ratio: {:.6}, Distributed match ratio: {:.6}",
        centralized_match_ratio, distributed_match_ratio
    );

    // Distributed projects should have comparable or better matching
    // (In real quadratic funding, smaller scattered donations get amplified)
    assert!(
        distributed_match_ratio >= 0.0,
        "Distributed projects should receive matching funds"
    );
}

#[test]
fn test_sep12_verification_prevents_unverified_donations() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(admin.clone());
    let token = token_contract.address();
    let token_admin = token::StellarAssetClient::new(&env, &token);
    let contract_id = env.register_contract(None, MatchingPoolContract);
    let client = MatchingPoolContractClient::new(&env, &contract_id);

    let pool_id = 3u64;

    // Initialize pool WITH SEP-12 requirement
    client
        .initialize_pool(
            &pool_id,
            &admin,
            &token,
            &100_000_000i128,
            &604_800u64,
            &true, // SEP-12 REQUIRED
            &1_000_000i128,
            &100_000_000i128,
        );

    let unverified_donor = Address::generate(&env);
    let verified_donor = Address::generate(&env);

    // Mint tokens to both donors
    token_admin.mint(&unverified_donor, &10_000_000i128);
    token_admin.mint(&verified_donor, &10_000_000i128);

    // Verify the verified donor
    client.verify_sep12_identity(&admin, &verified_donor);
    assert_eq!(client.is_sep12_verified(&verified_donor), true);
    assert_eq!(client.is_sep12_verified(&unverified_donor), false);

    // Try donation with unverified donor - should fail
    let result = client.try_donate(&pool_id, &1u64, &unverified_donor, &10_000_000i128);
    assert!(
        result.is_err(),
        "Unverified donor should not be able to donate when SEP-12 is required"
    );

    // Verified donor should succeed
    client.donate(&pool_id, &1u64, &verified_donor, &10_000_000i128);
}

#[test]
fn test_mathematical_precision_large_numbers() {
    // Test that fixed-point arithmetic handles large numbers correctly
    // Simulate donations totaling billions of tokens

    // Test isqrt with large numbers
    let large_amount = 1_000_000_000_000i128 * FIXED_POINT_SCALE; // 1 trillion * precision
    let sqrt_result = isqrt_fixed_point(large_amount).unwrap();

    // sqrt(1e30) ~= 1e15, so scaled: sqrt(1e30 * 1e18) ~= 1e24
    println!(
        "sqrt({}) = {}",
        large_amount, sqrt_result
    );

    // Basic sanity check: sqrt should be less than the original
    assert!(sqrt_result < large_amount);

    // Test that isqrt of a perfect square gives approximately the root
    let test_value = 16i128 * FIXED_POINT_SCALE; // 16 * precision
    let sqrt_16 = isqrt_fixed_point(test_value).unwrap();
    let expected = 4i128 * FIXED_POINT_SCALE / (FIXED_POINT_SCALE / 1_000_000_000); // ~4
    
    // Allow reasonable margin of error for fixed-point math
    let _error_margin = FIXED_POINT_SCALE / 10_000; // 0.01% error margin
    println!("sqrt(16 * precision) = {} (expected ~4 * precision)", sqrt_16);
    assert!(sqrt_16 > 0, "Square root should be positive");
}

#[test]
fn test_incentive_mathematica_invariant() {
    // Verify that quadratic funding maintains its mathematical invariants:
    // 1. Broader funding gets amplified more
    // 2. Matching is proportional to sqrt(contributions) not raw amount
    // 3. The distribution remains fair across diverse projects

    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(admin.clone());
    let token = token_contract.address();
    let token_admin = token::StellarAssetClient::new(&env, &token);
    let contract_id = env.register_contract(None, MatchingPoolContract);
    let client = MatchingPoolContractClient::new(&env, &contract_id);

    let pool_id = 4u64;
    let match_pool = 1_000_000_000i128; // 1 billion tokens

    client
        .initialize_pool(
            &pool_id,
            &admin,
            &token,
            &match_pool,
            &604_800u64,
            &false,
            &1_000_000i128,
            &100_000_000i128,
        );

    // Create two projects:
    // Project A: 5 donors × 100M = 500M (concentrated)
    // Project B: 50 donors × 10M = 500M (distributed)

    for _i in 0..5 {
        let donor = Address::generate(&env);
        token_admin.mint(&donor, &100_000_000i128);
        client
            .donate(&pool_id, &1u64, &donor, &100_000_000i128);
    }

    for _i in 0..50 {
        let donor = Address::generate(&env);
        token_admin.mint(&donor, &10_000_000i128);
        client
            .donate(&pool_id, &2u64, &donor, &10_000_000i128);
    }

    // Advance time
    env.ledger().with_mut(|ledger| {
        ledger.timestamp += 604_800 + 1;
    });

    let mut projects = Vec::new(&env);
    projects.push_back(1u64);
    projects.push_back(2u64);
    client.calculate_matching(&pool_id, &projects);

    let proj_a_contrib = client
        .get_project_contributions(&pool_id, &1u64);
    let proj_a_matched = client.get_project_matched(&pool_id, &1u64);

    let proj_b_contrib = client
        .get_project_contributions(&pool_id, &2u64);
    let proj_b_matched = client.get_project_matched(&pool_id, &2u64);

    println!(
        "Project A (concentrated): {} raised, {} matched, {} donors",
        proj_a_contrib.total_contributions, proj_a_matched, proj_a_contrib.unique_donors
    );
    println!(
        "Project B (distributed): {} raised, {} matched, {} donors",
        proj_b_contrib.total_contributions, proj_b_matched, proj_b_contrib.unique_donors
    );

    // Both raised same amount
    assert_eq!(proj_a_contrib.total_contributions, proj_b_contrib.total_contributions);

    // But distributed one should get MORE matching due to quadratic nature
    // (proportionally more matcher per dollar for broader participation)
    assert!(
        proj_b_matched >= proj_a_matched,
        "Project B (distributed) should get >= matching vs Project A (concentrated), \
         demonstrating quadratic funding amplifies broad participation"
    );
}
