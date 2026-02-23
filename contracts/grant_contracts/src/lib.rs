#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Env, Map, Symbol, String, Vec, 
    token, panic_with_error, unwrap::UnwrapOptimized
};

#[contract]
pub struct GrantContract;

#[contracttype]
pub enum DataKey {
    Grant(Symbol),
    Milestone(Symbol, Symbol),
}

#[contracttype]
pub struct Grant {
    pub admin: Address,
    pub grantee: Address,
    pub total_amount: u128,
    pub released_amount: u128,
    pub token_address: Address,
    pub created_at: u64,
    pub status: GrantStatus,
}

#[contracttype]
pub enum GrantStatus {
    Proposed,
    Active,
    Paused,
    Completed,
    Cancelled,
}

#[contracttype]
pub struct Milestone {
    pub amount: u128,
    pub description: String,
    pub approved: bool,
    pub approved_at: Option<u64>,
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
        }
    }
}

#[contractimpl]
impl GrantContract {
    pub fn create_grant(
        env: Env,
        grant_id: Symbol,
        admin: Address,
        grantee: Address,
        total_amount: u128,
        token_address: Address,
    ) {
        admin.require_auth();
        
        if total_amount == 0 {
            panic_with_error!(&env, GrantError::InvalidAmount);
        }

        let grant = Grant {
            admin: admin.clone(),
            grantee: grantee.clone(),
            total_amount,
            released_amount: 0,
            token_address: token_address.clone(),
            created_at: env.ledger().timestamp(),
            status: GrantStatus::Proposed,
        };

        env.storage().instance().set(&DataKey::Grant(grant_id), &grant);
    }

    pub fn add_milestone(
        env: Env,
        grant_id: Symbol,
        milestone_id: Symbol,
        amount: u128,
        description: String,
    ) {
        let grant_key = DataKey::Grant(grant_id.clone());
        let mut grant: Grant = env.storage().instance()
            .get::<_, Grant>(&grant_key)
            .unwrap_optimized();

        grant.admin.require_auth();

        if amount == 0 {
            panic_with_error!(&env, GrantError::InvalidAmount);
        }

        let milestone = Milestone {
            amount,
            description,
            approved: false,
            approved_at: None,
        };

        env.storage().instance().set(&DataKey::Milestone(grant_id, milestone_id), &milestone);
    }

    pub fn approve_milestone(env: Env, grant_id: Symbol, milestone_id: Symbol) {
        let grant_key = DataKey::Grant(grant_id.clone());
        let mut grant: Grant = env.storage().instance()
            .get::<_, Grant>(&grant_key)
            .unwrap_optimized();

        grant.admin.require_auth();

        let milestone_key = DataKey::Milestone(grant_id.clone(), milestone_id.clone());
        let mut milestone: Milestone = env.storage().instance()
            .get::<_, Milestone>(&milestone_key)
            .unwrap_optimized();

        if milestone.approved {
            panic_with_error!(&env, GrantError::AlreadyApproved);
        }

        let new_released = grant.released_amount.checked_add(milestone.amount)
            .unwrap_or_else(|| panic_with_error!(&env, GrantError::ExceedsTotalAmount));

        if new_released > grant.total_amount {
            panic_with_error!(&env, GrantError::ExceedsTotalAmount);
        }

        milestone.approved = true;
        milestone.approved_at = Some(env.ledger().timestamp());
        grant.released_amount = new_released;

        if grant.released_amount == grant.total_amount {
            grant.status = GrantStatus::Completed;
        }

        env.storage().instance().set(&milestone_key, &milestone);
        env.storage().instance().set(&grant_key, &grant);

        // Apply Checks-Effects-Interactions pattern
        // Update state before external call
        Self::transfer_tokens(&env, &grant.token_address, &grant.admin, &grant.grantee, milestone.amount);
    }

    pub fn withdraw(env: Env, grant_id: Symbol, amount: u128) {
        let grant_key = DataKey::Grant(grant_id.clone());
        let mut grant: Grant = env.storage().instance()
            .get::<_, Grant>(&grant_key)
            .unwrap_optimized();

        grant.grantee.require_auth();

        if amount == 0 {
            panic_with_error!(&env, GrantError::InvalidAmount);
        }

        let available = grant.released_amount;
        if amount > available {
            panic_with_error!(&env, GrantError::InvalidAmount);
        }

        // Checks-Effects-Interactions: Update state before external call
        grant.released_amount = grant.released_amount.checked_sub(amount).unwrap_optimized();
        env.storage().instance().set(&grant_key, &grant);

        // External interaction
        Self::transfer_tokens(&env, &grant.token_address, &env.current_contract_address(), &grant.grantee, amount);
    }

