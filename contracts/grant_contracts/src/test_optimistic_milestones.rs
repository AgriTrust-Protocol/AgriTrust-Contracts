#![cfg(test)]

use soroban_sdk::{Env, Address, vec, String, Symbol};
use crate::grant_contracts::{
    GrantContract, Error, GranteeConfig, GrantStatus, MilestoneStatus, ChallengeStatus,
    CHALLENGE_PERIOD, MAX_MILESTONE_REASON_LENGTH, MAX_CHALLENGE_REASON_LENGTH, MAX_EVIDENCE_LENGTH,
};

#[test]
fn test_milestone_claim_creation() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
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

    // Create grant with milestone configuration
    let grant_config = GranteeConfig {
        recipient: grantee.clone(),
        total_amount: 100000,
        flow_rate: 1000,
        asset: token.clone(),
        warmup_duration: 0,
        validator: None,
        linked_addresses: vec![&env],
        milestone_amount: 25000, // 4 milestones of 25,000 each
        total_milestones: 4,
    };

    GrantContract::batch_init(
        env.clone(),
        vec![&env, grant_config.clone()],
        1, // starting_grant_id
    ).unwrap();

    // Test successful milestone claim
    let claim_id = GrantContract::claim_milestone(
        env.clone(),
        1, // grant_id
        1, // milestone_number
        String::from_str(&env, "Completed MVP development"),
        String::from_str(&env, "GitHub repo: https://github.com/project/mvp"),
    ).unwrap();

    assert!(claim_id > 0);

    // Verify claim details
    let claim = GrantContract::get_milestone_claim(env.clone(), claim_id).unwrap();
    assert_eq!(claim.grant_id, 1);
    assert_eq!(claim.milestone_number, 1);
    assert_eq!(claim.amount, 25000);
    assert_eq!(claim.status, MilestoneStatus::Claimed);
    assert!(claim.challenger.is_none());
    assert!(claim.challenge_reason.is_none());
}

#[test]
fn test_milestone_claim_validation() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
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

    // Create grant with milestone configuration
    let grant_config = GranteeConfig {
        recipient: grantee.clone(),
        total_amount: 100000,
        flow_rate: 1000,
        asset: token.clone(),
        warmup_duration: 0,
        validator: None,
        linked_addresses: vec![&env],
        milestone_amount: 25000,
        total_milestones: 4,
    };

    GrantContract::batch_init(
        env.clone(),
        vec![&env, grant_config.clone()],
        1, // starting_grant_id
    ).unwrap();

    // Test 1: Invalid milestone number (0)
    let result = GrantContract::claim_milestone(
        env.clone(),
        1, // grant_id
        0, // invalid milestone_number
        String::from_str(&env, "Invalid milestone"),
        String::from_str(&env, "Evidence"),
    );
    assert_eq!(result, Err(Error::InvalidMilestoneNumber));

    // Test 2: Milestone already claimed
    let claim_id = GrantContract::claim_milestone(
        env.clone(),
        1, // grant_id
        1, // milestone_number
        String::from_str(&env, "First milestone"),
        String::from_str(&env, "Evidence"),
    ).unwrap();

    // Try to claim same milestone again
    let result = GrantContract::claim_milestone(
        env.clone(),
        1, // grant_id
        1, // already claimed
        String::from_str(&env, "Duplicate claim"),
        String::from_str(&env, "Evidence"),
    );
    assert_eq!(result, Err(Error::MilestoneAlreadyClaimed));

    // Test 3: Reason too long
    let long_reason = String::from_str(&env, &"a".repeat(MAX_MILESTONE_REASON_LENGTH as usize + 1));
    let result = GrantContract::claim_milestone(
        env.clone(),
        1, // grant_id
        2, // valid milestone_number
        long_reason,
        String::from_str(&env, "Evidence"),
    );
    assert_eq!(result, Err(Error::InvalidReasonLength));

    // Test 4: Evidence too long
    let long_evidence = String::from_str(&env, &"a".repeat(MAX_EVIDENCE_LENGTH as usize + 1));
    let result = GrantContract::claim_milestone(
        env.clone(),
        1, // grant_id
        2, // valid milestone_number
        String::from_str(&env, "Valid reason"),
        long_evidence,
    );
    assert_eq!(result, Err(Error::InvalidReasonLength));
}

