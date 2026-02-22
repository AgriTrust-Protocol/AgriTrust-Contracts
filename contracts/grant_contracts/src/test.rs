#![cfg(test)]

use super::*;
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
    client.create_grant(&grant_id, &admin, &grantee, &1_000_000).unwrap();

    // Add multiple milestones
    let milestone_1 = Symbol::new(&env, "m1");
    let milestone_2 = Symbol::new(&env, "m2");
    let milestone_3 = Symbol::new(&env, "m3");

    client.add_milestone(&grant_id, &milestone_1, &250_000, &String::from_str(&env, "Phase 1")).unwrap();
    client.add_milestone(&grant_id, &milestone_2, &350_000, &String::from_str(&env, "Phase 2")).unwrap();
    client.add_milestone(&grant_id, &milestone_3, &400_000, &String::from_str(&env, "Phase 3")).unwrap();

    // Approve first milestone
    client.approve_milestone(&grant_id, &milestone_1).unwrap();
    let grant_info = client.get_grant(&grant_id).unwrap();
    assert_eq!(grant_info.3, 250_000);

    // Approve second milestone
    client.approve_milestone(&grant_id, &milestone_2).unwrap();
    let grant_info = client.get_grant(&grant_id).unwrap();
    assert_eq!(grant_info.3, 600_000);

    // Approve third milestone
    client.approve_milestone(&grant_id, &milestone_3).unwrap();
    let grant_info = client.get_grant(&grant_id).unwrap();
    assert_eq!(grant_info.3, 1_000_000);
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
    client.create_grant(&grant_id, &admin, &grantee, &1_000_000).unwrap();

    let milestone_id = Symbol::new(&env, "milestone_double");
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

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    // Create a grant
    let grant_id = Symbol::new(&env, "grant_remaining");
    client.create_grant(&grant_id, &admin, &grantee, &1_000_000).unwrap();

    // Check remaining amount before any releases
    let remaining = client.get_remaining_amount(&grant_id).unwrap();
    assert_eq!(remaining, 1_000_000);

    // Add and approve a milestone
    let milestone_id = Symbol::new(&env, "m1");
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

    let contract_id = env.register(GrantContract, ());
    let client = GrantContractClient::new(&env, &contract_id);

    // Create a grant with 1M total
    let grant_id = Symbol::new(&env, "grant_exceed");
    client.create_grant(&grant_id, &admin, &grantee, &1_000_000).unwrap();

    // Add milestone for 600K
    let milestone_1 = Symbol::new(&env, "m1");
    client.add_milestone(&grant_id, &milestone_1, &600_000, &String::from_str(&env, "Phase 1")).unwrap();
    client.approve_milestone(&grant_id, &milestone_1).unwrap();

    // Add milestone for 500K (would exceed total)
    let milestone_2 = Symbol::new(&env, "m2");
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