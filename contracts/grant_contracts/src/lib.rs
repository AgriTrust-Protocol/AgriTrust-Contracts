#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Vec,
    Symbol, vec, IntoVal,
};

// --- Constants ---
pub const SCALING_FACTOR: i128 = 10_000_000; // 1e7
const XLM_DECIMALS: u32 = 7;
const RENT_RESERVE_XLM: i128 = 5 * 10i128.pow(XLM_DECIMALS);
const RATE_INCREASE_TIMELOCK_SECS: u64 = 48 * 60 * 60;
const INACTIVITY_THRESHOLD_SECS: u64 = 90 * 24 * 60 * 60;
const NFT_SUPPLY: i128 = 1000000; // Max NFT supply for completion certificates
const MIN_STAKE_PERCENTAGE: i128 = 1000; // 10% minimum stake (in basis points)
const MAX_STAKE_PERCENTAGE: i128 = 5000; // 50% maximum stake (in basis points)
const MIN_SECURITY_DEPOSIT_PERCENTAGE: i128 = 500; // 5% minimum security deposit
const MAX_SECURITY_DEPOSIT_PERCENTAGE: i128 = 2000; // 20% maximum security deposit

// Financial Snapshot constants
const SNAPSHOT_VERSION: u32 = 1; // Version for future compatibility
const SNAPSHOT_EXPIRY: u64 = 86400; // 24 hours in seconds

// --- Submodules ---
// Submodules removed for consolidation and to fix compilation errors.
// Core logic is now in this file.

// --- Types ---

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum GrantStatus {
    Active,
    Paused,
    Completed,
    Cancelled,
    RageQuitted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum StreamType {
    FixedAmount,
    FixedEndDate,
    TimeLockedLease,  // NEW: Lease stream to lessor address
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
    pub last_claim_time: u64,
    pub pending_rate: i128,
    pub effective_timestamp: u64,
    pub status: GrantStatus,
    pub redirect: Option<Address>,
    pub stream_type: StreamType,
    pub start_time: u64,
    pub warmup_duration: u64,
    // Staking fields
    pub required_stake: i128,
    pub staked_amount: i128,
    pub stake_token: Address,
    pub slash_reason: Option<String>,
    // Lease-specific fields
    pub lessor: Address,           // NEW: Equipment/property owner receiving payments
    pub property_id: String,        // NEW: Physical asset identifier
    pub serial_number: String,      // NEW: Equipment serial number
    pub security_deposit: i128,    // NEW: Security deposit amount
    pub lease_end_time: u64,      // NEW: Lease termination timestamp
    pub lease_terminated: bool,   // NEW: Legal oracle termination flag
    // Add funds tracking
    pub remaining_balance: i128,   // NEW: Remaining allocated balance for this grant
}

#[derive(Clone)]
#[contracttype]
pub struct FinancialSnapshot {
    pub grant_id: u64,           // Grant identifier
    pub total_received: i128,      // Total amount received by grantee
    pub timestamp: u64,           // When snapshot was created
    pub expiry: u64,             // When snapshot expires (24h)
    pub version: u32,            // Snapshot version for compatibility
    pub contract_signature: [u8; 64], // Contract's cryptographic signature
    pub hash: [u8; 32],        // SHA-256 hash of snapshot data
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    GrantToken,
    GrantIds,
    Treasury,
    Oracle,
    NativeToken,
    Grant(u64),
    RecipientGrants(Address),
    // Lease-related keys
    LeaseAgreement(u64), // Maps grant_id to lease agreement details
    PropertyRegistry(String), // Maps property_id to lease history
    // Financial snapshot keys
    FinancialSnapshot(u64, u64), // Maps grant_id + timestamp to snapshot
    SnapshotNonce(u64), // Maps grant_id to nonce for snapshot generation
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
    // Lease-related errors
    InvalidLeaseTerms = 14,
    LeaseAlreadyTerminated = 15,
    LeaseNotActive = 16,
    InvalidPropertyId = 17,
    InvalidSecurityDeposit = 18,
    LeaseNotExpired = 19,
    OracleTerminationFailed = 20,
    // Financial snapshot errors
    SnapshotExpired = 21,
    InvalidSnapshot = 22,
    SnapshotNotFound = 23,
    InvalidSignature = 24,
}

// --- Internal Helpers ---

fn read_admin(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&DataKey::Admin).ok_or(Error::NotInitialized)
}