#[test]
fn test_milestone_challenge_creation() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let challenger = Address::generate(&env);
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

    // Create grant and claim milestone
    let grant_config = GranteeConfig {
        recipient: grantee.clone(),
        total_amount: 100000,
        flow_rate: 1000,
        asset: token.clone(),
        warmup_duration: 0,
        validator: None,
        linked_addresses: vec![&env],
        milestone_amount: 25000,
        total_milestones: 4,
    };

    GrantContract::batch_init(
        env.clone(),
        vec![&env, grant_config.clone()],
        1, // starting_grant_id
    ).unwrap();

    let claim_id = GrantContract::claim_milestone(
        env.clone(),
        1, // grant_id
        1, // milestone_number
        String::from_str(&env, "Milestone to challenge"),
        String::from_str(&env, "Evidence of completion"),
    ).unwrap();

    // Test successful challenge creation
    let challenge_id = GrantContract::challenge_milestone(
        env.clone(),
        challenger.clone(),
        claim_id,
        String::from_str(&env, "Milestone not actually completed"),
        String::from_str(&env, "Evidence of incomplete work"),
    ).unwrap();

    assert!(challenge_id > 0);

    // Verify challenge details
    let challenge = GrantContract::get_milestone_challenge(env.clone(), challenge_id).unwrap();
    assert_eq!(challenge.claim_id, claim_id);
    assert_eq!(challenge.challenger, challenger);
    assert_eq!(challenge.status, ChallengeStatus::Active);
    assert!(challenge.resolved_at.is_none());
    assert!(challenge.resolution.is_none());

    // Verify claim status updated
    let claim = GrantContract::get_milestone_claim(env.clone(), claim_id).unwrap();
    assert_eq!(claim.status, MilestoneStatus::Challenged);
    assert_eq!(claim.challenger, Some(challenger));
    assert_eq!(claim.challenge_reason, Some(String::from_str(&env, "Milestone not actually completed")));
}

#[test]
fn test_milestone_challenge_validation() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let challenger = Address::generate(&env);
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

    // Create grant and claim milestone
    let grant_config = GranteeConfig {
        recipient: grantee.clone(),
        total_amount: 100000,
        flow_rate: 1000,
        asset: token.clone(),
        warmup_duration: 0,
        validator: None,
        linked_addresses: vec![&env],
        milestone_amount: 25000,
        total_milestones: 4,
    };

    GrantContract::batch_init(
        env.clone(),
        vec![&env, grant_config.clone()],
        1, // starting_grant_id
    ).unwrap();

    let claim_id = GrantContract::claim_milestone(
        env.clone(),
        1, // grant_id
        1, // milestone_number
        String::from_str(&env, "Milestone to challenge"),
        String::from_str(&env, "Evidence of completion"),
    ).unwrap();

    // Test 1: Challenging non-existent claim
    let result = GrantContract::challenge_milestone(
        env.clone(),
        challenger.clone(),
        999, // non-existent claim_id
        String::from_str(&env, "Invalid challenge"),
        String::from_str(&env, "Evidence"),
    );
    assert_eq!(result, Err(Error::MilestoneNotFound));

    // Test 2: Challenging claim not in claimed state
    // First, let the challenge period expire
    env.ledger().set_timestamp(env.ledger().timestamp() + CHALLENGE_PERIOD + 1);
    
    let result = GrantContract::challenge_milestone(
        env.clone(),
        challenger.clone(),
        claim_id,
        String::from_str(&env, "Late challenge"),
        String::from_str(&env, "Evidence"),
    );
    assert_eq!(result, Err(Error::ChallengePeriodExpired));

    // Reset timestamp
    env.ledger().set_timestamp(env.ledger().timestamp() - CHALLENGE_PERIOD - 1);

    // Test 3: Challenge reason too long
    let long_reason = String::from_str(&env, &"a".repeat(MAX_CHALLENGE_REASON_LENGTH as usize + 1));
    let result = GrantContract::challenge_milestone(
        env.clone(),
        challenger.clone(),
        claim_id,
        long_reason,
        String::from_str(&env, "Evidence"),
    );
    assert_eq!(result, Err(Error::InvalidReasonLength));

    // Test 4: Challenge evidence too long
    let long_evidence = String::from_str(&env, &"a".repeat(MAX_EVIDENCE_LENGTH as usize + 1));
    let result = GrantContract::challenge_milestone(
        env.clone(),
        challenger.clone(),
        claim_id,
        String::from_str(&env, "Valid reason"),
        long_evidence,
    );
    assert_eq!(result, Err(Error::InvalidReasonLength));
}

