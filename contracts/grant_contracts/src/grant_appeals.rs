#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Vec,
    Map, String,
};

use super::{Error, GrantStatus};

// --- Constants for Time-Weighted Voting ---
const APPEAL_VOTING_PERIOD: u64 = 7 * 24 * 60 * 60; // 7 days voting period
const MIN_APPEAL_PARTICIPATION: u32 = 1000; // 10% minimum participation (in basis points)
const APPEAL_APPROVAL_THRESHOLD: u32 = 6600; // 66% approval required (in basis points)
const MAX_APPEAL_REASON_LENGTH: u32 = 1000; // Maximum appeal reason length
const TIME_WEIGHT_MULTIPLIER: u64 = 86400; // 1 day in seconds for time weighting

// Time weighting factors (in basis points)
const TIME_WEIGHT_BRACKETS: [(u64, u32); 5] = [
    (30 * 86400, 5000),    // 30+ days: 50% weight
    (90 * 86400, 7500),    // 90+ days: 75% weight  
    (180 * 86400, 9000),   // 180+ days: 90% weight
    (365 * 86400, 10000),  // 365+ days: 100% weight (full)
    (730 * 86400, 12000),  // 730+ days: 120% weight (bonus)
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum AppealStatus {
    Proposed,     // Appeal created, voting open
    Approved,     // Appeal approved, grant reinstated
    Rejected,     // Appeal rejected by vote
    Executed,     // Appeal successfully executed
    Expired,      // Voting period expired
}

#[derive(Clone)]
#[contracttype]
pub struct GrantAppeal {
    pub appeal_id: u64,
    pub grant_id: u64,
    pub appellant: Address,
    pub reason: String,
    pub evidence_hash: [u8; 32], // Hash of evidence documents
    pub created_at: u64,
    pub voting_deadline: u64,
    pub status: AppealStatus,
    pub votes_for: i128,       // Total time-weighted voting power in favor
    pub votes_against: i128,   // Total time-weighted voting power against
    pub total_eligible_power: i128, // Total eligible time-weighted voting power
    pub executed_at: Option<u64>, // When appeal was executed
}

#[derive(Clone)]
#[contracttype]
pub struct TimeWeightedVote {
    pub voter: Address,
    pub appeal_id: u64,
    pub base_voting_power: i128,     // Raw token balance
    pub time_multiplier: u32,        // Time-based multiplier (in basis points)
    pub weighted_power: i128,         // Final time-weighted voting power
    pub vote: bool,                  // true = for, false = against
    pub voted_at: u64,
    pub token_holding_duration: u64, // How long tokens have been held
}

#[derive(Clone)]
#[contracttype]
pub struct TokenHoldingInfo {
    pub address: Address,
    pub balance: i128,
    pub first_acquired: u64,  // When tokens were first acquired
    pub last_updated: u64,
}

#[derive(Clone)]
#[contracttype]
pub enum AppealDataKey {
    Appeal(u64),
    TimeWeightedVote(Address, u64),
    TokenHoldingInfo(Address),
    AppealIds,
    GrantAppeals(u64), // Maps grant_id to list of appeal IDs
    NextAppealId,
    GovernanceToken,
    AppealVotingPower(Address), // Cached time-weighted voting power
}

#[contracterror]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum AppealError {
    NotInitialized = 1001,
    GrantNotFound = 1002,
    GrantNotCancelled = 1003,
    AppealAlreadyExists = 1004,
    InvalidAppealStatus = 1005,
    VotingPeriodEnded = 1006,
    VotingPeriodActive = 1007,
    AlreadyVoted = 1008,
    InsufficientVotingPower = 1009,
    ParticipationThresholdNotMet = 1010,
    ApprovalThresholdNotMet = 1011,
    AppealNotFound = 1012,
    InvalidReasonLength = 1013,
    NotAuthorized = 1014,
    MathOverflow = 1015,
}

pub struct GrantAppealContract;

#[contractimpl]
impl GrantAppealContract {
    /// Initialize the appeal system
    pub fn initialize(env: Env, governance_token: Address) -> Result<(), AppealError> {
        if env.storage().instance().has(&AppealDataKey::GovernanceToken) {
            return Err(AppealError::NotInitialized);
        }

        env.storage().instance().set(&AppealDataKey::GovernanceToken, &governance_token);
        env.storage().instance().set(&AppealDataKey::AppealIds, &Vec::<u64>::new(&env));
        env.storage().instance().set(&AppealDataKey::NextAppealId, &1u64);

        Ok(())
    }

