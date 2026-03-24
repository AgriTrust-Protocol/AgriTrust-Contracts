#![allow(unexpected_cfgs)]
#![no_std]

use core::cmp::min;

use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, token, Address, Env, Map, String,
    Symbol, Vec,
};

#[contract]
pub struct GrantContract;

#[contracttype]
pub enum DataKey {
    Grant(Symbol),
    Milestone(Symbol, Symbol),
    MilestoneVote(Symbol, Symbol, Address),
    Withdrawn(Symbol, Address),
}

#[derive(Clone)]
#[contracttype]
pub struct Grant {
    pub admin: Address,
    pub grantees: Map<Address, u32>,
    pub total_amount: u128,
    pub released_amount: u128,
    pub token_address: Address,
    pub created_at: u64,
    pub cliff_end: u64,
    pub stream_start: u64,
    pub stream_duration: u64,
    pub status: GrantStatus,
    pub council_members: Vec<Address>,
    pub voting_threshold: u32,
    pub acceleration_windows: Vec<StreamAcceleration>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum GrantStatus {
    Proposed,
    Active,
    Paused,
    Completed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct StreamAcceleration {
    pub milestone_id: Symbol,
    pub activated_at: u64,
    pub expires_at: u64,
    pub bonus_bps: u32,
}

#[derive(Clone)]
#[contracttype]
pub struct Milestone {
    pub amount: u128,
    pub description: String,
    pub approved: bool,
    pub approved_at: Option<u64>,
    pub votes_for: u32,
    pub votes_against: u32,
    pub voting_deadline: u64,
    pub acceleration_bps: u32,
    pub acceleration_duration: u64,
}

#[contracttype]
pub enum GrantError {
    GrantNotFound,
    Unauthorized,
    InvalidAmount,
    MilestoneNotFound,
    AlreadyApproved,
    ExceedsTotalAmount,
    InvalidStatus,
    InvalidShares,
    NotCouncilMember,
    AlreadyVoted,
    VotingExpired,
    InvalidGrantee,
    InvalidStreamConfig,
    InvalidAccelerationConfig,
}

impl From<GrantError> for soroban_sdk::Error {
    fn from(error: GrantError) -> Self {
        match error {
            GrantError::GrantNotFound => soroban_sdk::Error::from_contract_error(1),
            GrantError::Unauthorized => soroban_sdk::Error::from_contract_error(2),
            GrantError::InvalidAmount => soroban_sdk::Error::from_contract_error(3),
            GrantError::MilestoneNotFound => soroban_sdk::Error::from_contract_error(4),
            GrantError::AlreadyApproved => soroban_sdk::Error::from_contract_error(5),
            GrantError::ExceedsTotalAmount => soroban_sdk::Error::from_contract_error(6),
            GrantError::InvalidStatus => soroban_sdk::Error::from_contract_error(7),
            GrantError::InvalidShares => soroban_sdk::Error::from_contract_error(8),
            GrantError::NotCouncilMember => soroban_sdk::Error::from_contract_error(9),
            GrantError::AlreadyVoted => soroban_sdk::Error::from_contract_error(10),
            GrantError::VotingExpired => soroban_sdk::Error::from_contract_error(11),
            GrantError::InvalidGrantee => soroban_sdk::Error::from_contract_error(12),
            GrantError::InvalidStreamConfig => soroban_sdk::Error::from_contract_error(13),
            GrantError::InvalidAccelerationConfig => soroban_sdk::Error::from_contract_error(14),
        }
    }
}

#[contractimpl]
impl GrantContract {
    pub fn create_grant(
        env: Env,
        grant_id: Symbol,
        admin: Address,
        grantees: Map<Address, u32>,
        total_amount: u128,
        token_address: Address,
        cliff_end: u64,
        council_members: Vec<Address>,
        voting_threshold: u32,
    ) {
        admin.require_auth();

        if total_amount == 0 {
            panic_with_error!(&env, GrantError::InvalidAmount);
        }

        let mut total_shares = 0u32;
        for (_, share) in grantees.iter() {
            total_shares = total_shares.saturating_add(share);
        }
        if total_shares != 10_000 {
            panic_with_error!(&env, GrantError::InvalidShares);
        }

        if voting_threshold == 0 || voting_threshold > council_members.len() {
            panic_with_error!(&env, GrantError::InvalidAmount);
        }

        let created_at = env.ledger().timestamp();
        let grant = Grant {
            admin,
            grantees,
            total_amount,
            released_amount: 0,
            token_address,
            created_at,
            cliff_end,
            stream_start: created_at,
            stream_duration: 0,
            status: GrantStatus::Proposed,
            council_members,
            voting_threshold,
            acceleration_windows: Vec::new(&env),
        };

        env.storage()
            .instance()
            .set(&DataKey::Grant(grant_id), &grant);
    }

    pub fn configure_stream(env: Env, grant_id: Symbol, stream_start: u64, stream_duration: u64) {
        let mut grant = Self::load_grant(&env, &grant_id);
        grant.admin.require_auth();

        if stream_duration == 0 {
            panic_with_error!(&env, GrantError::InvalidStreamConfig);
        }

        grant.stream_start = stream_start;
        grant.stream_duration = stream_duration;
        env.storage()
            .instance()
            .set(&DataKey::Grant(grant_id), &grant);
    }

    pub fn add_milestone(
        env: Env,
        grant_id: Symbol,
        milestone_id: Symbol,
        amount: u128,
        description: String,
        voting_period: u64,
    ) {
        let grant = Self::load_grant(&env, &grant_id);
        grant.admin.require_auth();

        if amount == 0 || voting_period == 0 {
            panic_with_error!(&env, GrantError::InvalidAmount);
        }

        let milestone = Milestone {
            amount,
            description,
            approved: false,
            approved_at: None,
            votes_for: 0,
            votes_against: 0,
            voting_deadline: env.ledger().timestamp().saturating_add(voting_period),
            acceleration_bps: 0,
            acceleration_duration: 0,
        };

        env.storage()
            .instance()
            .set(&DataKey::Milestone(grant_id, milestone_id), &milestone);
    }

    pub fn configure_milestone_acceleration(
        env: Env,
        grant_id: Symbol,
        milestone_id: Symbol,
        acceleration_bps: u32,
        acceleration_duration: u64,
    ) {
        let grant = Self::load_grant(&env, &grant_id);
        grant.admin.require_auth();

        if acceleration_bps == 0 || acceleration_duration == 0 {
            panic_with_error!(&env, GrantError::InvalidAccelerationConfig);
        }

        let milestone_key = DataKey::Milestone(grant_id, milestone_id);
        let mut milestone = Self::load_milestone(&env, &milestone_key);
        if milestone.approved {
            panic_with_error!(&env, GrantError::AlreadyApproved);
        }

        milestone.acceleration_bps = acceleration_bps;
        milestone.acceleration_duration = acceleration_duration;
        env.storage().instance().set(&milestone_key, &milestone);
    }

    pub fn propose_milestone_approval(env: Env, grant_id: Symbol, milestone_id: Symbol) {
        let grant = Self::load_grant(&env, &grant_id);
        grant.admin.require_auth();

        let milestone_key = DataKey::Milestone(grant_id.clone(), milestone_id.clone());
        let mut milestone = Self::load_milestone(&env, &milestone_key);
        if milestone.approved {
            panic_with_error!(&env, GrantError::AlreadyApproved);
        }

        milestone.votes_for = 0;
        milestone.votes_against = 0;
        milestone.voting_deadline = env.ledger().timestamp().saturating_add(7 * 24 * 60 * 60);

        for member in grant.council_members.iter() {
            env.storage().instance().remove(&DataKey::MilestoneVote(
                grant_id.clone(),
                milestone_id.clone(),
                member,
            ));
        }

        env.storage().instance().set(&milestone_key, &milestone);
    }

    pub fn vote_milestone(
        env: Env,
        grant_id: Symbol,
        milestone_id: Symbol,
        voter: Address,
        approve: bool,
    ) {
        voter.require_auth();

        let mut grant = Self::load_grant(&env, &grant_id);
        let milestone_key = DataKey::Milestone(grant_id.clone(), milestone_id.clone());
        let mut milestone = Self::load_milestone(&env, &milestone_key);

        if milestone.approved {
            panic_with_error!(&env, GrantError::AlreadyApproved);
        }
        if env.ledger().timestamp() > milestone.voting_deadline {
            panic_with_error!(&env, GrantError::VotingExpired);
        }
        if !Self::is_council_member(&grant, &voter) {
            panic_with_error!(&env, GrantError::NotCouncilMember);
        }

        let vote_key = DataKey::MilestoneVote(grant_id.clone(), milestone_id.clone(), voter);
        if env.storage().instance().has(&vote_key) {
            panic_with_error!(&env, GrantError::AlreadyVoted);
        }
        env.storage().instance().set(&vote_key, &approve);

        if approve {
            milestone.votes_for = milestone.votes_for.saturating_add(1);
        } else {
            milestone.votes_against = milestone.votes_against.saturating_add(1);
        }

        if milestone.votes_for >= grant.voting_threshold {
            Self::finalize_milestone_approval(
                &env,
                &grant_id,
                &milestone_id,
                &mut grant,
                &mut milestone,
            );
        }

        env.storage().instance().set(&milestone_key, &milestone);
        env.storage()
            .instance()
            .set(&DataKey::Grant(grant_id), &grant);
    }

    pub fn approve_milestone(env: Env, grant_id: Symbol, milestone_id: Symbol) {
        let mut grant = Self::load_grant(&env, &grant_id);
        grant.admin.require_auth();

        let milestone_key = DataKey::Milestone(grant_id.clone(), milestone_id.clone());
        let mut milestone = Self::load_milestone(&env, &milestone_key);
        Self::finalize_milestone_approval(
            &env,
            &grant_id,
            &milestone_id,
            &mut grant,
            &mut milestone,
        );

        env.storage().instance().set(&milestone_key, &milestone);
        env.storage()
            .instance()
            .set(&DataKey::Grant(grant_id), &grant);
    }

    pub fn withdraw(env: Env, grant_id: Symbol, caller: Address) -> u128 {
        caller.require_auth();

        let grant = Self::load_grant(&env, &grant_id);
        let share = match grant.grantees.get(caller.clone()) {
            Some(share) => share,
            None => panic_with_error!(&env, GrantError::InvalidGrantee),
        };

        let available =
            Self::compute_withdrawable_amount(&env, &grant, &grant_id, caller.clone(), share);
        if available == 0 {
            return 0;
        }

        let withdrawn_key = DataKey::Withdrawn(grant_id, caller.clone());
        let already_withdrawn = env
            .storage()
            .instance()
            .get::<_, u128>(&withdrawn_key)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&withdrawn_key, &already_withdrawn.saturating_add(available));

        Self::transfer_tokens(
            &env,
            &grant.token_address,
            &env.current_contract_address(),
            &caller,
            available,
        );
        available
    }

    pub fn activate_grant(env: Env, grant_id: Symbol) {
        let mut grant = Self::load_grant(&env, &grant_id);
        grant.admin.require_auth();

        match grant.status {
            GrantStatus::Proposed => {
                grant.status = GrantStatus::Active;
                env.storage()
                    .instance()
                    .set(&DataKey::Grant(grant_id), &grant);
            }
            _ => panic_with_error!(&env, GrantError::InvalidStatus),
        }
    }

    pub fn pause_grant(env: Env, grant_id: Symbol) {
        let mut grant = Self::load_grant(&env, &grant_id);
        grant.admin.require_auth();

        match grant.status {
            GrantStatus::Active => {
                grant.status = GrantStatus::Paused;
                env.storage()
                    .instance()
                    .set(&DataKey::Grant(grant_id), &grant);
            }
            _ => panic_with_error!(&env, GrantError::InvalidStatus),
        }
    }

    pub fn resume_grant(env: Env, grant_id: Symbol) {
        let mut grant = Self::load_grant(&env, &grant_id);
        grant.admin.require_auth();

        match grant.status {
            GrantStatus::Paused => {
                grant.status = GrantStatus::Active;
                env.storage()
                    .instance()
                    .set(&DataKey::Grant(grant_id), &grant);
            }
            _ => panic_with_error!(&env, GrantError::InvalidStatus),
        }
    }

    pub fn cancel_grant(env: Env, grant_id: Symbol) {
        let mut grant = Self::load_grant(&env, &grant_id);
        grant.admin.require_auth();

        match grant.status {
            GrantStatus::Proposed | GrantStatus::Paused => {
                grant.status = GrantStatus::Cancelled;
                env.storage()
                    .instance()
                    .set(&DataKey::Grant(grant_id), &grant);
            }
            _ => panic_with_error!(&env, GrantError::InvalidStatus),
        }
    }

    pub fn get_grant(env: Env, grant_id: Symbol) -> Grant {
        Self::load_grant(&env, &grant_id)
    }

    pub fn get_milestone(env: Env, grant_id: Symbol, milestone_id: Symbol) -> Milestone {
        Self::load_milestone(&env, &DataKey::Milestone(grant_id, milestone_id))
    }

    pub fn get_withdrawable_amount(env: Env, grant_id: Symbol, caller: Address) -> u128 {
        let grant = Self::load_grant(&env, &grant_id);
        let share = match grant.grantees.get(caller.clone()) {
            Some(share) => share,
            None => return 0,
        };

        Self::compute_withdrawable_amount(&env, &grant, &grant_id, caller, share)
    }

    pub fn get_remaining_amount(env: Env, grant_id: Symbol) -> u128 {
        let grant = Self::load_grant(&env, &grant_id);
        grant.total_amount.saturating_sub(grant.released_amount)
    }

    fn finalize_milestone_approval(
        env: &Env,
        grant_id: &Symbol,
        milestone_id: &Symbol,
        grant: &mut Grant,
        milestone: &mut Milestone,
    ) {
        if milestone.approved {
            panic_with_error!(env, GrantError::AlreadyApproved);
        }
        match grant.status {
            GrantStatus::Cancelled | GrantStatus::Paused => {
                panic_with_error!(env, GrantError::InvalidStatus);
            }
            _ => {}
        }

        let new_released = grant
            .released_amount
            .checked_add(milestone.amount)
            .unwrap_or_else(|| panic_with_error!(env, GrantError::ExceedsTotalAmount));
        if new_released > grant.total_amount {
            panic_with_error!(env, GrantError::ExceedsTotalAmount);
        }

        milestone.approved = true;
        milestone.approved_at = Some(env.ledger().timestamp());
        grant.released_amount = new_released;

        if milestone.acceleration_bps > 0 && milestone.acceleration_duration > 0 {
            grant.acceleration_windows.push_back(StreamAcceleration {
                milestone_id: milestone_id.clone(),
                activated_at: env.ledger().timestamp(),
                expires_at: env
                    .ledger()
                    .timestamp()
                    .saturating_add(milestone.acceleration_duration),
                bonus_bps: milestone.acceleration_bps,
            });
        }

        if grant.released_amount == grant.total_amount {
            grant.status = GrantStatus::Completed;
        }

        Self::transfer_tokens(
            env,
            &grant.token_address,
            &grant.admin,
            &env.current_contract_address(),
            milestone.amount,
        );
        env.storage()
            .instance()
            .set(&DataKey::Grant(grant_id.clone()), grant);
    }

    fn compute_withdrawable_amount(
        env: &Env,
        grant: &Grant,
        grant_id: &Symbol,
        caller: Address,
        share: u32,
    ) -> u128 {
        let current_time = env.ledger().timestamp();
        if grant.cliff_end > 0 && current_time < grant.cliff_end {
            return 0;
        }
        match grant.status {
            GrantStatus::Proposed | GrantStatus::Paused | GrantStatus::Cancelled => return 0,
            _ => {}
        }

        let released_entitlement = grant.released_amount.saturating_mul(share as u128) / 10_000;
        let stream_limited_entitlement = if grant.stream_duration == 0 {
            released_entitlement
        } else {
            let total_entitlement = grant.total_amount.saturating_mul(share as u128) / 10_000;
            let streamed = grant::compute_accelerated_claimable_balance(
                total_entitlement,
                grant.stream_start,
                current_time,
                grant.stream_duration,
                &grant.acceleration_windows,
            );
            min(streamed, released_entitlement)
        };

        let withdrawn_key = DataKey::Withdrawn(grant_id.clone(), caller);
        let already_withdrawn = env
            .storage()
            .instance()
            .get::<_, u128>(&withdrawn_key)
            .unwrap_or(0);
        stream_limited_entitlement.saturating_sub(already_withdrawn)
    }

    fn load_grant(env: &Env, grant_id: &Symbol) -> Grant {
        env.storage()
            .instance()
            .get::<_, Grant>(&DataKey::Grant(grant_id.clone()))
            .unwrap_or_else(|| panic_with_error!(env, GrantError::GrantNotFound))
    }

    fn load_milestone(env: &Env, key: &DataKey) -> Milestone {
        env.storage()
            .instance()
            .get::<_, Milestone>(key)
            .unwrap_or_else(|| panic_with_error!(env, GrantError::MilestoneNotFound))
    }

    fn is_council_member(grant: &Grant, voter: &Address) -> bool {
        for member in grant.council_members.iter() {
            if member == *voter {
                return true;
            }
        }
        false
    }

    fn transfer_tokens(
        env: &Env,
        token_address: &Address,
        from: &Address,
        to: &Address,
        amount: u128,
    ) {
        token::Client::new(env, token_address).transfer(from, to, &(amount as i128));
    }
}

mod test;

pub mod grant {
    use core::cmp::{max, min};

