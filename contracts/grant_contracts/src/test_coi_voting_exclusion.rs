#![cfg(test)]

use soroban_sdk::{Env, Address, vec, String, Symbol};
use crate::grant_contracts::{
    GrantContract, Error, GranteeConfig,
    governance::{GovernanceContract, GovernanceError, ProposalStatus},
};

#[test]
fn test_coi_linked_address_management() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let linked_address = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);

    // Initialize grant contract
    GrantContract::initialize(
        env.clone(),
        admin.clone(),
        token.clone(),
        treasury.clone(),
        oracle.clone(),
        native_token.clone(),
    ).unwrap();

    // Create grant with linked addresses
    let grant_config = GranteeConfig {
        recipient: grantee.clone(),
        total_amount: 100000,
        flow_rate: 1000,
        asset: token.clone(),
        warmup_duration: 0,
        validator: None,
        linked_addresses: vec![&env, linked_address.clone()],
    };

    GrantContract::batch_init(
        env.clone(),
        vec![&env, grant_config.clone()],
        1, // starting_grant_id
    ).unwrap();

    // Test adding linked address
    let result = GrantContract::add_linked_address(
        env.clone(),
        admin.clone(),
        1, // grant_id
        linked_address.clone(),
    );
    assert_eq!(result, Ok(()));

    // Test getting linked addresses
    let linked_addresses = GrantContract::get_linked_addresses(env.clone(), 1).unwrap();
    assert_eq!(linked_addresses.len(), 1);
    assert_eq!(linked_addresses.get(0).unwrap(), linked_address);

    // Test removing linked address
    let result = GrantContract::remove_linked_address(
        env.clone(),
        admin.clone(),
        1, // grant_id
        linked_address.clone(),
    );
    assert_eq!(result, Ok(()));

    // Verify removal
    let linked_addresses_after = GrantContract::get_linked_addresses(env.clone(), 1).unwrap();
    assert_eq!(linked_addresses_after.len(), 0);
}

#[test]
fn test_coi_voter_conflict_detection() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let linked_address = Address::generate(&env);
    let voter = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let gov_token = Address::generate(&env);
    let stake_token = Address::generate(&env);

    // Initialize contracts
    GrantContract::initialize(
        env.clone(),
        admin.clone(),
        token.clone(),
        treasury.clone(),
        oracle.clone(),
        native_token.clone(),
    ).unwrap();

    GovernanceContract::initialize(
        env.clone(),
        gov_token.clone(),
        1000, // voting_threshold
        500,  // quorum_threshold
        stake_token.clone(),
        100,   // proposal_stake_amount
    ).unwrap();

    // Set up council members
    let council_members = vec![&env, admin.clone()];
    GovernanceContract::set_council_members(env.clone(), admin.clone(), council_members).unwrap();

    // Create grant with linked addresses
    let grant_config = GranteeConfig {
        recipient: grantee.clone(),
        total_amount: 100000,
        flow_rate: 1000,
        asset: token.clone(),
        warmup_duration: 0,
        validator: None,
        linked_addresses: vec![&env, linked_address.clone()],
    };

    GrantContract::batch_init(
        env.clone(),
        vec![&env, grant_config.clone()],
        1, // starting_grant_id
    ).unwrap();

    // Create atomic grant proposal
    let proposal_id = GovernanceContract::propose_atomic_grant(
        env.clone(),
        admin.clone(),
        String::from_str(&env, "Test Grant"),
        String::from_str(&env, "Testing COI functionality"),
        604800, // 7 days voting period
        vec![&env, grant_config.clone()],
        100000, // total_grant_amount
    ).unwrap();

    // Test 1: Grantee trying to vote (should fail)
    let result = GovernanceContract::quadratic_vote(
        env.clone(),
        grantee.clone(),
        proposal_id,
        1, // weight
    );
    assert_eq!(result, Err(GovernanceError::VoterHasConflictOfInterest));

    // Test 2: Linked address trying to vote (should fail)
    let result = GovernanceContract::quadratic_vote(
        env.clone(),
        linked_address.clone(),
        proposal_id,
        1, // weight
    );
    assert_eq!(result, Err(GovernanceError::VoterHasConflictOfInterest));

    // Test 3: Unrelated address can vote (should succeed)
    let neutral_voter = Address::generate(&env);
    let result = GovernanceContract::quadratic_vote(
        env.clone(),
        neutral_voter.clone(),
        proposal_id,
        1, // weight
    );
    assert_eq!(result, Ok(()));

    // Test 4: Check voter conflict directly
    let has_conflict = GrantContract::check_voter_conflict(
        env.clone(),
        grantee.clone(),
        1, // grant_id
    ).unwrap();
    assert_eq!(has_conflict, true); // Grantee has conflict

    let has_conflict = GrantContract::check_voter_conflict(
        env.clone(),
        neutral_voter.clone(),
        1, // grant_id
    ).unwrap();
    assert_eq!(has_conflict, false); // Neutral voter has no conflict
}