fn read_oracle(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&DataKey::Oracle).ok_or(Error::NotInitialized)
}

fn require_admin_auth(env: &Env) -> Result<(), Error> {
    read_admin(env)?.require_auth();
    Ok(())
}

fn require_oracle_auth(env: &Env) -> Result<(), Error> {
    read_oracle(env)?.require_auth();
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

fn read_treasury(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&DataKey::Treasury).ok_or(Error::NotInitialized)
}

fn read_grant_ids(env: &Env) -> Vec<u64> {
    env.storage()
        .instance()
        .get(&DataKey::GrantIds)
        .unwrap_or_else(|| Vec::new(env))
}

// Lease Helper Functions
fn read_lease_agreement(env: &Env, grant_id: u64) -> Result<(Address, String, String, i128, u64), Error> {
    env.storage()
        .instance()
        .get(&DataKey::LeaseAgreement(grant_id))
        .ok_or(Error::GrantNotFound)
}

fn write_lease_agreement(env: &Env, grant_id: u64, lessor: &Address, property_id: &str, serial_number: &str, security_deposit: i128, lease_end_time: u64) {
    let agreement = (lessor.clone(), String::from_str(env, property_id), String::from_str(env, serial_number), security_deposit, lease_end_time);
    env.storage().instance().set(&DataKey::LeaseAgreement(grant_id), &agreement);
}

fn read_property_history(env: &Env, property_id: &str) -> Vec<(u64, Address, u64)> {
    env.storage()
        .instance()
        .get(&DataKey::PropertyRegistry(String::from_str(env, property_id)))
        .unwrap_or_else(|| Vec::new(env))
}

fn write_property_history(env: &Env, property_id: &str, history: &Vec<(u64, Address, u64)>) {
    env.storage().instance().set(&DataKey::PropertyRegistry(String::from_str(env, property_id)), history);
}

fn calculate_security_deposit(total_amount: i128, deposit_percentage: i128) -> Result<i128, Error> {
    if deposit_percentage < MIN_SECURITY_DEPOSIT_PERCENTAGE || deposit_percentage > MAX_SECURITY_DEPOSIT_PERCENTAGE {
        return Err(Error::InvalidSecurityDeposit);
    }
    
    let deposit = total_amount
        .checked_mul(deposit_percentage)
        .ok_or(Error::MathOverflow)?
        .checked_div(10000) // Convert from basis points
        .ok_or(Error::MathOverflow)?;
    
    Ok(deposit)
}

// Financial Snapshot Helper Functions
fn read_snapshot_nonce(env: &Env, grant_id: u64) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::SnapshotNonce(grant_id))
        .unwrap_or(0)
}

fn write_snapshot_nonce(env: &Env, grant_id: u64, nonce: u64) {
    env.storage().instance().set(&DataKey::SnapshotNonce(grant_id), &nonce);
}

fn read_financial_snapshot(env: &Env, grant_id: u64, timestamp: u64) -> Result<FinancialSnapshot, Error> {
    env.storage()
        .instance()
        .get(&DataKey::FinancialSnapshot(grant_id, timestamp))
        .ok_or(Error::SnapshotNotFound)
}

fn write_financial_snapshot(env: &Env, grant_id: u64, timestamp: u64, snapshot: &FinancialSnapshot) {
    env.storage().instance().set(&DataKey::FinancialSnapshot(grant_id, timestamp), snapshot);
}

fn generate_snapshot_hash(
    grant_id: u64,
    total_received: i128,
    timestamp: u64,
    expiry: u64,
    version: u32,
) -> [u8; 32] {
    // Create a deterministic hash from snapshot data
    // In a real implementation, this would use SHA-256 or similar
    let mut hasher = [0u8; 32];
    
    // Simple hash implementation for demonstration
    // In production, use proper cryptographic hash
    let combined = format!(
        "{}:{}:{}:{}:{}",
        grant_id, total_received, timestamp, expiry, version
    );
    
    // For now, return a placeholder hash
    // TODO: Implement proper SHA-256 hashing
    for i in 0..32.min(combined.len()) {
        hasher[i] = combined.as_bytes()[i];
    }
    
    hasher
}

