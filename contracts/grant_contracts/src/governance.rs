#![no_std]

use soroban_sdk::{
    contract,
    contracterror,
    contractimpl,
    contracttype,
    symbol_short,
    token,
    Address,
    Env,
    Vec,
    Map,
};

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum ProposalStatus {
    Active,
    Passed,
    Rejected,
    Executed,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct Proposal {
    pub id: u64,
    pub proposer: Address,
    pub title: soroban_sdk::String,
    pub description: soroban_sdk::String,
    pub voting_deadline: u64,
    pub status: ProposalStatus,
    pub yes_votes: i128,
    pub no_votes: i128,
    pub total_voting_power: i128,
    pub created_at: u64,
    pub stake_amount: i128,
    pub stake_returned: bool,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct Vote {
    pub voter: Address,
    pub proposal_id: u64,
    pub weight: i128,
    pub voting_power: i128,
    pub voted_at: u64,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct VotingPower {
    pub address: Address,
    pub token_balance: i128,
    pub voting_power: i128,
    pub last_updated: u64,
}

#[derive(Clone)]
#[contracttype]
pub enum GovernanceDataKey {
    Proposal(u64),
    Vote(Address, u64),
    VotingPower(Address),
    ProposalIds,
    GovernanceToken,
    VotingThreshold,
    QuorumThreshold,
    StakeToken,
    ProposalStakeAmount,
}

#[contracterror]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum GovernanceError {
    NotInitialized = 101,
    AlreadyInitialized = 102,
    NotAuthorized = 103,
    ProposalNotFound = 104,
    ProposalAlreadyExists = 105,
    VotingEnded = 106,
    InvalidWeight = 107,
    InvalidAmount = 108,
    MathOverflow = 109,
    QuorumNotMet = 110,
    ThresholdNotMet = 111,
    AlreadyVoted = 112,
    InsufficientStake = 113,
    StakeAlreadyReturned = 114,
    ProposalNotConcluded = 115,
}

pub struct GovernanceContract;

#[contractimpl]
impl GovernanceContract {
    pub fn initialize(
        env: Env,
        governance_token: Address,
        voting_threshold: i128,
        quorum_threshold: i128,
        stake_token: Address,
        proposal_stake_amount: i128
    ) -> Result<(), GovernanceError> {
        if env.storage().instance().has(&GovernanceDataKey::GovernanceToken) {
            return Err(GovernanceError::AlreadyInitialized);
        }

        env.storage().instance().set(&GovernanceDataKey::GovernanceToken, &governance_token);
        env.storage().instance().set(&GovernanceDataKey::VotingThreshold, &voting_threshold);
        env.storage().instance().set(&GovernanceDataKey::QuorumThreshold, &quorum_threshold);
        env.storage().instance().set(&GovernanceDataKey::ProposalIds, &Vec::<u64>::new(&env));
        env.storage().instance().set(&GovernanceDataKey::StakeToken, &stake_token);
        env.storage().instance().set(&GovernanceDataKey::ProposalStakeAmount, &proposal_stake_amount);

        Ok(())
    }

    pub fn create_proposal(
        env: Env,
        proposer: Address,
        title: soroban_sdk::String,
        description: soroban_sdk::String,
        voting_period: u64
    ) -> Result<u64, GovernanceError> {
        Self::propose_grant(env, proposer, title, description, voting_period)
    }

    pub fn propose_grant(
        env: Env,
        proposer: Address,
        title: soroban_sdk::String,
        description: soroban_sdk::String,
        voting_period: u64
    ) -> Result<u64, GovernanceError> {
        proposer.require_auth();

        let stake_token = Self::get_stake_token(&env)?;
        let stake_amount = Self::get_proposal_stake_amount(&env)?;

        // Transfer stake from proposer to this contract
        let token_client = token::Client::new(&env, &stake_token);
        token_client.transfer(&proposer, &env.current_contract_address(), &stake_amount);

        let now = env.ledger().timestamp();
        let voting_deadline = now.checked_add(voting_period).ok_or(GovernanceError::MathOverflow)?;

        let mut proposal_ids = Self::get_proposal_ids(&env)?;
        let proposal_id = if proposal_ids.is_empty() {
            0
        } else {
            let last_id = proposal_ids.get(proposal_ids.len() - 1).unwrap();
            last_id.checked_add(1).ok_or(GovernanceError::MathOverflow)?
        };

        let proposal = Proposal {
            id: proposal_id,
            proposer: proposer.clone(),
            title,
            description,
            voting_deadline,
            status: ProposalStatus::Active,
            yes_votes: 0,
            no_votes: 0,
            total_voting_power: 0,
            created_at: now,
            stake_amount,
            stake_returned: false,
        };

        env.storage().instance().set(&GovernanceDataKey::Proposal(proposal_id), &proposal);
        proposal_ids.push_back(proposal_id);
        env.storage().instance().set(&GovernanceDataKey::ProposalIds, &proposal_ids);

        env.events().publish((symbol_short!("prop_new"), proposal_id), (proposer, voting_deadline));

        Ok(proposal_id)
    }

    pub fn quadratic_vote(
        env: Env,
        voter: Address,
        proposal_id: u64,
        weight: i128
    ) -> Result<(), GovernanceError> {
        voter.require_auth();

        if weight <= 0 {
            return Err(GovernanceError::InvalidWeight);
        }

        let mut proposal = Self::get_proposal(&env, proposal_id)?;
        let now = env.ledger().timestamp();

        if now >= proposal.voting_deadline {
            return Err(GovernanceError::VotingEnded);
        }

        if proposal.status != ProposalStatus::Active {
            return Err(GovernanceError::VotingEnded);
        }

        // Check if already voted
        if env.storage().instance().has(&GovernanceDataKey::Vote(voter.clone(), proposal_id)) {
            return Err(GovernanceError::AlreadyVoted);
        }

        let voting_power = Self::calculate_voting_power(&env, &voter)?;
        let vote_weight = weight.checked_mul(voting_power).ok_or(GovernanceError::MathOverflow)?;

        let vote = Vote {
            voter: voter.clone(),
            proposal_id,
            weight,
            voting_power,
            voted_at: now,
        };

        env.storage().instance().set(&GovernanceDataKey::Vote(voter.clone(), proposal_id), &vote);

        // Update proposal vote counts (quadratic voting: weight^2)
        let quadratic_weight = weight.checked_mul(weight).ok_or(GovernanceError::MathOverflow)?;

        proposal.yes_votes = proposal.yes_votes
            .checked_add(quadratic_weight)
            .ok_or(GovernanceError::MathOverflow)?;

        proposal.total_voting_power = proposal.total_voting_power
            .checked_add(voting_power)
            .ok_or(GovernanceError::MathOverflow)?;

        env.storage().instance().set(&GovernanceDataKey::Proposal(proposal_id), &proposal);

        env.events().publish(
            (symbol_short!("quad_vote"), proposal_id),
            (voter, weight, voting_power, quadratic_weight)
        );

        Ok(())
    }

    pub fn calculate_voting_power(env: &Env, address: &Address) -> Result<i128, GovernanceError> {
        let governance_token = Self::get_governance_token(env)?;
        let token_client = token::Client::new(env, &governance_token);
        let token_balance = token_client.balance(address);

        // Quadratic voting: voting_power = sqrt(token_balance)
        // Using integer approximation of square root
        let voting_power = Self::integer_sqrt(token_balance);

        // Update cached voting power
        let voting_power_record = VotingPower {
            address: address.clone(),
            token_balance,
            voting_power,
            last_updated: env.ledger().timestamp(),
        };

        env.storage()
            .instance()
            .set(&GovernanceDataKey::VotingPower(address.clone()), &voting_power_record);

        Ok(voting_power)
    }

    fn integer_sqrt(n: i128) -> i128 {
        if n <= 0 {
            return 0;
        }

        let mut x = n;
        let mut y = (x + 1) / 2;

        while y < x {
            x = y;
            y = (x + n / x) / 2;
        }

        x
    }

    pub fn execute_proposal(env: Env, proposal_id: u64) -> Result<(), GovernanceError> {
        let mut proposal = Self::get_proposal(&env, proposal_id)?;
        let now = env.ledger().timestamp();

        if now < proposal.voting_deadline {
            return Err(GovernanceError::VotingEnded);
        }

        if proposal.status != ProposalStatus::Active {
            return Err(GovernanceError::VotingEnded);
        }

        let quorum_threshold = Self::get_quorum_threshold(&env)?;
        let voting_threshold = Self::get_voting_threshold(&env)?;

        // Check quorum
        if proposal.total_voting_power < quorum_threshold {
            proposal.status = ProposalStatus::Rejected;
            env.storage().instance().set(&GovernanceDataKey::Proposal(proposal_id), &proposal);
            return Err(GovernanceError::QuorumNotMet);
        }

        // Check voting threshold (simple majority for now)
        let total_votes = proposal.yes_votes.checked_add(proposal.no_votes).unwrap_or(0);
        if total_votes == 0 || proposal.yes_votes < voting_threshold {
            proposal.status = ProposalStatus::Rejected;
            env.storage().instance().set(&GovernanceDataKey::Proposal(proposal_id), &proposal);
            return Err(GovernanceError::ThresholdNotMet);
        }

        proposal.status = ProposalStatus::Executed;
        env.storage().instance().set(&GovernanceDataKey::Proposal(proposal_id), &proposal);

        env.events().publish(
            (symbol_short!("prop_exec"), proposal_id),
            (proposal.yes_votes, proposal.no_votes)
        );

        Ok(())
    }

    pub fn refund_stake(env: Env, proposal_id: u64) -> Result<(), GovernanceError> {
        let mut proposal = Self::get_proposal(&env, proposal_id)?;
        
        if proposal.status != ProposalStatus::Executed && proposal.status != ProposalStatus::Rejected {
            return Err(GovernanceError::ProposalNotConcluded);
        }

        if proposal.stake_returned {
            return Err(GovernanceError::StakeAlreadyReturned);
        }

        proposal.stake_returned = true;
        env.storage().instance().set(&GovernanceDataKey::Proposal(proposal_id), &proposal);

        if proposal.stake_amount > 0 {
            let stake_token = Self::get_stake_token(&env)?;
            let token_client = token::Client::new(&env, &stake_token);
            token_client.transfer(
                &env.current_contract_address(),
                &proposal.proposer,
                &proposal.stake_amount
            );
        }

        Ok(())
    }

    pub fn slash_stake(env: Env, admin: Address, target_treasury: Address, proposal_id: u64) -> Result<(), GovernanceError> {
        admin.require_auth();
        // Assume admin is validated elsewhere or we could add DataKey::Admin locally
        // For dao-fund, governance calls might be gated by proper admin authentication
        
        let mut proposal = Self::get_proposal(&env, proposal_id)?;

        if proposal.stake_returned {
            return Err(GovernanceError::StakeAlreadyReturned);
        }

        proposal.stake_returned = true;
        env.storage().instance().set(&GovernanceDataKey::Proposal(proposal_id), &proposal);

        if proposal.stake_amount > 0 {
            let stake_token = Self::get_stake_token(&env)?;
            let token_client = token::Client::new(&env, &stake_token);
            token_client.transfer(
                &env.current_contract_address(),
                &target_treasury,
                &proposal.stake_amount
            );
        }

        Ok(())
    }

    // Helper functions
    fn get_proposal_ids(env: &Env) -> Result<Vec<u64>, GovernanceError> {
        env.storage()
            .instance()
            .get(&GovernanceDataKey::ProposalIds)
            .ok_or(GovernanceError::NotInitialized)
    }

    fn get_proposal(env: &Env, proposal_id: u64) -> Result<Proposal, GovernanceError> {
        env.storage()
            .instance()
            .get(&GovernanceDataKey::Proposal(proposal_id))
            .ok_or(GovernanceError::ProposalNotFound)
    }

    fn get_governance_token(env: &Env) -> Result<Address, GovernanceError> {
        env.storage()
            .instance()
            .get(&GovernanceDataKey::GovernanceToken)
            .ok_or(GovernanceError::NotInitialized)
    }

    fn get_voting_threshold(env: &Env) -> Result<i128, GovernanceError> {
        env.storage()
            .instance()
            .get(&GovernanceDataKey::VotingThreshold)
            .ok_or(GovernanceError::NotInitialized)
    }

    fn get_quorum_threshold(env: &Env) -> Result<i128, GovernanceError> {
        env.storage()
            .instance()
            .get(&GovernanceDataKey::QuorumThreshold)
            .ok_or(GovernanceError::NotInitialized)
    }

    fn get_stake_token(env: &Env) -> Result<Address, GovernanceError> {
        env.storage()
            .instance()
            .get(&GovernanceDataKey::StakeToken)
            .ok_or(GovernanceError::NotInitialized)
    }

    fn get_proposal_stake_amount(env: &Env) -> Result<i128, GovernanceError> {
        env.storage()
            .instance()
            .get(&GovernanceDataKey::ProposalStakeAmount)
            .ok_or(GovernanceError::NotInitialized)
    }

    // View functions
    pub fn get_proposal_info(env: Env, proposal_id: u64) -> Result<Proposal, GovernanceError> {
        Self::get_proposal(&env, proposal_id)
    }

    pub fn get_voter_power(env: Env, voter: Address) -> Result<i128, GovernanceError> {
        Self::calculate_voting_power(&env, &voter)
    }

    pub fn get_vote_info(
        env: Env,
        voter: Address,
        proposal_id: u64
    ) -> Result<Vote, GovernanceError> {
        env.storage()
            .instance()
            .get(&GovernanceDataKey::Vote(voter, proposal_id))
            .ok_or(GovernanceError::ProposalNotFound)
    }
}