#[test]
fn test_coi_multiple_linked_addresses() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let linked_addr1 = Address::generate(&env);
    let linked_addr2 = Address::generate(&env);
    let linked_addr3 = Address::generate(&env);
    let voter = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let gov_token = Address::generate(&env);
    let stake_token = Address::generate(&env);

    // Initialize contracts
    GrantContract::initialize(
        env.clone(),
        admin.clone(),
        token.clone(),
        treasury.clone(),
        oracle.clone(),
        native_token.clone(),
    ).unwrap();

    GovernanceContract::initialize(
        env.clone(),
        gov_token.clone(),
        1000, // voting_threshold
        500,  // quorum_threshold
        stake_token.clone(),
        100,   // proposal_stake_amount
    ).unwrap();

    // Set up council members
    let council_members = vec![&env, admin.clone()];
    GovernanceContract::set_council_members(env.clone(), admin.clone(), council_members).unwrap();

    // Create grant with multiple linked addresses
    let grant_config = GranteeConfig {
        recipient: grantee.clone(),
        total_amount: 100000,
        flow_rate: 1000,
        asset: token.clone(),
        warmup_duration: 0,
        validator: None,
        linked_addresses: vec![
            &env, 
            linked_addr1.clone(),
            linked_addr2.clone(),
            linked_addr3.clone(),
        ],
    };

    GrantContract::batch_init(
        env.clone(),
        vec![&env, grant_config.clone()],
        1, // starting_grant_id
    ).unwrap();

    // Create atomic grant proposal
    let proposal_id = GovernanceContract::propose_atomic_grant(
        env.clone(),
        admin.clone(),
        String::from_str(&env, "Multi-Linked Test"),
        String::from_str(&env, "Testing multiple linked addresses"),
        604800, // 7 days voting period
        vec![&env, grant_config.clone()],
        100000, // total_grant_amount
    ).unwrap();

    // Test: All linked addresses should be blocked from voting
    let result1 = GovernanceContract::quadratic_vote(
        env.clone(),
        linked_addr1.clone(),
        proposal_id,
        1, // weight
    );
    assert_eq!(result1, Err(GovernanceError::VoterHasConflictOfInterest));

    let result2 = GovernanceContract::quadratic_vote(
        env.clone(),
        linked_addr2.clone(),
        proposal_id,
        1, // weight
    );
    assert_eq!(result2, Err(GovernanceError::VoterHasConflictOfInterest));

    let result3 = GovernanceContract::quadratic_vote(
        env.clone(),
        linked_addr3.clone(),
        proposal_id,
        1, // weight
    );
    assert_eq!(result3, Err(GovernanceError::VoterHasConflictOfInterest));
}

