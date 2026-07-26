#[cfg(test)]
mod tests {
    use soroban_sdk::{testutils::Address as _, Address, Env, String};
    use crate::proposal_manager::{ProposalStatus, ProposalType};
    use crate::GovernorQuadratic;

    fn create_env() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let token = Address::generate(&env);
        (env, admin, token)
    }

    fn setup(env: &Env, token: &Address, admin: &Address) {
        GovernorQuadratic::initialize(
            env.clone(),
            token.clone(),
            admin.clone(),
            1_000_000_000_000_000,
        );
    }

    #[test]
    fn test_initialize() {
        let (env, admin, token) = create_env();
        setup(&env, &token, &admin);

        let stored_admin = GovernorQuadratic::get_admin(env.clone());
        assert_eq!(stored_admin, admin);
    }

    #[test]
    fn test_create_proposal() {
        let (env, admin, token) = create_env();
        setup(&env, &token, &admin);

        let id = GovernorQuadratic::create_proposal(
            env.clone(),
            admin.clone(),
            String::from_str(&env, "Test"),
            String::from_str(&env, "Description"),
            ProposalType::TextProposal,
            86400,
        );

        let proposal = GovernorQuadratic::get_proposal(env.clone(), id);
        assert_eq!(proposal.id, id);
        assert_eq!(proposal.proposer, admin);
        assert_eq!(proposal.status, ProposalStatus::Draft);
    }

    #[test]
    fn test_amend_proposal() {
        let (env, admin, token) = create_env();
        setup(&env, &token, &admin);

        let id = GovernorQuadratic::create_proposal(
            env.clone(),
            admin.clone(),
            String::from_str(&env, "Original"),
            String::from_str(&env, "Original desc"),
            ProposalType::TextProposal,
            86400,
        );

        let version = GovernorQuadratic::amend_proposal(
            env.clone(),
            admin.clone(),
            id,
            String::from_str(&env, "Amended"),
            String::from_str(&env, "Amended desc"),
        );

        assert_eq!(version, 1);

        let v0 = GovernorQuadratic::get_proposal_version(env.clone(), id, 0);
        assert_eq!(v0.title, String::from_str(&env, "Original"));

        let v1 = GovernorQuadratic::get_proposal_version(env.clone(), id, 1);
        assert_eq!(v1.title, String::from_str(&env, "Amended"));

        let proposal = GovernorQuadratic::get_proposal(env.clone(), id);
        assert_eq!(proposal.title, String::from_str(&env, "Amended"));
        assert_eq!(proposal.current_version, 1);
    }

    #[test]
    fn test_voting_power_and_lock() {
        let (env, admin, token) = create_env();
        setup(&env, &token, &admin);

        let voter = Address::generate(&env);

        let id = GovernorQuadratic::create_proposal(
            env.clone(),
            admin.clone(),
            String::from_str(&env, "Test"),
            String::from_str(&env, "Desc"),
            ProposalType::TextProposal,
            86400,
        );

        GovernorQuadratic::start_voting(env.clone(), admin.clone(), id);

        let voting_power = GovernorQuadratic::get_voting_power(env.clone(), voter.clone());
        assert_eq!(voting_power, 0);

        GovernorQuadratic::lock_tokens(env.clone(), voter.clone(), 1000, 52);

        let power_after_lock = GovernorQuadratic::get_voting_power(env.clone(), voter.clone());
        assert!(power_after_lock > 0);
    }

    #[test]
    fn test_full_proposal_lifecycle() {
        let (env, admin, token) = create_env();
        setup(&env, &token, &admin);

        let voter = Address::generate(&env);

        let id = GovernorQuadratic::create_proposal(
            env.clone(),
            admin.clone(),
            String::from_str(&env, "Full Cycle"),
            String::from_str(&env, "Testing full lifecycle"),
            ProposalType::ParameterChange,
            3600,
        );

        GovernorQuadratic::start_voting(env.clone(), admin.clone(), id);

        GovernorQuadratic::lock_tokens(env.clone(), voter.clone(), 1_000_000_000_000, 52);
        GovernorQuadratic::cast_vote(env.clone(), voter.clone(), id, true, 1000);

        env.ledger().set_timestamp(env.ledger().timestamp() + 4000);

        GovernorQuadratic::queue_proposal(env.clone(), admin.clone(), id);

        env.ledger().set_timestamp(env.ledger().timestamp() + 172800 + 1);

        GovernorQuadratic::execute_proposal(env.clone(), admin.clone(), id);

        let proposal = GovernorQuadratic::get_proposal(env.clone(), id);
        assert_eq!(proposal.status, ProposalStatus::Executed);
    }

    #[test]
    fn test_delegation() {
        let (env, admin, token) = create_env();
        setup(&env, &token, &admin);

        let delegator = Address::generate(&env);
        let delegate = Address::generate(&env);

        GovernorQuadratic::delegate_vote(
            env.clone(),
            delegator.clone(),
            delegate.clone(),
            ProposalType::TreasurySpend,
        );

        let stored = GovernorQuadratic::get_delegate(
            env.clone(),
            delegator.clone(),
            ProposalType::TreasurySpend,
        );
        assert_eq!(stored, Some(delegate));
    }

    #[test]
    fn test_cancel_proposal() {
        let (env, admin, token) = create_env();
        setup(&env, &token, &admin);

        let id = GovernorQuadratic::create_proposal(
            env.clone(),
            admin.clone(),
            String::from_str(&env, "Cancel Test"),
            String::from_str(&env, "Desc"),
            ProposalType::TextProposal,
            86400,
        );

        GovernorQuadratic::cancel_proposal(env.clone(), admin.clone(), id);

        let proposal = GovernorQuadratic::get_proposal(env.clone(), id);
        assert_eq!(proposal.status, ProposalStatus::Cancelled);
    }

    #[test]
    fn test_treasury_spend_proposal() {
        let (env, admin, token) = create_env();
        setup(&env, &token, &admin);

        let id = GovernorQuadratic::create_proposal(
            env.clone(),
            admin.clone(),
            String::from_str(&env, "Treasury Spend"),
            String::from_str(&env, "Spend funds"),
            ProposalType::TreasurySpend,
            86400,
        );
        let proposal = GovernorQuadratic::get_proposal(env.clone(), id);
        assert_eq!(proposal.proposal_type, ProposalType::TreasurySpend);
    }

    #[test]
    fn test_contract_upgrade_proposal() {
        let (env, admin, token) = create_env();
        setup(&env, &token, &admin);

        let id = GovernorQuadratic::create_proposal(
            env.clone(),
            admin.clone(),
            String::from_str(&env, "Upgrade"),
            String::from_str(&env, "Upgrade contract"),
            ProposalType::ContractUpgrade,
            86400,
        );
        let proposal = GovernorQuadratic::get_proposal(env.clone(), id);
        assert_eq!(proposal.proposal_type, ProposalType::ContractUpgrade);
    }
}