    use soroban_sdk::Vec;

    use crate::StreamAcceleration;

    pub fn compute_claimable_balance(total: u128, start: u64, now: u64, duration: u64) -> u128 {
        if duration == 0 {
            return if now >= start { total } else { 0 };
        }
        if now <= start {
            return 0;
        }
        let elapsed = now.saturating_sub(start);
        if elapsed >= duration {
            return total;
        }

        let dur = duration as u128;
        let el = elapsed as u128;
        let whole = total / dur;
        let rem = total % dur;

        let part1 = match whole.checked_mul(el) {
            Some(value) => value,
            None => return total,
        };
        let part2 = match rem.checked_mul(el) {
            Some(value) => value / dur,
            None => return total,
        };
        part1.saturating_add(part2)
    }

    pub fn compute_accelerated_claimable_balance(
        total: u128,
        start: u64,
        now: u64,
        duration: u64,
        windows: &Vec<StreamAcceleration>,
    ) -> u128 {
        let base = compute_claimable_balance(total, start, now, duration);
        let mut extra = 0u128;

        for window in windows.iter() {
            if window.bonus_bps == 0 {
                continue;
            }

            let overlap_start = max(start, window.activated_at);
            let overlap_end = min(now, window.expires_at);
            if overlap_end <= overlap_start {
                continue;
            }

            let baseline_during_window =
                compute_claimable_balance(total, start, overlap_end, duration).saturating_sub(
                    compute_claimable_balance(total, start, overlap_start, duration),
                );
            let bonus = baseline_during_window.saturating_mul(window.bonus_bps as u128) / 10_000;
            extra = extra.saturating_add(bonus);
        }

        min(total, base.saturating_add(extra))
    }
}