#[test]
fn test_coi_linked_address_management_errors() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    let linked_address = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);

    // Initialize grant contract
    GrantContract::initialize(
        env.clone(),
        admin.clone(),
        token.clone(),
        treasury.clone(),
        oracle.clone(),
        native_token.clone(),
    ).unwrap();

    // Create grant
    let grant_config = GranteeConfig {
        recipient: Address::generate(&env),
        total_amount: 100000,
        flow_rate: 1000,
        asset: token.clone(),
        warmup_duration: 0,
        validator: None,
        linked_addresses: vec![&env],
    };

    GrantContract::batch_init(
        env.clone(),
        vec![&env, grant_config.clone()],
        1, // starting_grant_id
    ).unwrap();

    // Test 1: Unauthorized user trying to add linked address (should fail)
    let result = GrantContract::add_linked_address(
        env.clone(),
        unauthorized.clone(),
        1, // grant_id
        linked_address.clone(),
    );
    assert_eq!(result, Err(Error::NotAuthorized));

    // Test 2: Adding duplicate linked address (should fail)
    GrantContract::add_linked_address(
        env.clone(),
        admin.clone(),
        1, // grant_id
        linked_address.clone(),
    ).unwrap();

    let result = GrantContract::add_linked_address(
        env.clone(),
        admin.clone(),
        1, // grant_id
        linked_address.clone(),
    );
    assert_eq!(result, Err(Error::LinkedAddressAlreadyExists));

    // Test 3: Removing non-existent linked address (should fail)
    let non_existent = Address::generate(&env);
    let result = GrantContract::remove_linked_address(
        env.clone(),
        admin.clone(),
        1, // grant_id
        non_existent.clone(),
    );
    assert_eq!(result, Err(Error::LinkedAddressNotFound));

    // Test 4: Getting linked addresses for non-existent grant (should fail)
    let result = GrantContract::get_linked_addresses(
        env.clone(),
        999, // non-existent grant_id
    );
    assert_eq!(result, Err(Error::GrantNotFound));
}

#[test]
fn test_coi_regular_proposal_no_check() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let voter = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let gov_token = Address::generate(&env);
    let stake_token = Address::generate(&env);

    // Initialize contracts
    GrantContract::initialize(
        env.clone(),
        admin.clone(),
        token.clone(),
        treasury.clone(),
        oracle.clone(),
        native_token.clone(),
    ).unwrap();

    GovernanceContract::initialize(
        env.clone(),
        gov_token.clone(),
        1000, // voting_threshold
        500,  // quorum_threshold
        stake_token.clone(),
        100,   // proposal_stake_amount
    ).unwrap();

    // Set up council members
    let council_members = vec![&env, admin.clone()];
    GovernanceContract::set_council_members(env.clone(), admin.clone(), council_members).unwrap();

    // Create regular proposal (no grant payload - no COI check)
    let proposal_id = GovernanceContract::create_proposal(
        env.clone(),
        admin.clone(),
        String::from_str(&env, "Regular Proposal"),
        String::from_str(&env, "No grant payload"),
        604800, // 7 days voting period
    ).unwrap();

    // Test: Grantee should be able to vote on regular proposal
    let result = GovernanceContract::quadratic_vote(
        env.clone(),
        grantee.clone(),
        proposal_id,
        1, // weight
    );
    assert_eq!(result, Ok(())); // Should succeed - no COI check for regular proposals
}

#[test]
fn test_coi_grant_creation_with_linked_addresses() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let linked_addr1 = Address::generate(&env);
    let linked_addr2 = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);

    // Initialize grant contract
    GrantContract::initialize(
        env.clone(),
        admin.clone(),
        token.clone(),
        treasury.clone(),
        oracle.clone(),
        native_token.clone(),
    ).unwrap();

    // Create grant with multiple linked addresses
    let grant_config = GranteeConfig {
        recipient: grantee.clone(),
        total_amount: 100000,
        flow_rate: 1000,
        asset: token.clone(),
        warmup_duration: 0,
        validator: None,
        linked_addresses: vec![&env, linked_addr1.clone(), linked_addr2.clone()],
    };

    let result = GrantContract::batch_init(
        env.clone(),
        vec![&env, grant_config.clone()],
        1, // starting_grant_id
    );

    assert!(result.is_ok());

    // Verify linked addresses were stored
    let stored_linked = GrantContract::get_linked_addresses(env.clone(), 1).unwrap();
    assert_eq!(stored_linked.len(), 2);
    
    // Verify both addresses are in the list
    let mut found_addr1 = false;
    let mut found_addr2 = false;
    for addr in stored_linked.iter() {
        if *addr == linked_addr1 {
            found_addr1 = true;
        }
        if *addr == linked_addr2 {
            found_addr2 = true;
        }
    }
    
    assert!(found_addr1);
    assert!(found_addr2);
}