fn generate_contract_signature(
    env: &Env,
    grant_id: u64,
    total_received: i128,
    timestamp: u64,
) -> [u8; 64] {
    // Generate a contract signature for the snapshot
    // In a real implementation, this would use the contract's private key
    // For now, return a deterministic signature based on the data
    let mut signature = [0u8; 64];
    
    let combined = format!("{}:{}:{}", grant_id, total_received, timestamp);
    
    // Simple signature generation for demonstration
    // In production, use proper cryptographic signing
    for i in 0..64.min(combined.len()) {
        signature[i] = combined.as_bytes()[i];
    }
    
    signature
}

fn total_allocated_funds(env: &Env) -> Result<i128, Error> {
    let mut total = 0_i128;
    let ids = read_grant_ids(env);
    for i in 0..ids.len() {
        let grant_id = ids.get(i).unwrap();
        if let Some(grant) = env.storage().instance().get::<_, Grant>(&DataKey::Grant(grant_id)) {
            if grant.status == GrantStatus::Active || grant.status == GrantStatus::Paused {
                let remaining = grant.total_amount
                    .checked_sub(grant.withdrawn)
                    .ok_or(Error::MathOverflow)?;
                total = total.checked_add(remaining).ok_or(Error::MathOverflow)?;
            }
        }
    }
    Ok(total)
}

fn calculate_warmup_multiplier(grant: &Grant, now: u64) -> i128 {
    if grant.warmup_duration == 0 {
        return 10000; // 100% in basis points
    }

    let warmup_end = grant.start_time + grant.warmup_duration;

    if now >= warmup_end {
        return 10000; 
    }

    if now <= grant.start_time {
        return 2500; // 25% at start
    }

    let elapsed_warmup = now - grant.start_time;
    let progress = ((elapsed_warmup as i128) * 10000) / (grant.warmup_duration as i128);

    // 25% + (75% * progress)
    2500 + (7500 * progress) / 10000
}

fn settle_grant(env: &Env, grant: &mut Grant, grant_id: u64, now: u64) -> Result<(), Error> {
    if now < grant.last_update_ts { return Err(Error::InvalidState); }
    
    let elapsed = now - grant.last_update_ts;
    if elapsed == 0 {
        return Ok(());
    }

    // Don't process accruals for terminated leases
    if grant.status == GrantStatus::Active && !grant.lease_terminated {
        // Handle pending rate increases first
        if grant.pending_rate > grant.flow_rate && grant.effective_timestamp != 0 && now >= grant.effective_timestamp {
            let switch_ts = grant.effective_timestamp;
            // Settle up to switch_ts at old rate
            let pre_elapsed = switch_ts - grant.last_update_ts;
            let pre_accrued = calculate_accrued(grant, pre_elapsed, switch_ts)?;
            grant.claimable = grant.claimable.checked_add(pre_accrued).ok_or(Error::MathOverflow)?;
            
            // Apply new rate
            grant.flow_rate = grant.pending_rate;
            grant.rate_updated_at = switch_ts;
            grant.pending_rate = 0;
            grant.effective_timestamp = 0;
            grant.last_update_ts = switch_ts;
            
            // Recalculate remaining elapsed
            let post_elapsed = now - switch_ts;
            let post_accrued = calculate_accrued(grant, post_elapsed, now)?;
            grant.claimable = grant.claimable.checked_add(post_accrued).ok_or(Error::MathOverflow)?;
        } else {
            let accrued = calculate_accrued(grant, elapsed, now)?;
            grant.claimable = grant.claimable.checked_add(accrued).ok_or(Error::MathOverflow)?;
        }
    }

    // Update remaining balance based on total allocated and withdrawn
    let total_withdrawable = grant.remaining_balance.checked_sub(grant.claimable).ok_or(Error::MathOverflow)?;
    grant.remaining_balance = total_withdrawable.checked_sub(grant.withdrawn).ok_or(Error::MathOverflow)?;

    let total_accounted = grant.withdrawn.checked_add(grant.claimable).ok_or(Error::MathOverflow)?;
    if total_accounted >= grant.total_amount {
        grant.claimable = grant.total_amount - grant.withdrawn;
        grant.status = GrantStatus::Completed;
        
        // Mint completion certificate if this is the first time completing
        // (Note: Leases don't get NFTs as they're for physical assets)
    }

    grant.last_update_ts = now;
    Ok(())
}

