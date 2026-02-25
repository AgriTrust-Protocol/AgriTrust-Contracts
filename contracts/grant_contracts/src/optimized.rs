#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env,
};

#[contract]
pub struct GrantContract;

// Bitwise status flags for grant optimization
// Each flag represents 1 bit in a u32 status mask
pub const STATUS_ACTIVE: u32 = 0b00000001;      // Grant is currently active
pub const STATUS_PAUSED: u32 = 0b00000010;      // Grant is paused
pub const STATUS_COMPLETED: u32 = 0b00000100;    // Grant is completed
pub const STATUS_CANCELLED: u32 = 0b00001000;    // Grant is cancelled
pub const STATUS_REVOCABLE: u32 = 0b00010000;  // Grant can be revoked
pub const STATUS_MILESTONE_BASED: u32 = 0b00100000; // Grant uses milestone-based releases
pub const STATUS_AUTO_RENEW: u32 = 0b01000000;  // Grant auto-renews
pub const STATUS_EMERGENCY_PAUSE: u32 = 0b10000000; // Grant is emergency paused

// Helper functions for bitwise operations
pub fn has_status(status_mask: u32, flag: u32) -> bool {
    (status_mask & flag) != 0
}

pub fn set_status(status_mask: u32, flag: u32) -> u32 {
    status_mask | flag
}

pub fn clear_status(status_mask: u32, flag: u32) -> u32 {
    status_mask & !flag
}

pub fn toggle_status(status_mask: u32, flag: u32) -> u32 {
    status_mask ^ flag
}

#[derive(Clone)]
#[contracttype]
pub struct Grant {
    pub recipient: Address,
    pub total_amount: i128,
    pub withdrawn: i128,
    pub claimable: i128,
    pub flow_rate: i128,
    pub last_update_ts: u64,
    pub rate_updated_at: u64,
    pub status_mask: u32, // Replaces multiple boolean fields with single u32
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Grant(u64),
}

#[contracterror]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    NotAuthorized = 3,
    GrantNotFound = 4,
    GrantAlreadyExists = 5,
    InvalidRate = 6,
    InvalidAmount = 7,
    InvalidState = 8,
    MathOverflow = 9,
    InvalidStatusTransition = 10, // New error for invalid status transitions
}

fn read_admin(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotInitialized)
}

fn require_admin_auth(env: &Env) -> Result<(), Error> {
    let admin = read_admin(env)?;
    admin.require_auth();
    Ok(())
}

fn read_grant(env: &Env, grant_id: u64) -> Result<Grant, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Grant(grant_id))
        .ok_or(Error::GrantNotFound)
}

fn write_grant(env: &Env, grant_id: u64, grant: &Grant) {
    env.storage().instance().set(&DataKey::Grant(grant_id), grant);
}

// Status transition validation using bitwise operations
fn validate_status_transition(current_mask: u32, new_mask: u32) -> Result<(), Error> {
    // Can't transition from completed or cancelled states
    if has_status(current_mask, STATUS_COMPLETED) || has_status(current_mask, STATUS_CANCELLED) {
        return Err(Error::InvalidStatusTransition);
    }
    
    // Validate specific transitions
    match (current_mask, new_mask) {
        // From any state to cancelled
        (_, new) if has_status(new, STATUS_CANCELLED) => Ok(()),
        
        // From active to paused
        (current, new) if has_status(current, STATUS_ACTIVE) && has_status(new, STATUS_PAUSED) 
            && !has_status(new, STATUS_ACTIVE) => Ok(()),
        
        // From paused to active
        (current, new) if has_status(current, STATUS_PAUSED) && has_status(new, STATUS_ACTIVE) 
            && !has_status(new, STATUS_PAUSED) => Ok(()),
        
        // From active/paused to completed
        (current, new) if (has_status(current, STATUS_ACTIVE) || has_status(current, STATUS_PAUSED)) 
            && has_status(new, STATUS_COMPLETED) 
            && !has_status(new, STATUS_ACTIVE) && !has_status(new, STATUS_PAUSED) => Ok(()),
        
        // Initial creation (must be active)
        (0, new) if has_status(new, STATUS_ACTIVE) 
            && !has_status(new, STATUS_PAUSED) && !has_status(new, STATUS_COMPLETED) && !has_status(new, STATUS_CANCELLED) => Ok(()),
        
        // Invalid transitions
        _ => Err(Error::InvalidStatusTransition),
    }
}