#[test]
fn test_milestone_fund_release() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
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

    // Create grant and claim milestone
    let grant_config = GranteeConfig {
        recipient: grantee.clone(),
        total_amount: 100000,
        flow_rate: 1000,
        asset: token.clone(),
        warmup_duration: 0,
        validator: None,
        linked_addresses: vec![&env],
        milestone_amount: 25000,
        total_milestones: 4,
    };

    GrantContract::batch_init(
        env.clone(),
        vec![&env, grant_config.clone()],
        1, // starting_grant_id
    ).unwrap();

    let claim_id = GrantContract::claim_milestone(
        env.clone(),
        1, // grant_id
        1, // milestone_number
        String::from_str(&env, "Completed milestone"),
        String::from_str(&env, "Evidence of completion"),
    ).unwrap();

    // Test 1: Release before challenge period expires (should fail)
    let result = GrantContract::release_milestone_funds(
        env.clone(),
        claim_id,
    );
    assert_eq!(result, Err(Error::ChallengePeriodExpired));

    // Test 2: Release after challenge period expires (should succeed)
    // Advance time past challenge period
    env.ledger().set_timestamp(env.ledger().timestamp() + CHALLENGE_PERIOD + 1);
    
    let result = GrantContract::release_milestone_funds(
        env.clone(),
        claim_id,
    );
    assert_eq!(result, Ok(()));

    // Verify claim status updated to Paid
    let claim = GrantContract::get_milestone_claim(env.clone(), claim_id).unwrap();
    assert_eq!(claim.status, MilestoneStatus::Paid);

    // Verify grant status returned to Active
    let grant = GrantContract::get_grant(env.clone(), 1).unwrap();
    assert_eq!(grant.status, GrantStatus::Active);
}

#[test]
fn test_milestone_challenge_resolution() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let challenger = Address::generate(&env);
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

    // Create grant and claim milestone
    let grant_config = GranteeConfig {
        recipient: grantee.clone(),
        total_amount: 100000,
        flow_rate: 1000,
        asset: token.clone(),
        warmup_duration: 0,
        validator: None,
        linked_addresses: vec![&env],
        milestone_amount: 25000,
        total_milestones: 4,
    };

    GrantContract::batch_init(
        env.clone(),
        vec![&env, grant_config.clone()],
        1, // starting_grant_id
    ).unwrap();

    let claim_id = GrantContract::claim_milestone(
        env.clone(),
        1, // grant_id
        1, // milestone_number
        String::from_str(&env, "Milestone to resolve"),
        String::from_str(&env, "Evidence of completion"),
    ).unwrap();

    let challenge_id = GrantContract::challenge_milestone(
        env.clone(),
        challenger.clone(),
        claim_id,
        String::from_str(&env, "Challenge milestone"),
        String::from_str(&env, "Evidence of incomplete work"),
    ).unwrap();

    // Test 1: Resolve challenge in favor of claimer (approve)
    let result = GrantContract::resolve_milestone_challenge(
        env.clone(),
        admin.clone(),
        challenge_id,
        true, // approved
        String::from_str(&env, "Challenge rejected - milestone is valid"),
    );
    assert_eq!(result, Ok(()));

    // Verify claim status updated to Approved
    let claim = GrantContract::get_milestone_claim(env.clone(), claim_id).unwrap();
    assert_eq!(claim.status, MilestoneStatus::Approved);

    // Verify challenge resolved
    let challenge = GrantContract::get_milestone_challenge(env.clone(), challenge_id).unwrap();
    assert_eq!(challenge.status, ChallengeStatus::ResolvedApproved);
    assert!(challenge.resolved_at.is_some());
    assert_eq!(challenge.resolution, Some(String::from_str(&env, "Challenge rejected - milestone is valid")));

    // Test 2: Resolve challenge against claimer (reject)
    let claim_id2 = GrantContract::claim_milestone(
        env.clone(),
        1, // grant_id
        2, // milestone_number
        String::from_str(&env, "Second milestone"),
        String::from_str(&env, "Evidence of completion"),
    ).unwrap();

    let challenge_id2 = GrantContract::challenge_milestone(
        env.clone(),
        challenger.clone(),
        claim_id2,
        String::from_str(&env, "Valid challenge"),
        String::from_str(&env, "Evidence of incomplete work"),
    ).unwrap();

    let result2 = GrantContract::resolve_milestone_challenge(
        env.clone(),
        admin.clone(),
        challenge_id2,
        false, // rejected
        String::from_str(&env, "Challenge upheld - milestone invalid"),
    );
    assert_eq!(result2, Ok(()));

    // Verify claim status updated to Rejected
    let claim2 = GrantContract::get_milestone_claim(env.clone(), claim_id2).unwrap();
    assert_eq!(claim2.status, MilestoneStatus::Rejected);

    // Verify challenge resolved
    let challenge2 = GrantContract::get_milestone_challenge(env.clone(), challenge_id2).unwrap();
    assert_eq!(challenge2.status, ChallengeStatus::ResolvedRejected);
    assert!(challenge2.resolved_at.is_some());
    assert_eq!(challenge2.resolution, Some(String::from_str(&env, "Challenge upheld - milestone invalid")));
}