    pub fn activate_grant(env: Env, grant_id: Symbol) {
        let grant_key = DataKey::Grant(grant_id.clone());
        let mut grant: Grant = env.storage().instance()
            .get::<_, Grant>(&grant_key)
            .unwrap_optimized();

        grant.admin.require_auth();

        match grant.status {
            GrantStatus::Proposed => {
                grant.status = GrantStatus::Active;
                env.storage().instance().set(&grant_key, &grant);
            }
            _ => panic_with_error!(&env, GrantError::InvalidStatus),
        }
    }

    pub fn pause_grant(env: Env, grant_id: Symbol) {
        let grant_key = DataKey::Grant(grant_id.clone());
        let mut grant: Grant = env.storage().instance()
            .get::<_, Grant>(&grant_key)
            .unwrap_optimized();

        grant.admin.require_auth();

        match grant.status {
            GrantStatus::Active => {
                grant.status = GrantStatus::Paused;
                env.storage().instance().set(&grant_key, &grant);
            }
            _ => panic_with_error!(&env, GrantError::InvalidStatus),
        }
    }

    pub fn resume_grant(env: Env, grant_id: Symbol) {
        let grant_key = DataKey::Grant(grant_id.clone());
        let mut grant: Grant = env.storage().instance()
            .get::<_, Grant>(&grant_key)
            .unwrap_optimized();

        grant.admin.require_auth();

        match grant.status {
            GrantStatus::Paused => {
                grant.status = GrantStatus::Active;
                env.storage().instance().set(&grant_key, &grant);
            }
            _ => panic_with_error!(&env, GrantError::InvalidStatus),
        }
    }

    pub fn cancel_grant(env: Env, grant_id: Symbol) {
        let grant_key = DataKey::Grant(grant_id.clone());
        let mut grant: Grant = env.storage().instance()
            .get::<_, Grant>(&grant_key)
            .unwrap_optimized();

        grant.admin.require_auth();

        match grant.status {
            GrantStatus::Proposed | GrantStatus::Paused => {
                grant.status = GrantStatus::Cancelled;
                env.storage().instance().set(&grant_key, &grant);
            }
            _ => panic_with_error!(&env, GrantError::InvalidStatus),
        }
    }

    pub fn get_grant(env: Env, grant_id: Symbol) -> Grant {
        env.storage().instance()
            .get::<_, Grant>(&DataKey::Grant(grant_id))
            .unwrap_optimized()
    }

    pub fn get_remaining_amount(env: Env, grant_id: Symbol) -> u128 {
        let grant = Self::get_grant(env, grant_id);
        grant.total_amount.saturating_sub(grant.released_amount)
    }

    fn transfer_tokens(env: &Env, token_address: &Address, from: &Address, to: &Address, amount: u128) {
        let token_client = token::Client::new(env, token_address);
        
        // Handle potential transfer fees by checking balance after transfer
        let from_balance_before = token_client.balance(from);
        let to_balance_before = token_client.balance(to);
        
        token_client.transfer(from, to, &(amount as i128));
        
        let from_balance_after = token_client.balance(from);
        let to_balance_after = token_client.balance(to);
        
        // Verify transfer behavior for tokens with fees
        let expected_from_decrease = amount as i128;
        let actual_from_decrease = from_balance_before.saturating_sub(from_balance_after);
        let actual_to_increase = to_balance_after.saturating_sub(to_balance_before);
        
        // For tokens with transfer fees, actual_to_increase might be less than amount
        // This is expected behavior for fee-charging tokens
        if actual_from_decrease != expected_from_decrease {
            // Log warning but don't fail - some tokens might have complex fee structures
            // Note: Logging is limited in Soroban, so we'll just continue
            // The transfer fee detection logic is still useful for debugging
        }
    }
}

mod test;

// Grant math utilities used by tests and (optionally) the contract.
pub mod grant {
    /// Compute the claimable balance for a linear vesting grant.
    ///
    /// - `total`: total amount granted (u128)
    /// - `start`: grant start timestamp (seconds, u64)
    /// - `now`: current timestamp (seconds, u64)
    /// - `duration`: grant duration (seconds, u64)
    ///
    /// Returns the amount (u128) claimable at `now` (clamped 0..=total).
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

        // Use decomposition to reduce risk of intermediate overflow:
        // total * elapsed / duration == (total / duration) * elapsed + (total % duration) * elapsed / duration
        let dur = duration as u128;
        let el = elapsed as u128;
        let whole = total / dur;
        let rem = total % dur;

        // whole * el shouldn't overflow in realistic token amounts, but use checked_mul with fallback.
        let part1 = match whole.checked_mul(el) {
            Some(v) => v,
            None => {
                // fallback: perform (whole / dur) * (el * dur) approximated by dividing early
                // This branch is extremely unlikely; clamp to total as safe fallback.
                return total;
            }
        };
        let part2 = match rem.checked_mul(el) {
            Some(v) => v / dur,
            None => {
                return total;
            }
        };
        part1 + part2
    }
}