fn settle_grant(grant: &mut Grant, now: u64) -> Result<(), Error> {
    if now < grant.last_update_ts {
        return Err(Error::InvalidState);
    }

    let elapsed = now - grant.last_update_ts;
    grant.last_update_ts = now;

    // Only accrue if grant is active (not paused, completed, or cancelled)
    if !has_status(grant.status_mask, STATUS_ACTIVE) || elapsed == 0 || grant.flow_rate == 0 {
        return Ok(());
    }

    if grant.flow_rate < 0 {
        return Err(Error::InvalidRate);
    }

    let elapsed_i128 = i128::from(elapsed);
    let accrued = grant
        .flow_rate
        .checked_mul(elapsed_i128)
        .ok_or(Error::MathOverflow)?;

    let accounted = grant
        .withdrawn
        .checked_add(grant.claimable)
        .ok_or(Error::MathOverflow)?;

    if accounted > grant.total_amount {
        return Err(Error::InvalidState);
    }

    let remaining = grant
        .total_amount
        .checked_sub(accounted)
        .ok_or(Error::MathOverflow)?;

    let delta = if accrued > remaining {
        remaining
    } else {
        accrued
    };

    grant.claimable = grant
        .claimable
        .checked_add(delta)
        .ok_or(Error::MathOverflow)?;

    let new_accounted = grant
        .withdrawn
        .checked_add(grant.claimable)
        .ok_or(Error::MathOverflow)?;

    if new_accounted == grant.total_amount {
        // Mark as completed
        grant.status_mask = set_status(grant.status_mask, STATUS_COMPLETED);
        grant.status_mask = clear_status(grant.status_mask, STATUS_ACTIVE);
    }

    Ok(())
}

fn preview_grant_at_now(env: &Env, grant: &Grant) -> Result<Grant, Error> {
    let mut preview = grant.clone();
    settle_grant(&mut preview, env.ledger().timestamp())?;
    Ok(preview)
}

#[contractimpl]
impl GrantContract {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    pub fn create_grant(
        env: Env,
        grant_id: u64,
        recipient: Address,
        total_amount: i128,
        flow_rate: i128,
        initial_status_mask: u32, // Allow setting initial flags
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;

        if total_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        if flow_rate < 0 {
            return Err(Error::InvalidRate);
        }

        // Validate initial status
        validate_status_transition(0, initial_status_mask)?;

        let key = DataKey::Grant(grant_id);
        if env.storage().instance().has(&key) {
            return Err(Error::GrantAlreadyExists);
        }

        let now = env.ledger().timestamp();
        let grant = Grant {
            recipient,
            total_amount,
            withdrawn: 0,
            claimable: 0,
            flow_rate,
            last_update_ts: now,
            rate_updated_at: now,
            status_mask: initial_status_mask,
        };

        env.storage().instance().set(&key, &grant);
        Ok(())
    }

    pub fn cancel_grant(env: Env, grant_id: u64) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let mut grant = read_grant(&env, grant_id)?;

        let current_mask = grant.status_mask;
        let new_mask = set_status(current_mask, STATUS_CANCELLED);
        
        // Validate transition
        validate_status_transition(current_mask, new_mask)?;

        settle_grant(&mut grant, env.ledger().timestamp())?;
        grant.status_mask = new_mask;
        grant.flow_rate = 0; // Stop flow rate

