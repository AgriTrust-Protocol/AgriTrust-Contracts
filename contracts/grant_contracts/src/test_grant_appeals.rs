#[cfg(test)]
mod test_grant_appeals {
    use super::*;
    use soroban_sdk::testutils::{Address as TestAddress, Ledger as TestLedger};
    use soroban_sdk::{Address, Env, String, Vec};

    #[test]
    fn test_time_weighted_voting_calculation() {
        let env = Env::new();
        let admin = Address::generate(&env);
        let governance_token = Address::generate(&env);

        // Initialize appeal system
        GrantAppealContract::initialize(env.clone(), governance_token).unwrap();

        let voter = Address::generate(&env);
        
        // Mock token balance (would require token contract in real test)
        // For now, test the time multiplier logic
        let holding_durations = vec![
            (10 * 86400, 2500),    // 10 days: 25%
            (30 * 86400, 5000),    // 30 days: 50%
            (90 * 86400, 7500),    // 90 days: 75%
            (180 * 86400, 9000),  // 180 days: 90%
            (365 * 86400, 10000), // 365 days: 100%
            (730 * 86400, 12000), // 730 days: 120%
        ];

        for (duration, expected_multiplier) in holding_durations {
            let multiplier = GrantAppealContract::get_time_multiplier(duration);
            assert_eq!(multiplier, expected_multiplier);
        }
    }

    #[test]
    fn test_appeal_creation() {
        let env = Env::new();
        let admin = Address::generate(&env);
        let governance_token = Address::generate(&env);
        let appellant = Address::generate(&env);

        // Initialize appeal system
        GrantAppealContract::initialize(env.clone(), governance_token).unwrap();

        let grant_id = 1u64;
        let reason = String::from_str(&env, "This grant was unfairly cancelled and should be reinstated.");
        let evidence = String::from_str(&env, "Evidence of compliance and progress...");

        // Create appeal
        let appeal_id = GrantAppealContract::create_appeal(
            env.clone(),
            grant_id,
            appellant.clone(),
            reason.clone(),
            evidence.clone(),
        ).unwrap();

        // Verify appeal was created
        let appeal = GrantAppealContract::get_appeal_info(env.clone(), appeal_id).unwrap();
        assert_eq!(appeal.grant_id, grant_id);
        assert_eq!(appeal.appellant, appellant);
        assert_eq!(appeal.reason, reason);
        assert_eq!(appeal.status, AppealStatus::Proposed);
    }

    #[test]
    fn test_appeal_voting() {
        let env = Env::new();
        let admin = Address::generate(&env);
        let governance_token = Address::generate(&env);
        let appellant = Address::generate(&env);
        let voter = Address::generate(&env);

        // Initialize appeal system
        GrantAppealContract::initialize(env.clone(), governance_token).unwrap();

        let grant_id = 1u64;
        let reason = String::from_str(&env, "This grant was unfairly cancelled");
        let evidence = String::from_str(&env, "Evidence of progress...");

        // Create appeal
        let appeal_id = GrantAppealContract::create_appeal(
            env.clone(),
            grant_id,
            appellant,
            reason,
            evidence,
        ).unwrap();

        // Test voting (would require token balance in real implementation)
        // For now, test the voting logic structure
        let appeal = GrantAppealContract::get_appeal_info(env.clone(), appeal_id).unwrap();
        assert_eq!(appeal.votes_for, 0);
        assert_eq!(appeal.votes_against, 0);
    }

    #[test]
    fn test_appeal_execution() {
        let env = Env::new();
        let admin = Address::generate(&env);
        let governance_token = Address::generate(&env);
        let appellant = Address::generate(&env);

        // Initialize appeal system
        GrantAppealContract::initialize(env.clone(), governance_token).unwrap();

        let grant_id = 1u64;
        let reason = String::from_str(&env, "This grant was unfairly cancelled");
        let evidence = String::from_str(&env, "Evidence of progress...");

        // Create appeal
        let appeal_id = GrantAppealContract::create_appeal(
            env.clone(),
            grant_id,
            appellant,
            reason,
            evidence,
        ).unwrap();

        // Fast forward past voting deadline
        env.ledger().set_timestamp(
            env.ledger().timestamp() + APPEAL_VOTING_PERIOD + 1,
        );

        // Try to execute without votes (should fail participation threshold)
        let result = GrantAppealContract::execute_appeal(env.clone(), admin, appeal_id);
        assert_eq!(result, Err(AppealError::ParticipationThresholdNotMet));

        let appeal = GrantAppealContract::get_appeal_info(env.clone(), appeal_id).unwrap();
        assert_eq!(appeal.status, AppealStatus::Expired);
    }

    #[test]
    fn test_appeal_results_calculation() {
        let appeal = GrantAppeal {
            appeal_id: 1,
            grant_id: 1,
            appellant: Address::generate(&Env::new()),
            reason: String::from_str(&Env::new(), "Test"),
            evidence_hash: [0u8; 32],
            created_at: 0,
            voting_deadline: 0,
            status: AppealStatus::Proposed,
            votes_for: 6600,  // 66% approval
            votes_against: 3400, // 34% against
            total_eligible_power: 10000, // 100% participation
            executed_at: None,
        };

        let (participation_met, approval_met) = GrantAppealContract::calculate_appeal_results(&appeal);
        assert!(participation_met); // 100% >= 10% threshold
        assert!(approval_met);     // 66% >= 66% threshold

        // Test failing case
        let appeal_failing = GrantAppeal {
            votes_for: 5000,  // 50% approval (below threshold)
            votes_against: 5000,
            total_eligible_power: 5000, // 50% participation (above threshold)
            ..appeal
        };

        let (participation_met, approval_met) = GrantAppealContract::calculate_appeal_results(&appeal_failing);
        assert!(participation_met); // 100% >= 10% threshold
        assert!(!approval_met);     // 50% < 66% threshold
    }

    #[test]
    fn test_multiple_appeals_per_grant() {
        let env = Env::new();
        let admin = Address::generate(&env);
        let governance_token = Address::generate(&env);
        let appellant = Address::generate(&env);

        // Initialize appeal system
        GrantAppealContract::initialize(env.clone(), governance_token).unwrap();

        let grant_id = 1u64;
        let reason = String::from_str(&env, "This grant was unfairly cancelled");
        let evidence = String::from_str(&env, "Evidence of progress...");

        // Create first appeal
        let appeal_id1 = GrantAppealContract::create_appeal(
            env.clone(),
            grant_id,
            appellant.clone(),
            reason.clone(),
            evidence.clone(),
        ).unwrap();

        // Try to create second appeal (should fail)
        let result = GrantAppealContract::create_appeal(
            env.clone(),
            grant_id,
            appellant,
            reason,
            evidence,
        );
        assert_eq!(result, Err(AppealError::AppealAlreadyExists));

        // Verify grant has one appeal
        let grant_appeals = GrantAppealContract::get_grant_appeals(env.clone(), grant_id).unwrap();
        assert_eq!(grant_appeals.len(), 1);
        assert_eq!(grant_appeals.get(0).unwrap(), appeal_id1);
    }
}