#[test]
fn test_coi_comprehensive_voting_scenario() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee1 = Address::generate(&env);
    let grantee2 = Address::generate(&env);
    let linked_addr1 = Address::generate(&env);
    let linked_addr2 = Address::generate(&env);
    let neutral_voter1 = Address::generate(&env);
    let neutral_voter2 = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);
    let gov_token = Address::generate(&env);
    let stake_token = Address::generate(&env);

    // Initialize contracts
    GrantContract::initialize(
        env.clone(),
        admin.clone(),
        token.clone(),
        treasury.clone(),
        oracle.clone(),
        native_token.clone(),
    ).unwrap();

    GovernanceContract::initialize(
        env.clone(),
        gov_token.clone(),
        1000, // voting_threshold
        500,  // quorum_threshold
        stake_token.clone(),
        100,   // proposal_stake_amount
    ).unwrap();

    // Set up council members
    let council_members = vec![&env, admin.clone()];
    GovernanceContract::set_council_members(env.clone(), admin.clone(), council_members).unwrap();

    // Create two grants with different linked addresses
    let grant_config1 = GranteeConfig {
        recipient: grantee1.clone(),
        total_amount: 100000,
        flow_rate: 1000,
        asset: token.clone(),
        warmup_duration: 0,
        validator: None,
        linked_addresses: vec![&env, linked_addr1.clone()],
    };

    let grant_config2 = GranteeConfig {
        recipient: grantee2.clone(),
        total_amount: 100000,
        flow_rate: 1000,
        asset: token.clone(),
        warmup_duration: 0,
        validator: None,
        linked_addresses: vec![&env, linked_addr2.clone()],
    };

    GrantContract::batch_init(
        env.clone(),
        vec![&env, grant_config1.clone(), grant_config2.clone()],
        1, // starting_grant_id
    ).unwrap();

    // Create atomic grant proposal for both grants
    let proposal_id = GovernanceContract::propose_atomic_grant(
        env.clone(),
        admin.clone(),
        String::from_str(&env, "Multi-Grant Test"),
        String::from_str(&env, "Testing COI with multiple grants"),
        604800, // 7 days voting period
        vec![&env, grant_config1.clone(), grant_config2.clone()],
        200000, // total_grant_amount
    ).unwrap();

    // Test voting scenarios:
    
    // 1. Grantee1 should not be able to vote (conflict with own grant)
    let result = GovernanceContract::quadratic_vote(
        env.clone(),
        grantee1.clone(),
        proposal_id,
        1, // weight
    );
    assert_eq!(result, Err(GovernanceError::VoterHasConflictOfInterest));

    // 2. Linked address 1 should not be able to vote (conflict with grantee1)
    let result = GovernanceContract::quadratic_vote(
        env.clone(),
        linked_addr1.clone(),
        proposal_id,
        1, // weight
    );
    assert_eq!(result, Err(GovernanceError::VoterHasConflictOfInterest));

    // 3. Grantee2 should not be able to vote (conflict with own grant)
    let result = GovernanceContract::quadratic_vote(
        env.clone(),
        grantee2.clone(),
        proposal_id,
        1, // weight
    );
    assert_eq!(result, Err(GovernanceError::VoterHasConflictOfInterest));

    // 4. Linked address 2 should not be able to vote (conflict with grantee2)
    let result = GovernanceContract::quadratic_vote(
        env.clone(),
        linked_addr2.clone(),
        proposal_id,
        1, // weight
    );
    assert_eq!(result, Err(GovernanceError::VoterHasConflictOfInterest));

    // 5. Neutral voters should be able to vote
    let result1 = GovernanceContract::quadratic_vote(
        env.clone(),
        neutral_voter1.clone(),
        proposal_id,
        1, // weight
    );
    assert_eq!(result1, Ok(()));

    let result2 = GovernanceContract::quadratic_vote(
        env.clone(),
        neutral_voter2.clone(),
        proposal_id,
        1, // weight
    );
    assert_eq!(result2, Ok(()));

    // 6. Verify conflict checking function works correctly
    let conflict1 = GrantContract::check_voter_conflict(
        env.clone(),
        grantee1.clone(),
        1, // grant_id
    ).unwrap();
    assert_eq!(conflict1, true);

    let conflict2 = GrantContract::check_voter_conflict(
        env.clone(),
        linked_addr1.clone(),
        1, // grant_id
    ).unwrap();
    assert_eq!(conflict2, true);

    let no_conflict = GrantContract::check_voter_conflict(
        env.clone(),
        neutral_voter1.clone(),
        1, // grant_id
    ).unwrap();
    assert_eq!(no_conflict, false);
}