#[test]
fn test_milestone_query_functions() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
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

    // Create grant with milestone configuration
    let grant_config = GranteeConfig {
        recipient: grantee.clone(),
        total_amount: 100000,
        flow_rate: 1000,
        asset: token.clone(),
        warmup_duration: 0,
        validator: None,
        linked_addresses: vec![&env],
        milestone_amount: 25000,
        total_milestones: 4,
    };

    GrantContract::batch_init(
        env.clone(),
        vec![&env, grant_config.clone()],
        1, // starting_grant_id
    ).unwrap();

    // Create multiple milestone claims
    let claim_id1 = GrantContract::claim_milestone(
        env.clone(),
        1, // grant_id
        1, // milestone_number
        String::from_str(&env, "First milestone"),
        String::from_str(&env, "Evidence 1"),
    ).unwrap();

    let claim_id2 = GrantContract::claim_milestone(
        env.clone(),
        1, // grant_id
        2, // milestone_number
        String::from_str(&env, "Second milestone"),
        String::from_str(&env, "Evidence 2"),
    ).unwrap();

    // Test getting all milestone claims for grant
    let milestone_ids = GrantContract::get_grant_milestones(env.clone(), 1).unwrap();
    assert_eq!(milestone_ids.len(), 2);
    assert!(milestone_ids.contains(&claim_id1));
    assert!(milestone_ids.contains(&claim_id2));

    // Test getting individual milestone claims
    let claim1 = GrantContract::get_milestone_claim(env.clone(), claim_id1).unwrap();
    assert_eq!(claim1.milestone_number, 1);
    assert_eq!(claim1.grant_id, 1);

    let claim2 = GrantContract::get_milestone_claim(env.clone(), claim_id2).unwrap();
    assert_eq!(claim2.milestone_number, 2);
    assert_eq!(claim2.grant_id, 1);

    // Create challenges for testing
    let challenge_id1 = GrantContract::challenge_milestone(
        env.clone(),
        admin.clone(),
        claim_id1,
        String::from_str(&env, "Challenge 1"),
        String::from_str(&env, "Evidence 1"),
    ).unwrap();

    let challenge_id2 = GrantContract::challenge_milestone(
        env.clone(),
        admin.clone(),
        claim_id2,
        String::from_str(&env, "Challenge 2"),
        String::from_str(&env, "Evidence 2"),
    ).unwrap();

    // Test getting individual challenges
    let challenge1 = GrantContract::get_milestone_challenge(env.clone(), challenge_id1).unwrap();
    assert_eq!(challenge1.claim_id, claim_id1);
    assert_eq!(challenge1.challenger, admin);

    let challenge2 = GrantContract::get_milestone_challenge(env.clone(), challenge_id2).unwrap();
    assert_eq!(challenge2.claim_id, claim_id2);
    assert_eq!(challenge2.challenger, admin);
}

#[test]
fn test_milestone_insufficient_funds() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
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

    // Create grant with insufficient milestone funds
    let grant_config = GranteeConfig {
        recipient: grantee.clone(),
        total_amount: 50000, // Only 2 milestones worth of funds
        flow_rate: 1000,
        asset: token.clone(),
        warmup_duration: 0,
        validator: None,
        linked_addresses: vec![&env],
        milestone_amount: 25000, // 4 milestones needed, but only funds for 2
        total_milestones: 4,
    };

    GrantContract::batch_init(
        env.clone(),
        vec![&env, grant_config.clone()],
        1, // starting_grant_id
    ).unwrap();

    // Try to claim milestone (should fail due to insufficient funds)
    let result = GrantContract::claim_milestone(
        env.clone(),
        1, // grant_id
        1, // milestone_number
        String::from_str(&env, "Attempt to claim with insufficient funds"),
        String::from_str(&env, "Evidence"),
    );
    assert_eq!(result, Err(Error::InsufficientMilestoneFunds));
}