    /// Create an appeal for a cancelled grant
    pub fn create_appeal(
        env: Env,
        grant_id: u64,
        appellant: Address,
        reason: String,
        evidence: String,
    ) -> Result<u64, AppealError> {
        appellant.require_auth();

        // Validate reason length
        if reason.len() > MAX_APPEAL_REASON_LENGTH as usize {
            return Err(AppealError::InvalidReasonLength);
        }

        // Check if grant exists and is cancelled (would need to integrate with main grant contract)
        // For now, we'll assume grant validation happens elsewhere
        
        let mut appeal_ids = Self::get_appeal_ids(&env)?;
        let appeal_id = Self::get_next_appeal_id(&env)?;

        // Check if appeal already exists for this grant
        let grant_appeals = Self::get_grant_appeals(&env, grant_id)?;
        for existing_appeal_id in grant_appeals.iter() {
            let existing_appeal = Self::get_appeal(&env, existing_appeal_id)?;
            if matches!(existing_appeal.status, AppealStatus::Proposed | AppealStatus::Approved) {
                return Err(AppealError::AppealAlreadyExists);
            }
        }

        let now = env.ledger().timestamp();
        let voting_deadline = now
            .checked_add(APPEAL_VOTING_PERIOD)
            .ok_or(AppealError::MathOverflow)?;

        let evidence_hash = Self::generate_evidence_hash(&evidence);

        let appeal = GrantAppeal {
            appeal_id,
            grant_id,
            appellant: appellant.clone(),
            reason,
            evidence_hash,
            created_at: now,
            voting_deadline,
            status: AppealStatus::Proposed,
            votes_for: 0,
            votes_against: 0,
            total_eligible_power: 0,
            executed_at: None,
        };

        // Store the appeal
        env.storage().instance().set(&AppealDataKey::Appeal(appeal_id), &appeal);
        appeal_ids.push_back(appeal_id);
        env.storage().instance().set(&AppealDataKey::AppealIds, &appeal_ids);

        // Update grant appeals mapping
        let mut grant_appeal_list = Self::get_grant_appeals(&env, grant_id)?;
        grant_appeal_list.push_back(appeal_id);
        env.storage().instance().set(&AppealDataKey::GrantAppeals(grant_id), &grant_appeal_list);

        // Update next appeal ID
        env.storage().instance().set(&AppealDataKey::NextAppealId, &(appeal_id + 1));

        env.events().publish(
            (symbol_short!("appeal_created"),),
            (appeal_id, grant_id, appellant, voting_deadline),
        );

        Ok(appeal_id)
    }

    /// Vote on an appeal with time-weighted voting power
    pub fn vote_on_appeal(
        env: Env,
        voter: Address,
        appeal_id: u64,
        vote: bool, // true = for appeal, false = against appeal
    ) -> Result<(), AppealError> {
        voter.require_auth();

        let mut appeal = Self::get_appeal(&env, appeal_id)?;
        let now = env.ledger().timestamp();

        // Check voting period
        if now >= appeal.voting_deadline {
            return Err(AppealError::VotingPeriodEnded);
        }

        if appeal.status != AppealStatus::Proposed {
            return Err(AppealError::InvalidAppealStatus);
        }

        // Check if already voted
        if env.storage().instance().has(&AppealDataKey::TimeWeightedVote(voter.clone(), appeal_id)) {
            return Err(AppealError::AlreadyVoted);
        }

        // Calculate time-weighted voting power
        let (base_power, time_multiplier, weighted_power, holding_duration) = 
            Self::calculate_time_weighted_voting_power(&env, &voter)?;

        if weighted_power <= 0 {
            return Err(AppealError::InsufficientVotingPower);
        }

        // Record the vote
        let time_weighted_vote = TimeWeightedVote {
            voter: voter.clone(),
            appeal_id,
            base_voting_power: base_power,
            time_multiplier,
            weighted_power,
            vote,
            voted_at: now,
            token_holding_duration: holding_duration,
        };

        env.storage().instance().set(
            &AppealDataKey::TimeWeightedVote(voter.clone(), appeal_id),
            &time_weighted_vote,
        );

        // Update appeal tallies
        if vote {
            appeal.votes_for = appeal.votes_for
                .checked_add(weighted_power)
                .ok_or(AppealError::MathOverflow)?;
        } else {
            appeal.votes_against = appeal.votes_against
                .checked_add(weighted_power)
                .ok_or(AppealError::MathOverflow)?;
        }

        appeal.total_eligible_power = appeal.total_eligible_power
            .checked_add(weighted_power)
            .ok_or(AppealError::MathOverflow)?;

        env.storage().instance().set(&AppealDataKey::Appeal(appeal_id), &appeal);

        env.events().publish(
            (symbol_short!("appeal_vote"),),
            (
                appeal_id,
                voter,
                vote,
                base_power,
                time_multiplier,
                weighted_power,
                holding_duration,
            ),
        );

        Ok(())
    }