        write_grant(&env, grant_id, &grant);
        Ok(())
    }

    pub fn pause_grant(env: Env, grant_id: u64) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let mut grant = read_grant(&env, grant_id)?;

        let current_mask = grant.status_mask;
        
        // Can only pause active grants
        if !has_status(current_mask, STATUS_ACTIVE) {
            return Err(Error::InvalidState);
        }

        let new_mask = set_status(current_mask, STATUS_PAUSED);
        new_mask = clear_status(new_mask, STATUS_ACTIVE);

        settle_grant(&mut grant, env.ledger().timestamp())?;
        grant.status_mask = new_mask;

        write_grant(&env, grant_id, &grant);
        Ok(())
    }

    pub fn resume_grant(env: Env, grant_id: u64) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let mut grant = read_grant(&env, grant_id)?;

        let current_mask = grant.status_mask;
        
        // Can only resume paused grants
        if !has_status(current_mask, STATUS_PAUSED) {
            return Err(Error::InvalidState);
        }

        let new_mask = set_status(current_mask, STATUS_ACTIVE);
        new_mask = clear_status(new_mask, STATUS_PAUSED);

        settle_grant(&mut grant, env.ledger().timestamp())?;
        grant.status_mask = new_mask;

        write_grant(&env, grant_id, &grant);
        Ok(())
    }

    pub fn set_grant_flags(
        env: Env, 
        grant_id: u64, 
        flags_to_set: u32, 
        flags_to_clear: u32
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let mut grant = read_grant(&env, grant_id)?;

        let current_mask = grant.status_mask;
        let new_mask = (current_mask | flags_to_set) & !flags_to_clear;
        
        // Validate that we're not making invalid transitions
        validate_status_transition(current_mask, new_mask)?;

        settle_grant(&mut grant, env.ledger().timestamp())?;
        grant.status_mask = new_mask;

        write_grant(&env, grant_id, &grant);
        Ok(())
    }

    pub fn get_grant(env: Env, grant_id: u64) -> Result<Grant, Error> {
        let grant = read_grant(&env, grant_id)?;
        preview_grant_at_now(&env, &grant)
    }

    pub fn get_grant_status(env: Env, grant_id: u64) -> Result<u32, Error> {
        let grant = read_grant(&env, grant_id)?;
        Ok(grant.status_mask)
    }

    pub fn is_grant_active(env: Env, grant_id: u64) -> Result<bool, Error> {
        let grant = read_grant(&env, grant_id)?;
        preview_grant_at_now(&env, &grant)?;
        Ok(has_status(grant.status_mask, STATUS_ACTIVE))
    }

    pub fn is_grant_paused(env: Env, grant_id: u64) -> Result<bool, Error> {
        let grant = read_grant(&env, grant_id)?;
        preview_grant_at_now(&env, &grant)?;
        Ok(has_status(grant.status_mask, STATUS_PAUSED))
    }

    pub fn is_grant_completed(env: Env, grant_id: u64) -> Result<bool, Error> {
        let grant = read_grant(&env, grant_id)?;
        preview_grant_at_now(&env, &grant)?;
        Ok(has_status(grant.status_mask, STATUS_COMPLETED))
    }

    pub fn is_grant_cancelled(env: Env, grant_id: u64) -> Result<bool, Error> {
        let grant = read_grant(&env, grant_id)?;
        preview_grant_at_now(&env, &grant)?;
        Ok(has_status(grant.status_mask, STATUS_CANCELLED))
    }

    pub fn claimable(env: Env, grant_id: u64) -> Result<i128, Error> {
        let grant = read_grant(&env, grant_id)?;
        let preview = preview_grant_at_now(&env, &grant)?;
        Ok(preview.claimable)
    }

    pub fn withdraw(env: Env, grant_id: u64, amount: i128) -> Result<(), Error> {
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let mut grant = read_grant(&env, grant_id)?;

        // Can only withdraw from active grants
        if !has_status(grant.status_mask, STATUS_ACTIVE) {
            return Err(Error::InvalidState);
        }

        grant.recipient.require_auth();

        settle_grant(&mut grant, env.ledger().timestamp())?;

        if amount > grant.claimable {
            return Err(Error::InvalidAmount);
        }

        grant.claimable = grant
            .claimable
            .checked_sub(amount)
            .ok_or(Error::MathOverflow)?;
        grant.withdrawn = grant
            .withdrawn
            .checked_add(amount)
            .ok_or(Error::MathOverflow)?;

        let accounted = grant
            .withdrawn
            .checked_add(grant.claimable)
            .ok_or(Error::MathOverflow)?;

        if accounted == grant.total_amount {
            grant.status_mask = set_status(grant.status_mask, STATUS_COMPLETED);
            grant.status_mask = clear_status(grant.status_mask, STATUS_ACTIVE);
        }

        write_grant(&env, grant_id, &grant);
    }

    pub fn update_rate(env: Env, grant_id: u64, new_rate: i128) -> Result<(), Error> {
        require_admin_auth(&env)?;

        if new_rate < 0 {
            return Err(Error::InvalidRate);
        }

        let mut grant = read_grant(&env, grant_id)?;
        
        // Can only update rate for active or paused grants
        if !has_status(grant.status_mask, STATUS_ACTIVE) && !has_status(grant.status_mask, STATUS_PAUSED) {
            return Err(Error::InvalidState);
        }

        let old_rate = grant.flow_rate;

        settle_grant(&mut grant, env.ledger().timestamp())?;
        
        if !has_status(grant.status_mask, STATUS_ACTIVE) && !has_status(grant.status_mask, STATUS_PAUSED) {
            write_grant(&env, grant_id, &grant);
            return Err(Error::InvalidState);
        }

        grant.flow_rate = new_rate;
        grant.rate_updated_at = grant.last_update_ts;

        write_grant(&env, grant_id, &grant);

        env.events().publish(
            (symbol_short!("rateupdt"), grant_id),
            (old_rate, new_rate, grant.rate_updated_at),
        );

        Ok(())
    }
}
