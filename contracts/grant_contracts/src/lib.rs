#![no_std]

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Vec,
    vec, Symbol,
};

const XLM_DECIMALS: u32 = 7;
const RENT_RESERVE_XLM: i128 = 5 * 10i128.pow(XLM_DECIMALS); // 5 XLM

pub mod optimized;
pub mod benchmarks;
pub mod self_terminate;
pub mod multi_token;
pub mod yield_treasury;
pub mod yield_enhanced;

// Re-export implementations
pub use optimized::{
    GrantContract as OptimizedContract, Grant as OptimizedGrant, DataKey as OptimizedDataKey,
    STATUS_ACTIVE, STATUS_PAUSED, STATUS_COMPLETED, STATUS_CANCELLED,
    STATUS_REVOCABLE, STATUS_MILESTONE_BASED, STATUS_AUTO_RENEW, STATUS_EMERGENCY_PAUSE,
};

pub const SCALING_FACTOR: i128 = 10_000_000; // 1e7

#[contract]
pub struct GrantContract;

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum GrantStatus {
    Active,
    Paused,      // Explicitly added to track pause state for Issue #39
    Completed,
    Cancelled,
    RageQuitted, // Terminal state for Issue #39
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamType {
    FixedAmount,
    FixedEndDate,
}

const INACTIVITY_THRESHOLD_SECS: u64 = 90 * 24 * 60 * 60; 

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
    pub last_claim_time: u64,
    pub pending_rate: i128,
    pub effective_timestamp: u64,
    pub status: GrantStatus,
    pub redirect: Option<Address>,
    pub stream_type: StreamType,
    pub start_time: u64,
    pub warmup_duration: u64,
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    GrantToken,
    GrantIds,
    Treasury,
    Oracle,
    Grant(u64),
    RecipientGrants(Address),
    NativeToken,
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
    InsufficientReserve = 10,
    RescueWouldViolateAllocated = 11,
    GranteeMismatch = 12,
    GrantNotInactive = 13,
}

const RATE_INCREASE_TIMELOCK_SECS: u64 = 48 * 60 * 60;

// --- Internal Helpers ---

fn read_admin(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&DataKey::Admin).ok_or(Error::NotInitialized)
}

fn require_admin_auth(env: &Env) -> Result<(), Error> {
    read_admin(env)?.require_auth();
    Ok(())
}

fn read_grant(env: &Env, grant_id: u64) -> Result<Grant, Error> {
    env.storage().instance().get(&DataKey::Grant(grant_id)).ok_or(Error::GrantNotFound)
}

fn write_grant(env: &Env, grant_id: u64, grant: &Grant) {
    env.storage().instance().set(&DataKey::Grant(grant_id), grant);
}

fn read_grant_token(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&DataKey::GrantToken).ok_or(Error::NotInitialized)
}

fn settle_grant(grant: &mut Grant, now: u64) -> Result<(), Error> {
    if now < grant.last_update_ts { return Err(Error::InvalidState); }
    
    let elapsed = now - grant.last_update_ts;
    if grant.status != GrantStatus::Active || elapsed == 0 {
        grant.last_update_ts = now;
        return Ok(());
    }

    let elapsed_i128 = i128::from(elapsed);
    let scaled_accrued = grant.flow_rate.checked_mul(elapsed_i128).ok_or(Error::MathOverflow)?;
    let accrued = scaled_accrued.checked_div(SCALING_FACTOR).ok_or(Error::MathOverflow)?;

    let remaining = grant.total_amount.checked_sub(grant.withdrawn + grant.claimable).ok_or(Error::MathOverflow)?;
    let delta = if accrued > remaining { remaining } else { accrued };

    grant.claimable = grant.claimable.checked_add(delta).ok_or(Error::MathOverflow)?;
    
    if (grant.withdrawn + grant.claimable) >= grant.total_amount {
        grant.status = GrantStatus::Completed;
    }

    grant.last_update_ts = now;
    Ok(())
}

#[contractimpl]
impl GrantContract {
    pub fn initialize(env: Env, admin: Address, grant_token: Address, treasury: Address, native_token: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) { return Err(Error::AlreadyInitialized); }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::GrantToken, &grant_token);
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        env.storage().instance().set(&DataKey::NativeToken, &native_token);
        env.storage().instance().set(&DataKey::GrantIds, &Vec::<u64>::new(&env));
        Ok(())
    }

    /// DAO Admin pauses the stream.
    pub fn pause_stream(env: Env, grant_id: u64) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Active { return Err(Error::InvalidState); }
        
        settle_grant(&mut grant, env.ledger().timestamp())?;
        grant.status = GrantStatus::Paused;
        write_grant(&env, grant_id, &grant);
        Ok(())
    }

    /// Issue #39: Rage Quit for Grantees.
    /// If paused, grantee can claim accrued funds and permanently close the grant.
    pub fn rage_quit(env: Env, grant_id: u64) -> Result<(), Error> {
        let mut grant = read_grant(&env, grant_id)?;

        // 1. Authorize the grantee
        grant.recipient.require_auth();

        // 2. Ensure the grant is paused (Security/Fairness requirement)
        if grant.status != GrantStatus::Paused {
            return Err(Error::InvalidState);
        }

        // 3. Settle to ensure 100% of accrued funds are calculated up to the pause
        settle_grant(&mut grant, env.ledger().timestamp())?;

        let claim_amount = grant.claimable;
        if claim_amount > 0 {
            let token_addr = read_grant_token(&env)?;
            let client = token::Client::new(&env, &token_addr);
            client.transfer(&env.current_contract_address(), &grant.recipient, &claim_amount);
            
            grant.withdrawn += claim_amount;
            grant.claimable = 0;
        }

        // 4. Set terminal state - grant can never be resumed
        grant.status = GrantStatus::RageQuitted;
        grant.flow_rate = 0;
        write_grant(&env, grant_id, &grant);

        env.events().publish((symbol_short!("ragequit"), grant_id), grant.recipient.clone());
        Ok(())
    }

    /// Admin attempt to resume. Blocks if grant was Rage Quitted.
    pub fn resume_stream(env: Env, grant_id: u64) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let mut grant = read_grant(&env, grant_id)?;
        
        // Ensure it cannot be resumed if Rage Quitted (Terminal State)
        if grant.status != GrantStatus::Paused {
            return Err(Error::InvalidState);
        }

        grant.status = GrantStatus::Active;
        grant.last_update_ts = env.ledger().timestamp();
        write_grant(&env, grant_id, &grant);
        Ok(())
    }

    pub fn withdraw(env: Env, grant_id: u64, amount: i128) -> Result<(), Error> {
        let mut grant = read_grant(&env, grant_id)?;
        grant.recipient.require_auth();
        
        if grant.status == GrantStatus::Cancelled || grant.status == GrantStatus::RageQuitted {
            return Err(Error::InvalidState);
        }

        settle_grant(&mut grant, env.ledger().timestamp())?;
        if amount > grant.claimable { return Err(Error::InvalidAmount); }

        grant.claimable -= amount;
        grant.withdrawn += amount;
        
        let token_addr = read_grant_token(&env)?;
        let client = token::Client::new(&env, &token_addr);
        client.transfer(&env.current_contract_address(), &grant.recipient, &amount);

        grant.last_claim_time = env.ledger().timestamp();
        write_grant(&env, grant_id, &grant);
        Ok(())
    }
}