    /// Execute an approved appeal (reinstates the grant)
    pub fn execute_appeal(
        env: Env,
        caller: Address,
        appeal_id: u64,
    ) -> Result<(), AppealError> {
        caller.require_auth();

        let mut appeal = Self::get_appeal(&env, appeal_id)?;
        let now = env.ledger().timestamp();

        // Check voting period has ended
        if now < appeal.voting_deadline {
            return Err(AppealError::VotingPeriodActive);
        }

        if appeal.status != AppealStatus::Proposed {
            return Err(AppealError::InvalidAppealStatus);
        }

        // Calculate voting results
        let (participation_met, approval_met) = Self::calculate_appeal_results(&appeal);

        if !participation_met {
            appeal.status = AppealStatus::Expired;
            env.storage().instance().set(&AppealDataKey::Appeal(appeal_id), &appeal);
            return Err(AppealError::ParticipationThresholdNotMet);
        }

        if !approval_met {
            appeal.status = AppealStatus::Rejected;
            env.storage().instance().set(&AppealDataKey::Appeal(appeal_id), &appeal);
            return Err(AppealError::ApprovalThresholdNotMet);
        }

        // Appeal approved - execute (would integrate with main grant contract to reinstate)
        appeal.status = AppealStatus::Approved;
        appeal.executed_at = Some(now);
        env.storage().instance().set(&AppealDataKey::Appeal(appeal_id), &appeal);

        env.events().publish(
            (symbol_short!("appeal_executed"),),
            (
                appeal_id,
                appeal.grant_id,
                appeal.votes_for,
                appeal.votes_against,
                appeal.total_eligible_power,
            ),
        );

        Ok(())
    }

    /// Calculate time-weighted voting power based on token holding duration
    fn calculate_time_weighted_voting_power(
        env: &Env,
        voter: &Address,
    ) -> Result<(i128, u32, i128, u64), AppealError> {
        let governance_token = Self::get_governance_token(env)?;
        let token_client = token::Client::new(env, &governance_token);
        
        let balance = token_client.balance(voter);
        if balance <= 0 {
            return Ok((0, 0, 0, 0));
        }

        // Get or update token holding info
        let holding_info = Self::get_or_update_holding_info(env, voter, balance)?;
        
        let holding_duration = env.ledger().timestamp()
            .checked_sub(holding_info.first_acquired)
            .unwrap_or(0);

        // Find appropriate time multiplier based on holding duration
        let time_multiplier = Self::get_time_multiplier(holding_duration);

        // Calculate weighted power: base_power * time_multiplier / 10000
        let weighted_power = balance
            .checked_mul(time_multiplier as i128)
            .ok_or(AppealError::MathOverflow)?
            .checked_div(10000)
            .ok_or(AppealError::MathOverflow)?;

        // Cache the voting power for efficiency
        env.storage().instance().set(
            &AppealDataKey::AppealVotingPower(voter.clone()),
            &weighted_power,
        );

        Ok((balance, time_multiplier, weighted_power, holding_duration))
    }

    /// Get time multiplier based on holding duration
    fn get_time_multiplier(holding_duration: u64) -> u32 {
        for (days_required, multiplier) in TIME_WEIGHT_BRACKETS.iter() {
            if holding_duration >= *days_required {
                return *multiplier;
            }
        }
        2500 // Default: 25% for new holders (< 30 days)
    }