fn calculate_accrued(grant: &Grant, elapsed: u64, now: u64) -> Result<i128, Error> {
    let elapsed_i128 = i128::from(elapsed);
    let base_accrued = grant.flow_rate.checked_mul(elapsed_i128).ok_or(Error::MathOverflow)?;

    let multiplier = calculate_warmup_multiplier(grant, now);
    let accrued = base_accrued
        .checked_mul(multiplier)
        .ok_or(Error::MathOverflow)?
        .checked_div(10000)
        .ok_or(Error::MathOverflow)?;

    Ok(accrued)
}

// --- Contract Implementation ---

#[contract]
pub struct GrantContract;

#[contractimpl]
impl GrantContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        grant_token: Address,
        treasury: Address,
        oracle: Address,
        native_token: Address,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::GrantToken, &grant_token);
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        env.storage().instance().set(&DataKey::Oracle, &oracle);
        env.storage().instance().set(&DataKey::NativeToken, &native_token);
        env.storage().instance().set(&DataKey::GrantIds, &Vec::<u64>::new(&env));
        Ok(())
    }

    pub fn create_grant(
        env: Env,
        grant_id: u64,
        recipient: Address,
        total_amount: i128,
        flow_rate: i128,
        warmup_duration: u64,
        lessor: Address,
        property_id: String,
        serial_number: String,
        security_deposit_percentage: i128,
        lease_end_time: u64,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;

        if total_amount <= 0 || flow_rate < 0 {
            return Err(Error::InvalidAmount);
        }

        // Calculate security deposit
        let security_deposit = calculate_security_deposit(total_amount, security_deposit_percentage)?;

        let key = DataKey::Grant(grant_id);
        if env.storage().instance().has(&key) {
            return Err(Error::GrantAlreadyExists);
        }

        let now = env.ledger().timestamp();
        let grant = Grant {
            recipient: recipient.clone(),
            total_amount,
            withdrawn: 0,
            claimable: 0,
            flow_rate,
            last_update_ts: now,
            rate_updated_at: now,
            last_claim_time: now,
            pending_rate: 0,
            effective_timestamp: 0,
            status: GrantStatus::Active,
            redirect: None,
            stream_type: StreamType::TimeLockedLease,
            start_time: now,
            warmup_duration,
            // Staking fields (set to 0 for leases)
            required_stake: 0,
            staked_amount: 0,
            stake_token: Address::generate(&env), // Placeholder
            slash_reason: None,
            // Lease-specific fields
            lessor: lessor.clone(),
            property_id: property_id.clone(),
            serial_number: serial_number.clone(),
            security_deposit,
            lease_end_time,
            lease_terminated: false,
            // Add funds tracking
            remaining_balance: total_amount, // Initially, remaining equals total
        };

        env.storage().instance().set(&key, &grant);

        // Store lease agreement details
        write_lease_agreement(&env, grant_id, &lessor, &property_id, &serial_number, security_deposit, lease_end_time);

        // Update property registry
        let mut history = read_property_history(&env, &property_id);
        history.push_back((grant_id, recipient.clone(), now));
        write_property_history(&env, &property_id, &history);

        let mut ids = read_grant_ids(&env);
        ids.push_back(grant_id);
        env.storage().instance().set(&DataKey::GrantIds, &ids);

        let recipient_key = DataKey::RecipientGrants(recipient);
        let mut user_grants: Vec<u64> = env.storage().instance().get(&recipient_key).unwrap_or(vec![&env]);
        user_grants.push_back(grant_id);
        env.storage().instance().set(&recipient_key, &user_grants);

        // Publish lease creation event
        env.events().publish(
            (Symbol::new(&env, "lease_created"), grant_id),
            (recipient, lessor, property_id, security_deposit),
        );

        Ok(())
    }

    pub fn withdraw(env: Env, grant_id: u64, amount: i128) -> Result<(), Error> {
        let mut grant = read_grant(&env, grant_id)?;
        
        // For leases, authenticate lessor; for regular grants, authenticate recipient
        match grant.stream_type {
            StreamType::TimeLockedLease => {
                grant.lessor.require_auth();
            }
            _ => {
                grant.recipient.require_auth();
            }
        }

        if grant.status == GrantStatus::Cancelled || grant.status == GrantStatus::RageQuitted || grant.lease_terminated {
            return Err(Error::InvalidState);
        }

        settle_grant(&mut grant, env.ledger().timestamp())?;

        if amount > grant.claimable {
            return Err(Error::InvalidAmount);
        }

        grant.claimable = grant.claimable.checked_sub(amount).ok_or(Error::MathOverflow)?;
        grant.withdrawn = grant.withdrawn.checked_add(amount).ok_or(Error::MathOverflow)?;
        grant.last_claim_time = env.ledger().timestamp();

        write_grant(&env, grant_id, &grant);

        let token_addr = read_grant_token(&env)?;
        let client = token::Client::new(&env, &token_addr);
        
        // For leases, pay lessor; for regular grants, pay recipient
        let target = match grant.stream_type {
            StreamType::TimeLockedLease => grant.lessor.clone(),
            _ => grant.redirect.unwrap_or(grant.recipient.clone()),
        };
        
        client.transfer(&env.current_contract_address(), &target, &amount);

        try_call_on_withdraw(&env, &grant.recipient, grant_id, amount);

        Ok(())
    }

    pub fn pause_stream(env: Env, grant_id: u64) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Active { return Err(Error::InvalidState); }
        
        settle_grant(&mut grant, env.ledger().timestamp())?;
        grant.status = GrantStatus::Paused;
        write_grant(&env, grant_id, &grant);
        Ok(())
    }

    pub fn resume_stream(env: Env, grant_id: u64) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Paused { return Err(Error::InvalidState); }

        grant.status = GrantStatus::Active;
        grant.last_update_ts = env.ledger().timestamp();
        write_grant(&env, grant_id, &grant);
        Ok(())
    }

    pub fn propose_rate_change(env: Env, grant_id: u64, new_rate: i128) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Active { return Err(Error::InvalidState); }
        if new_rate < 0 { return Err(Error::InvalidRate); }

        settle_grant(&mut grant, env.ledger().timestamp())?;
        
        let old_rate = grant.flow_rate;
        if new_rate > old_rate {
            grant.pending_rate = new_rate;
            grant.effective_timestamp = env.ledger().timestamp() + RATE_INCREASE_TIMELOCK_SECS;
        } else {
            grant.flow_rate = new_rate;
            grant.rate_updated_at = env.ledger().timestamp();
            grant.pending_rate = 0;
            grant.effective_timestamp = 0;
        }

        write_grant(&env, grant_id, &grant);
        env.events().publish((symbol_short!("rateupdt"), grant_id), (old_rate, new_rate));
        Ok(())
    }

    pub fn apply_kpi_multiplier(env: Env, grant_id: u64, multiplier: i128) -> Result<(), Error> {
        require_oracle_auth(&env)?;
        if multiplier <= 0 { return Err(Error::InvalidRate); }

        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Active { return Err(Error::InvalidState); }

        settle_grant(&mut grant, env.ledger().timestamp())?;
        
        let old_rate = grant.flow_rate;
        grant.flow_rate = grant.flow_rate.checked_mul(multiplier).ok_or(Error::MathOverflow)? / 10000;
        grant.rate_updated_at = env.ledger().timestamp();

        write_grant(&env, grant_id, &grant);
        env.events().publish((symbol_short!("kpimul"), grant_id), (old_rate, grant.flow_rate, multiplier));
        Ok(())
    }

    pub fn rage_quit(env: Env, grant_id: u64) -> Result<(), Error> {
        let mut grant = read_grant(&env, grant_id)?;
        grant.recipient.require_auth();

        if grant.status != GrantStatus::Paused { return Err(Error::InvalidState); }

        settle_grant(&mut grant, env.ledger().timestamp())?;
        
        let claim_amount = grant.claimable;
        grant.claimable = 0;
        grant.withdrawn = grant.withdrawn.checked_add(claim_amount).ok_or(Error::MathOverflow)?;
        grant.status = GrantStatus::RageQuitted;
        
        let remaining = grant.total_amount.checked_sub(grant.withdrawn).ok_or(Error::MathOverflow)?;
        write_grant(&env, grant_id, &grant);

        let token_addr = read_grant_token(&env)?;
        let client = token::Client::new(&env, &token_addr);
        client.transfer(&env.current_contract_address(), &grant.recipient, &claim_amount);

        if remaining > 0 {
            let treasury = read_treasury(&env)?;
            client.transfer(&env.current_contract_address(), &treasury, &remaining);
        }

        Ok(())
    }

    pub fn rescue_tokens(env: Env, token_address: Address, amount: i128, to: Address) -> Result<(), Error> {
        require_admin_auth(&env)?;
        if amount <= 0 { return Err(Error::InvalidAmount); }

        let client = token::Client::new(&env, &token_address);
        let balance = client.balance(&env.current_contract_address());

        let total_allocated = if token_address == read_grant_token(&env)? {
            total_allocated_funds(&env)?
        } else {
            0
        };

        if balance.checked_sub(amount).ok_or(Error::MathOverflow)? < total_allocated {
            return Err(Error::RescueWouldViolateAllocated);
        }

        client.transfer(&env.current_contract_address(), &to, &amount);
        Ok(())
    }

    pub fn get_grant(env: Env, grant_id: u64) -> Result<Grant, Error> {
        read_grant(&env, grant_id)
    }

    // Add funds functionality
    pub fn add_funds(env: Env, grant_id: u64, amount: i128) -> Result<(), Error> {
        require_admin_auth(&env)?;
        
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        
        let mut grant = read_grant(&env, grant_id)?;
        
        // Validate grant state
        if grant.status != GrantStatus::Active && grant.status != GrantStatus::Paused {
            return Err(Error::InvalidState);
        }
        
        // Settle any pending accruals first
        settle_grant(&mut grant, grant_id, env.ledger().timestamp())?;
        
        // Add funds to remaining balance
        grant.remaining_balance = grant.remaining_balance.checked_add(amount).ok_or(Error::MathOverflow)?;
        
        // Calculate new end time if flow rate is constant
        let new_end_time = if grant.flow_rate > 0 {
            let additional_seconds = amount.checked_div(grant.flow_rate).ok_or(Error::MathOverflow)?;
            let current_end = if grant.lease_end_time > 0 { grant.lease_end_time } else { u64::MAX };
            grant.lease_end_time.checked_add(additional_seconds).ok_or(Error::MathOverflow)?
        } else {
            grant.lease_end_time // Flow rate is 0, end time unchanged
        };
        
        grant.lease_end_time = new_end_time;
        write_grant(&env, grant_id, &grant);
        
        // Publish GrantTopUp event
        env.events().publish(
            (Symbol::new(&env, "grant_top_up"), grant_id),
            (grant.recipient, amount, grant.remaining_balance, new_end_time),
        );
        
        Ok(())
    }

    // Lease-specific functions
    pub fn terminate_lease_by_oracle(env: Env, grant_id: u64, reason: String) -> Result<(), Error> {
        require_oracle_auth(&env)?;
        
        let mut grant = read_grant(&env, grant_id)?;
        
        // Validate lease can be terminated
        if grant.stream_type != StreamType::TimeLockedLease {
            return Err(Error::InvalidLeaseTerms);
        }
        
        if grant.lease_terminated {
            return Err(Error::LeaseAlreadyTerminated);
        }
        
        // Check if lease has expired
        let now = env.ledger().timestamp();
        if now < grant.lease_end_time {
            return Err(Error::LeaseNotExpired);
        }
        
        // Mark lease as terminated
        grant.lease_terminated = true;
        grant.status = GrantStatus::Cancelled;
        write_grant(&env, grant_id, &grant);
        
        // Return security deposit to treasury
        if grant.security_deposit > 0 {
            let treasury = read_treasury(&env)?;
            let token_client = token::Client::new(&env, &read_grant_token(&env)?);
            token_client.transfer(&env.current_contract_address(), &treasury, &grant.security_deposit);
        }
        
        // Publish termination event
        env.events().publish(
            (Symbol::new(&env, "lease_terminated"), grant_id),
            (grant.lessor, grant.recipient, reason, grant.security_deposit),
        );
        
        Ok(())
    }

    pub fn get_lease_info(env: Env, grant_id: u64) -> Result<(Address, String, String, i128, u64, bool), Error> {
        let grant = read_grant(&env, grant_id)?;
        if grant.stream_type != StreamType::TimeLockedLease {
            return Err(Error::InvalidLeaseTerms);
        }
        
        let (lessor, property_id, serial_number, security_deposit, lease_end_time) = read_lease_agreement(&env, grant_id)?;
        
        Ok((lessor, property_id, serial_number, security_deposit, lease_end_time, grant.lease_terminated))
    }

    pub fn get_property_history(env: Env, property_id: String) -> Vec<(u64, Address, u64)> {
        read_property_history(&env, &property_id)
    }

    // Financial Snapshot Functions
    pub fn create_financial_snapshot(env: Env, grant_id: u64) -> Result<FinancialSnapshot, Error> {
        let grant = read_grant(&env, grant_id)?;
        
        // Only grantee can create their own snapshot
        grant.recipient.require_auth();
        
        // Settle any pending accruals first
        settle_grant(&mut grant.clone(), grant_id, env.ledger().timestamp())?;
        
        let now = env.ledger().timestamp();
        let expiry = now + SNAPSHOT_EXPIRY;
        let nonce = read_snapshot_nonce(&env, grant_id) + 1;
        
        // Calculate total received (withdrawn + claimable)
        let total_received = grant.withdrawn.checked_add(grant.claimable).ok_or(Error::MathOverflow)?;
        
        // Generate hash and signature
        let hash = generate_snapshot_hash(grant_id, total_received, now, expiry, SNAPSHOT_VERSION);
        let signature = generate_contract_signature(&env, grant_id, total_received, now);
        
        let snapshot = FinancialSnapshot {
            grant_id,
            total_received,
            timestamp: now,
            expiry,
            version: SNAPSHOT_VERSION,
            contract_signature: signature,
            hash,
        };
        
        // Store snapshot and update nonce
        write_financial_snapshot(&env, grant_id, now, &snapshot);
        write_snapshot_nonce(&env, grant_id, nonce);
        
        // Publish snapshot creation event
        env.events().publish(
            (Symbol::new(&env, "financial_snapshot_created"), grant_id),
            (grant.recipient, total_received, now, expiry),
        );
        
        Ok(snapshot)
    }

    pub fn verify_financial_snapshot(
        env: Env, 
        grant_id: u64, 
        timestamp: u64,
        total_received: i128,
        hash: [u8; 32],
        signature: [u8; 64]
    ) -> Result<bool, Error> {
        let snapshot = read_financial_snapshot(&env, grant_id, timestamp)?;
        
        // Check if snapshot has expired
        if env.ledger().timestamp() > snapshot.expiry {
            return Err(Error::SnapshotExpired);
        }
        
        // Verify the hash matches
        let expected_hash = generate_snapshot_hash(
            grant_id,
            total_received,
            timestamp,
            snapshot.expiry,
            SNAPSHOT_VERSION
        );
        
        if hash != expected_hash {
            return Err(Error::InvalidSnapshot);
        }
        
        // Verify the signature matches
        let expected_signature = generate_contract_signature(&env, grant_id, total_received, timestamp);
        if signature != expected_signature {
            return Err(Error::InvalidSignature);
        }
        
        Ok(true)
    }

    pub fn get_snapshot_info(env: Env, grant_id: u64, timestamp: u64) -> Result<FinancialSnapshot, Error> {
        let snapshot = read_financial_snapshot(&env, grant_id, timestamp)?;
        
        // Check if snapshot has expired
        if env.ledger().timestamp() > snapshot.expiry {
            return Err(Error::SnapshotExpired);
        }
        
        Ok(snapshot)
    }

    pub fn claimable(env: Env, grant_id: u64) -> i128 {
        if let Ok(mut grant) = read_grant(&env, grant_id) {
            let _ = settle_grant(&mut grant, env.ledger().timestamp());
            grant.claimable
        } else {
            0
        }
    }
}

fn try_call_on_withdraw(env: &Env, recipient: &Address, grant_id: u64, amount: i128) {
    let args = (grant_id, amount).into_val(env);
    let _ = env.try_invoke_contract::<soroban_sdk::Val, soroban_sdk::Error>(
        recipient,
        &Symbol::new(env, "on_withdraw"),
        args,
    );
}

#[cfg(test)]
mod test;
#[cfg(test)]
mod test_nft;
#[cfg(test)]
mod test_staking;
#[cfg(test)]
mod test_lease;
#[cfg(test)]
mod test_add_funds;
#[cfg(test)]
mod test_financial_snapshot;
