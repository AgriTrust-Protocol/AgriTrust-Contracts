use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, Map, Symbol, Vec, String, i128, u128,
};

// --- Quadratic Voting Constants ---
pub const QV_PRECISION: u128 = 1_000_000; // 6 decimal places for quadratic calculations
pub const MAX_VOTES_PER_PROPOSAL: u128 = 1000; // Maximum votes per proposal per address
pub const VOTING_DURATION: u64 = 7 * 24 * 60 * 60; // 7 days voting period
pub const MIN_PARTICIPATION_THRESHOLD: u32 = 1000; // 10% minimum participation (basis points)

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Proposal {
    pub id: u64,
    pub grant_amount: i128,
    pub description: String,
    pub proposer: Address,
    pub votes_received: u128,
    pub unique_voters: u32,
    pub total_cost: u128,
    pub deadline: u64,
    pub status: ProposalStatus,
    pub created_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct VoteRecord {
    pub voter: Address,
    pub proposal_id: u64,
    pub votes: u128,
    pub cost: u128,
    pub timestamp: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct GrantAllocation {
    pub proposal_id: u64,
    pub amount: i128,
    pub votes: u128,
    pub unique_voters: u32,
    pub quadratic_weight: u128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct VotingStats {
    pub total_proposals: u64,
    pub total_voters: u32,
    pub total_votes_cast: u128,
    pub total_tokens_spent: u128,
    pub last_updated: u64,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[contracterror]
pub enum QuadraticVotingError {
    InvalidAmount = 1,
    InsufficientTokens = 2,
    MathOverflow = 3,
    ProposalNotFound = 4,
    VotingEnded = 5,
    TooManyVotes = 6,
    AlreadyVoted = 7,
    InvalidProposal = 8,
    NotInitialized = 9,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum ProposalStatus {
    Active,
    Approved,
    Rejected,
    Expired,
}

/// Quadratic Voting Module for Community Grant Allocation
pub struct QuadraticVoting;

#[contractimpl]
impl QuadraticVoting {
    /// Initialize quadratic voting system
    pub fn initialize_voting(env: Env, admin: Address, voting_token: Address) -> Result<(), QuadraticVotingError> {
        admin.require_auth();
        
        let config_key = Symbol::new(&env, "qv_config");
        let config = VotingConfig {
            admin,
            voting_token,
            precision: QV_PRECISION,
            max_votes_per_proposal: MAX_VOTES_PER_PROPOSAL,
            voting_duration: VOTING_DURATION,
            min_participation_threshold: MIN_PARTICIPATION_THRESHOLD,
        };
        
        env.storage().instance().set(&config_key, config);
        
        // Initialize stats
        let stats_key = Symbol::new(&env, "voting_stats");
        let stats = VotingStats {
            total_proposals: 0,
            total_voters: 0,
            total_votes_cast: 0,
            total_tokens_spent: 0,
            last_updated: env.ledger().timestamp(),
        };
        
        env.storage().instance().set(&stats_key, stats);
        
        Ok(())
    }
    
    /// Create a new grant proposal
    pub fn create_proposal(
        env: Env,
        proposer: Address,
        grant_amount: i128,
        description: String,
    ) -> Result<u64, QuadraticVotingError> {
        if grant_amount <= 0 {
            return Err(QuadraticVotingError::InvalidAmount);
        }
        
        if description.len() as u32 > 1000 {
            return Err(QuadraticVotingError::InvalidProposal);
        }
        
        let config = Self::get_voting_config(&env)?;
        
        // Get next proposal ID
        let proposals_key = Symbol::new(&env, "proposals");
        let next_id = Self::get_next_proposal_id(&env)?;
        
        let proposal = Proposal {
            id: next_id,
            grant_amount,
            description,
            proposer: proposer.clone(),
            votes_received: 0,
            unique_voters: 0,
            total_cost: 0,
            deadline: env.ledger().timestamp() + config.voting_duration,
            status: ProposalStatus::Active,
            created_at: env.ledger().timestamp(),
        };
        
        // Store proposal
        let mut proposals = Self::get_proposals(&env)?;
        proposals.push_back(proposal);
        env.storage().instance().set(&proposals_key, proposals);
        
        // Update stats
        Self::update_stats(&env, |stats| {
            stats.total_proposals += 1;
        })?;
        
        Ok(next_id)
    }
    
    /// Cast votes for a proposal
    pub fn cast_vote(
        env: Env,
        voter: Address,
        proposal_id: u64,
        votes: u128,
    ) -> Result<(), QuadraticVotingError> {
        if votes == 0 {
            return Err(QuadraticVotingError::InvalidAmount);
        }
        
        let config = Self::get_voting_config(&env)?;
        
        // Check if proposal exists and is active
        let proposal = Self::get_proposal(&env, proposal_id)?;
        if proposal.status != ProposalStatus::Active {
            return Err(QuadraticVotingError::ProposalNotFound);
        }
        
        if env.ledger().timestamp() > proposal.deadline {
            return Err(QuadraticVotingError::VotingEnded);
        }
        
        if votes > config.max_votes_per_proposal {
            return Err(QuadraticVotingError::TooManyVotes);
        }
        
        // Calculate cost using quadratic function: cost = votes²
        let cost = Self::calculate_vote_cost(votes)?;
        
        // Check voter's token balance
        let token_client = soroban_sdk::token::Client::new(&env, &config.voting_token);
        let voter_balance = token_client.balance(&voter);
        
        let already_spent = Self::get_voter_tokens_spent(&env, voter.clone())?;
        if voter_balance < already_spent + cost as i128 {
            return Err(QuadraticVotingError::InsufficientTokens);
        }
        
        // Check if voter has already voted on this proposal
        let vote_key = Symbol::new(&env, &format!("vote_{}_{}", proposal_id, voter));
        if env.storage().instance().get::<Symbol, VoteRecord>(&vote_key).is_some() {
            return Err(QuadraticVotingError::AlreadyVoted);
        }
        
        // Create vote record
        let vote_record = VoteRecord {
            voter: voter.clone(),
            proposal_id,
            votes,
            cost,
            timestamp: env.ledger().timestamp(),
        };
        
        // Store vote record
        env.storage().instance().set(&vote_key, vote_record);
        
        // Update voter's total spent
        let spent_key = Symbol::new(&env, &format!("spent_{}", voter));
        env.storage().instance().set(&spent_key, already_spent + cost);
        
        // Update proposal
        Self::update_proposal(&env, proposal_id, |proposal| {
            let current_votes = Self::get_proposal_votes(&env, proposal_id, voter.clone()).unwrap_or(0);
            
            proposal.votes_received += votes;
            proposal.total_cost += cost;
            
            if current_votes == 0 {
                proposal.unique_voters += 1;
            }
        })?;
        
        // Update stats
        Self::update_stats(&env, |stats| {
            stats.total_votes_cast += votes;
            stats.total_tokens_spent += cost;
        })?;
        
        Ok(())
    }
    
    /// Allocate grants based on quadratic voting results
    pub fn allocate_grants(env: Env, admin: Address) -> Result<Vec<GrantAllocation>, QuadraticVotingError> {
        admin.require_auth();
        
        let config = Self::get_voting_config(&env)?;
        let proposals = Self::get_proposals(&env)?;
        let mut allocations = Vec::new(&env);
        
        // Filter active proposals that have ended
        let mut ended_proposals = Vec::new(&env);
        let current_time = env.ledger().timestamp();
        
        for proposal in proposals.iter() {
            if proposal.status == ProposalStatus::Active && current_time > proposal.deadline {
                ended_proposals.push_back(proposal.clone());
            }
        }
        
        if ended_proposals.is_empty() {
            return Ok(allocations);
        }
        
        // Calculate total quadratic weight
        let mut total_weight = 0u128;
        for proposal in ended_proposals.iter() {
            let weight = Self::calculate_quadratic_weight(proposal.votes_received);
            total_weight += weight;
        }
        
        // Calculate total pool (this would come from treasury in real implementation)
        let total_pool = Self::get_available_grant_pool(&env)?;
        
        // Allocate grants based on quadratic weight
        for proposal in ended_proposals.iter() {
            if proposal.votes_received == 0 {
                continue;
            }
            
            let quadratic_weight = Self::calculate_quadratic_weight(proposal.votes_received);
            let allocation_ratio = if total_weight > 0 {
                (quadratic_weight * QV_PRECISION) / total_weight
            } else {
                0
            };
            
            let grant_amount = (total_pool as u128 * allocation_ratio) / QV_PRECISION;
            
            let allocation = GrantAllocation {
                proposal_id: proposal.id,
                amount: grant_amount as i128,
                votes: proposal.votes_received,
                unique_voters: proposal.unique_voters,
                quadratic_weight,
            };
            
            allocations.push_back(allocation);
            
            // Update proposal status
            Self::update_proposal(&env, proposal.id, |proposal| {
                proposal.status = if grant_amount > 0 {
                    ProposalStatus::Approved
                } else {
                    ProposalStatus::Rejected
                };
            })?;
        }
        
        Ok(allocations)
    }
    
    /// Get voter's current votes on a proposal
    pub fn get_voter_votes(env: Env, voter: Address, proposal_id: u64) -> Result<u128, QuadraticVotingError> {
        let vote_key = Symbol::new(&env, &format!("vote_{}_{}", proposal_id, voter));
        if let Some(vote_record) = env.storage().instance().get::<Symbol, VoteRecord>(&vote_key) {
            Ok(vote_record.votes)
        } else {
            Ok(0)
        }
    }
    
    /// Get cost for a given number of votes
    pub fn calculate_vote_cost(votes: u128) -> Result<u128, QuadraticVotingError> {
        // Cost = votes² using fixed-point arithmetic
        let votes_scaled = votes * QV_PRECISION;
        let cost_scaled = votes_scaled * votes_scaled;
        let cost = cost_scaled / QV_PRECISION;
        
        Ok(cost)
    }
    
    /// Calculate votes from cost (inverse function)
    pub fn calculate_votes_from_cost(cost: u128) -> Result<u128, QuadraticVotingError> {
        // votes = √cost using approximation
        let cost_scaled = cost * QV_PRECISION;
        let votes_scaled = Self::integer_sqrt(cost_scaled)?;
        let votes = votes_scaled / QV_PRECISION;
        
        Ok(votes)
    }
    
    /// Get quadratic weight for vote counting
    fn calculate_quadratic_weight(votes: u128) -> u128 {
        // Weight = votes² for quadratic voting
        votes * votes
    }
    
    /// Integer square root approximation
    fn integer_sqrt(n: u128) -> Result<u128, QuadraticVotingError> {
        if n == 0 {
            return Ok(0);
        }
        
        // Newton's method for integer square root
        let mut x = n;
        let mut y = (x + 1) / 2;
        
        while y < x {
            x = y;
            y = (x + n / x) / 2;
        }
        
        Ok(x)
    }
    
    /// Get proposal by ID
    fn get_proposal(env: &Env, proposal_id: u64) -> Result<Proposal, QuadraticVotingError> {
        let proposals = Self::get_proposals(env)?;
        
        for proposal in proposals.iter() {
            if proposal.id == proposal_id {
                return Ok(proposal);
            }
        }
        
        Err(QuadraticVotingError::ProposalNotFound)
    }
    
    /// Get all proposals
    fn get_proposals(env: &Env) -> Result<Vec<Proposal>, QuadraticVotingError> {
        let proposals_key = Symbol::new(env, "proposals");
        Ok(env.storage().instance().get(&proposals_key).unwrap_or_else(|| Vec::new(env)))
    }
    
    /// Update proposal
    fn update_proposal<F>(env: &Env, proposal_id: u64, updater: F) -> Result<(), QuadraticVotingError>
    where
        F: FnOnce(&mut Proposal),
    {
        let proposals_key = Symbol::new(env, "proposals");
        let mut proposals = Self::get_proposals(env)?;
        
        // Find and update the proposal
        for i in 0..proposals.len() {
            if proposals.get(i).unwrap().id == proposal_id {
                let mut proposal = proposals.get(i).unwrap();
                updater(&mut proposal);
                proposals.set(i, proposal);
                env.storage().instance().set(&proposals_key, proposals);
                return Ok(());
            }
        }
        
        Err(QuadraticVotingError::ProposalNotFound)
    }
    
    /// Get next proposal ID
    fn get_next_proposal_id(env: &Env) -> Result<u64, QuadraticVotingError> {
        let proposals = Self::get_proposals(env)?;
        
        if proposals.is_empty() {
            Ok(1)
        } else {
            let mut max_id = 0u64;
            for proposal in proposals.iter() {
                if proposal.id > max_id {
                    max_id = proposal.id;
                }
            }
            Ok(max_id + 1)
        }
    }
    
    /// Get voter's tokens spent
    fn get_voter_tokens_spent(env: &Env, voter: Address) -> Result<u128, QuadraticVotingError> {
        let spent_key = Symbol::new(env, &format!("spent_{}", voter));
        Ok(env.storage().instance().get(&spent_key).unwrap_or(0))
    }
    
    /// Get proposal votes for a specific voter
    fn get_proposal_votes(env: &Env, proposal_id: u64, voter: Address) -> Result<u128, QuadraticVotingError> {
        let vote_key = Symbol::new(env, &format!("vote_{}_{}", proposal_id, voter));
        if let Some(vote_record) = env.storage().instance().get::<Symbol, VoteRecord>(&vote_key) {
            Ok(vote_record.votes)
        } else {
            Ok(0)
        }
    }
    
    /// Get voting configuration
    fn get_voting_config(env: &Env) -> Result<VotingConfig, QuadraticVotingError> {
        let config_key = Symbol::new(env, "qv_config");
        env.storage().instance()
            .get(&config_key)
            .ok_or(QuadraticVotingError::NotInitialized)
    }
    
    /// Update voting statistics
    fn update_stats<F>(env: &Env, updater: F) -> Result<(), QuadraticVotingError>
    where
        F: FnOnce(&mut VotingStats),
    {
        let stats_key = Symbol::new(env, "voting_stats");
        let mut stats = env.storage().instance()
            .get(&stats_key)
            .ok_or(QuadraticVotingError::NotInitialized)?;
        
        updater(&mut stats);
        stats.last_updated = env.ledger().timestamp();
        
        env.storage().instance().set(&stats_key, stats);
        Ok(())
    }
    
    /// Get available grant pool (simplified for demo)
    fn get_available_grant_pool(env: &Env) -> Result<i128, QuadraticVotingError> {
        // In a real implementation, this would query the treasury contract
        // For now, return a fixed amount
        Ok(1_000_000_000) // 1000 tokens (assuming 7 decimals)
    }
    
    /// Get voting statistics
    pub fn get_voting_stats(env: Env) -> Result<VotingStats, QuadraticVotingError> {
        let stats_key = Symbol::new(&env, "voting_stats");
        env.storage().instance()
            .get(&stats_key)
            .ok_or(QuadraticVotingError::NotInitialized)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct VotingConfig {
    pub admin: Address,
    pub voting_token: Address,
    pub precision: u128,
    pub max_votes_per_proposal: u128,
    pub voting_duration: u64,
    pub min_participation_threshold: u32,
}