#[test]
fn test_milestone_comprehensive_workflow() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let challenger = Address::generate(&env);
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
        oracle.clone(),
        native_token.clone(),
    ).unwrap();

    // Create grant with milestone configuration
    let grant_config = GranteeConfig {
        recipient: grantee.clone(),
        total_amount: 100000,
        flow_rate: 1000,
        asset: token.clone(),
        warmup_duration: 0,
        validator: None,
        linked_addresses: vec![&env],
        milestone_amount: 25000,
        total_milestones: 4,
    };

    GrantContract::batch_init(
        env.clone(),
        vec![&env, grant_config.clone()],
        1, // starting_grant_id
    ).unwrap();

    // Step 1: Claim first milestone
    let claim_id1 = GrantContract::claim_milestone(
        env.clone(),
        1, // grant_id
        1, // milestone_number
        String::from_str(&env, "Completed initial development"),
        String::from_str(&env, "GitHub repo with initial code"),
    ).unwrap();

    // Step 2: Challenge first milestone
    let challenge_id1 = GrantContract::challenge_milestone(
        env.clone(),
        challenger.clone(),
        claim_id1,
        String::from_str(&env, "Development not complete"),
        String::from_str(&env, "Missing key features"),
    ).unwrap();

    // Step 3: Reject challenge (milestone was actually complete)
    let result = GrantContract::resolve_milestone_challenge(
        env.clone(),
        admin.clone(),
        challenge_id1,
        true, // approved - challenge rejected
        String::from_str(&env, "Development is complete - challenge rejected"),
    );
    assert_eq!(result, Ok(()));

    // Step 4: Release milestone funds after approval
    // Advance time past challenge period
    env.ledger().set_timestamp(env.ledger().timestamp() + CHALLENGE_PERIOD + 1);
    
    let result = GrantContract::release_milestone_funds(
        env.clone(),
        claim_id1,
    );
    assert_eq!(result, Ok(()));

    // Step 5: Claim second milestone
    let claim_id2 = GrantContract::claim_milestone(
        env.clone(),
        1, // grant_id
        2, // milestone_number
        String::from_str(&env, "Completed testing phase"),
        String::from_str(&env, "Test suite results"),
    ).unwrap();

    // Step 6: Let second milestone pass without challenge
    // Advance time past challenge period
    env.ledger().set_timestamp(env.ledger().timestamp() + CHALLENGE_PERIOD + 1);
    
    let result = GrantContract::release_milestone_funds(
        env.clone(),
        claim_id2,
    );
    assert_eq!(result, Ok(()));

    // Step 7: Claim third milestone
    let claim_id3 = GrantContract::claim_milestone(
        env.clone(),
        1, // grant_id
        3, // milestone_number
        String::from_str(&env, "Completed deployment"),
        String::from_str(&env, "Deployment evidence"),
    ).unwrap();

    // Step 8: Challenge third milestone
    let challenge_id3 = GrantContract::challenge_milestone(
        env.clone(),
        challenger.clone(),
        claim_id3,
        String::from_str(&env, "Deployment issues found"),
        String::from_str(&env, "Bug reports"),
    ).unwrap();

    // Step 9: Approve challenge (deployment had issues but was acceptable)
    let result = GrantContract::resolve_milestone_challenge(
        env.clone(),
        admin.clone(),
        challenge_id3,
        false, // rejected - challenge upheld
        String::from_str(&env, "Deployment issues are acceptable - challenge approved"),
    );
    assert_eq!(result, Ok(()));

    // Verify final state
    let milestones = GrantContract::get_grant_milestones(env.clone(), 1).unwrap();
    assert_eq!(milestones.len(), 3);

    let claim1 = GrantContract::get_milestone_claim(env.clone(), claim_id1).unwrap();
    assert_eq!(claim1.status, MilestoneStatus::Paid);

    let claim2 = GrantContract::get_milestone_claim(env.clone(), claim_id2).unwrap();
    assert_eq!(claim2.status, MilestoneStatus::Paid);

    let claim3 = GrantContract::get_milestone_claim(env.clone(), claim_id3).unwrap();
    assert_eq!(claim3.status, MilestoneStatus::Rejected);

    let challenge3 = GrantContract::get_milestone_challenge(env.clone(), challenge_id3).unwrap();
    assert_eq!(challenge3.status, ChallengeStatus::ResolvedRejected);
}
