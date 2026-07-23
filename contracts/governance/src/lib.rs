#![no_std]

mod proposal_manager;
mod voting_power;
mod delegation;
mod timelock;
mod quorum_calculator;

use soroban_sdk::{contract, contractimpl, Address, Env, String, Vec};
use soroban_sdk::contracttype;

#[cfg(test)]
mod test;

use proposal_manager::{ProposalManager, Proposal, ProposalStatus, ProposalType, ProposalVersion, VoteRecord};
use voting_power::VotingPower;
use delegation::DelegationManager;
use timelock::TimeLock;
use quorum_calculator::QuorumCalculator;

#[contract]
pub struct GovernorQuadratic;

#[contractimpl]
impl GovernorQuadratic {
    pub fn initialize(
        env: Env,
        governance_token: Address,
        admin: Address,
        total_supply: i128,
    ) {
        if env.storage().instance().has(&StorageKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage().instance().set(&StorageKey::GovernanceToken, &governance_token);
        env.storage().instance().set(&StorageKey::TotalSupply, &total_supply);
        env.storage().instance().set(&StorageKey::ProposalIdCounter, &0u64);
        env.storage().instance().set(&StorageKey::ProposalIds, &Vec::<u64>::new(&env));
        env.storage().instance().set(&StorageKey::DelegationCount, &0u64);
        VotingPower::initialize(&env);
        QuorumCalculator::initialize(&env, total_supply);
        TimeLock::initialize(&env);

        env.events().publish(
            (soroban_sdk::symbol_short!("init"),),
            (governance_token, admin, total_supply),
        );
    }

    pub fn create_proposal(
        env: Env,
        proposer: Address,
        title: String,
        description: String,
        proposal_type: ProposalType,
        voting_period_sec: u64,
    ) -> u64 {
        proposer.require_auth();
        ProposalManager::create_proposal(
            &env, &proposer, title, description, proposal_type, voting_period_sec,
        )
    }

    pub fn amend_proposal(
        env: Env,
        proposer: Address,
        proposal_id: u64,
        new_title: String,
        new_description: String,
    ) -> u32 {
        proposer.require_auth();
        ProposalManager::amend_proposal(&env, &proposer, proposal_id, new_title, new_description)
    }

    pub fn start_voting(
        env: Env,
        proposer: Address,
        proposal_id: u64,
    ) {
        proposer.require_auth();
        ProposalManager::start_voting(&env, proposal_id);
    }

    pub fn delegate_vote(
        env: Env,
        delegator: Address,
        delegate: Address,
        proposal_type: ProposalType,
    ) {
        delegator.require_auth();
        DelegationManager::set_delegate(&env, &delegator, &delegate, &proposal_type);
    }

    pub fn undelegate_vote(
        env: Env,
        delegator: Address,
        proposal_type: ProposalType,
    ) {
        delegator.require_auth();
        DelegationManager::remove_delegate(&env, &delegator, &proposal_type);
    }

    pub fn lock_tokens(
        env: Env,
        holder: Address,
        amount: i128,
        duration_weeks: u32,
    ) {
        holder.require_auth();
        VotingPower::lock_tokens(&env, &holder, amount, duration_weeks);
    }

    pub fn withdraw_locked_tokens(
        env: Env,
        holder: Address,
    ) {
        holder.require_auth();
        VotingPower::withdraw_locked(&env, &holder);
    }

    pub fn cast_vote(
        env: Env,
        voter: Address,
        proposal_id: u64,
        support: bool,
        votes: i128,
    ) {
        voter.require_auth();

        let proposal = ProposalManager::get_proposal(&env, proposal_id);
        if proposal.status != ProposalStatus::Voting {
            panic!("proposal not in voting phase");
        }

        let now = env.ledger().timestamp();
        if now >= proposal.voting_end {
            panic!("voting period ended");
        }

        let actual_voter = DelegationManager::resolve_voter(&env, &voter, &proposal.proposal_type);
        let voting_power = VotingPower::calculate_power(&env, &actual_voter);
        let cost = votes.checked_mul(votes).expect("overflow");
        if cost > voting_power {
            panic!("insufficient voting power");
        }

        ProposalManager::cast_vote(&env, &actual_voter, proposal_id, support, votes, cost);
    }

    pub fn queue_proposal(
        env: Env,
        caller: Address,
        proposal_id: u64,
    ) {
        caller.require_auth();
        let mut proposal = ProposalManager::get_proposal(&env, proposal_id);

        if proposal.status != ProposalStatus::Voting {
            panic!("proposal not in voting phase");
        }
        let now = env.ledger().timestamp();
        if now < proposal.voting_end {
            panic!("voting period not ended");
        }

        let total_supply: i128 = env.storage().instance().get(&StorageKey::TotalSupply).expect("not initialized");
        let quorum = QuorumCalculator::calculate_quorum(&env, &proposal);
        let quorum_threshold = total_supply / 25;

        if quorum < quorum_threshold {
            proposal.status = ProposalStatus::Cancelled;
            ProposalManager::save_proposal(&env, proposal_id, &proposal);
            panic!("quorum not met");
        }

        proposal.status = ProposalStatus::Queued;
        proposal.queue_time = now;
        ProposalManager::save_proposal(&env, proposal_id, &proposal);

        TimeLock::queue_action(&env, proposal_id, now);
    }

    pub fn execute_proposal(
        env: Env,
        caller: Address,
        proposal_id: u64,
    ) {
        caller.require_auth();

        let mut proposal = ProposalManager::get_proposal(&env, proposal_id);
        if proposal.status != ProposalStatus::Queued {
            panic!("proposal not queued");
        }

        TimeLock::assert_can_execute(&env, proposal_id, proposal.queue_time);

        proposal.status = ProposalStatus::Executed;
        proposal.executed_at = env.ledger().timestamp();
        ProposalManager::save_proposal(&env, proposal_id, &proposal);

        env.events().publish(
            (soroban_sdk::symbol_short!("execute"), proposal_id),
            (proposal.proposer, proposal.executed_at),
        );
    }

    pub fn cancel_proposal(
        env: Env,
        caller: Address,
        proposal_id: u64,
    ) {
        caller.require_auth();
        let mut proposal = ProposalManager::get_proposal(&env, proposal_id);

        if caller != proposal.proposer {
            let admin: Address = env.storage().instance().get(&StorageKey::Admin).expect("not initialized");
            if caller != admin {
                panic!("not authorized");
            }
        }

        if proposal.status == ProposalStatus::Executed {
            panic!("already executed");
        }

        if proposal.status == ProposalStatus::Queued {
            let now = env.ledger().timestamp();
            if now >= proposal.queue_time + TimeLock::DELAY_SEC {
                panic!("timelock delay already passed");
            }
            TimeLock::cancel_action(&env, proposal_id);
        }

        proposal.status = ProposalStatus::Cancelled;
        ProposalManager::save_proposal(&env, proposal_id, &proposal);
    }

    pub fn get_proposal(env: Env, proposal_id: u64) -> Proposal {
        ProposalManager::get_proposal(&env, proposal_id)
    }

    pub fn get_proposal_version(
        env: Env,
        proposal_id: u64,
        version: u32,
    ) -> ProposalVersion {
        ProposalManager::get_proposal_version(&env, proposal_id, version)
    }

    pub fn get_delegate(
        env: Env,
        delegator: Address,
        proposal_type: ProposalType,
    ) -> Option<Address> {
        DelegationManager::get_delegate(&env, &delegator, &proposal_type)
    }

    pub fn get_voting_power(env: Env, holder: Address) -> i128 {
        VotingPower::calculate_power(&env, &holder)
    }

    pub fn get_all_proposal_ids(env: Env) -> Vec<u64> {
        ProposalManager::get_all_proposal_ids(&env)
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage().instance().get(&StorageKey::Admin).expect("not initialized")
    }

    pub fn get_governance_token(env: Env) -> Address {
        env.storage().instance().get(&StorageKey::GovernanceToken).expect("not initialized")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub enum StorageKey {
    Admin,
    GovernanceToken,
    TotalSupply,
    ProposalIdCounter,
    ProposalIds,
    DelegationCount,
    Proposal(u64),
    ProposalVersion(u64, u32),
    ProposalVotes(u64),
    LockedBalance(Address),
    MaxMultiplier,
    BaseMultiplier,
    Delegation(Address, ProposalType),
    TimelockDelay,
    TimelockAction(u64),
    QuorumBps,
    QuorumVotes,
}