    /// Get or update token holding information
    fn get_or_update_holding_info(
        env: &Env,
        voter: &Address,
        current_balance: i128,
    ) -> Result<TokenHoldingInfo, AppealError> {
        let now = env.ledger().timestamp();
        
        if let Some(mut holding_info) = env.storage().instance()
            .get(&AppealDataKey::TokenHoldingInfo(voter.clone())) 
        {
            // Update existing holding info
            holding_info.balance = current_balance;
            holding_info.last_updated = now;
            
            // If balance went to 0 and now > 0, reset first_acquired
            if holding_info.balance == 0 && current_balance > 0 {
                holding_info.first_acquired = now;
            }
            
            env.storage().instance().set(
                &AppealDataKey::TokenHoldingInfo(voter.clone()),
                &holding_info,
            );
            
            Ok(holding_info)
        } else {
            // Create new holding info
            let holding_info = TokenHoldingInfo {
                address: voter.clone(),
                balance: current_balance,
                first_acquired: now,
                last_updated: now,
            };
            
            env.storage().instance().set(
                &AppealDataKey::TokenHoldingInfo(voter.clone()),
                &holding_info,
            );
            
            Ok(holding_info)
        }
    }

    /// Calculate appeal voting results
    fn calculate_appeal_results(appeal: &GrantAppeal) -> (bool, bool) {
        let total_power = appeal.total_eligible_power;
        let votes_cast = appeal.votes_for.checked_add(appeal.votes_against).unwrap_or(0);
        
        // Check minimum participation (10%)
        let participation_met = if total_power > 0 {
            (votes_cast.checked_mul(10000).unwrap_or(0) / total_power) >= MIN_APPEAL_PARTICIPATION as i128
        } else {
            false
        };
        
        // Check approval threshold (66%)
        let approval_met = if votes_cast > 0 {
            (appeal.votes_for.checked_mul(10000).unwrap_or(0) / votes_cast) >= APPEAL_APPROVAL_THRESHOLD as i128
        } else {
            false
        };
        
        (participation_met, approval_met)
    }

    /// Generate evidence hash
    fn generate_evidence_hash(evidence: &String) -> [u8; 32] {
        let mut hash = [0u8; 32];
        
        // Simple hash implementation for demonstration
        for i in 0..32.min(evidence.len()) {
            hash[i] = evidence.as_bytes()[i];
        }
        
        hash
    }

    // --- Helper functions ---

    fn get_appeal_ids(env: &Env) -> Result<Vec<u64>, AppealError> {
        env.storage()
            .instance()
            .get(&AppealDataKey::AppealIds)
            .ok_or(AppealError::NotInitialized)
    }

    fn get_next_appeal_id(env: &Env) -> Result<u64, AppealError> {
        env.storage()
            .instance()
            .get(&AppealDataKey::NextAppealId)
            .ok_or(AppealError::NotInitialized)
    }

    fn get_appeal(env: &Env, appeal_id: u64) -> Result<GrantAppeal, AppealError> {
        env.storage()
            .instance()
            .get(&AppealDataKey::Appeal(appeal_id))
            .ok_or(AppealError::AppealNotFound)
    }

    fn get_grant_appeals(env: &Env, grant_id: u64) -> Result<Vec<u64>, AppealError> {
        env.storage()
            .instance()
            .get(&AppealDataKey::GrantAppeals(grant_id))
            .unwrap_or_else(|| Vec::new(env))
    }

    fn get_governance_token(env: &Env) -> Result<Address, AppealError> {
        env.storage()
            .instance()
            .get(&AppealDataKey::GovernanceToken)
            .ok_or(AppealError::NotInitialized)
    }

    // --- View functions ---

    pub fn get_appeal_info(env: Env, appeal_id: u64) -> Result<GrantAppeal, AppealError> {
        Self::get_appeal(&env, appeal_id)
    }

    pub fn get_vote_info(
        env: Env,
        voter: Address,
        appeal_id: u64,
    ) -> Result<TimeWeightedVote, AppealError> {
        env.storage()
            .instance()
            .get(&AppealDataKey::TimeWeightedVote(voter, appeal_id))
            .ok_or(AppealError::AppealNotFound)
    }

    pub fn get_time_weighted_voting_power(
        env: Env,
        voter: Address,
    ) -> Result<(i128, u32, u64), AppealError> {
        let (base_power, time_multiplier, weighted_power, holding_duration) = 
            Self::calculate_time_weighted_voting_power(&env, &voter)?;
        
        Ok((base_power, time_multiplier, holding_duration))
    }

    pub fn get_grant_appeals(env: Env, grant_id: u64) -> Result<Vec<u64>, AppealError> {
        Self::get_grant_appeals(&env, grant_id)
    }

    pub fn get_all_appeals(env: Env) -> Result<Vec<u64>, AppealError> {
        Self::get_appeal_ids(&env)
    }
}
