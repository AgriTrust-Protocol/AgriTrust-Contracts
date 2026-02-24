#![no_std]

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, vec, Address, Env,
    Vec,
};

/// Scaling factor for high-precision flow rate calculations.
/// This prevents zero flow rates when dealing with low-decimal tokens.
/// Flow rates are stored as scaled values (multiplied by this factor).
pub const SCALING_FACTOR: i128 = 10_000_000; // 1e7

#[contract]
pub struct GrantContract;


#[derive(Clone, PartialEq, Debug)]
#[contracttype]
pub enum GrantStatus {
    Active,
    Completed,
    Cancelled,
}

/// 90 days in seconds (inactivity threshold for slash_inactive_grant).
const INACTIVITY_THRESHOLD_SECS: u64 = 90 * 24 * 60 * 60; // 7_776_000

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
    /// Last time the grantee withdrew (or grant creation if never claimed). Used for inactivity slash.
    pub last_claim_time: u64,
    pub status: GrantStatus,
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    /// Token used for grants; allocated funds are measured in this token.
    GrantToken,
    /// DAO treasury; slashed funds are sent here.
    Treasury,
    /// All grant IDs ever created (for computing total_allocated_funds).
    GrantIds,
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
    /// Rescue amount would leave less than total allocated funds in the contract.
    RescueWouldViolateAllocated = 10,
    /// Grant has been active (claimed) within the inactivity threshold; cannot slash yet.
    GrantNotInactive = 11,
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

fn read_grant_token(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&DataKey::GrantToken)
        .ok_or(Error::NotInitialized)
}

fn read_treasury(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Treasury)
        .ok_or(Error::NotInitialized)
}

fn read_grant_ids(env: &Env) -> Vec<u64> {
    env.storage()
        .instance()
        .get(&DataKey::GrantIds)
        .unwrap_or_else(|| Vec::new(env))
}

/// Sum of (total_amount - withdrawn) for all active grants. Represents tokens that must remain in the contract.
fn total_allocated_funds(env: &Env) -> Result<i128, Error> {
    let mut total = 0_i128;
    let ids = read_grant_ids(env);
    for i in 0..ids.len() {
        let grant_id = ids.get(i).unwrap();
        if let Some(grant) = env.storage().instance().get::<_, Grant>(&DataKey::Grant(grant_id)) {
            if grant.status == GrantStatus::Active {
                let remaining = grant
                    .total_amount
                    .checked_sub(grant.withdrawn)
                    .ok_or(Error::MathOverflow)?;
                total = total.checked_add(remaining).ok_or(Error::MathOverflow)?;
            }
        }
    }
    Ok(total)
    env.storage()
        .instance()
        .set(&DataKey::Grant(grant_id), grant);
}



fn settle_grant(grant: &mut Grant, now: u64) -> Result<(), Error> {
    if now < grant.last_update_ts {
        return Err(Error::InvalidState);
    }

    let elapsed = now - grant.last_update_ts;
    grant.last_update_ts = now;

    if grant.status != GrantStatus::Active || elapsed == 0 || grant.flow_rate == 0 {
        return Ok(());
    }

    if grant.flow_rate < 0 {
        return Err(Error::InvalidRate);
    }

    let elapsed_i128 = i128::from(elapsed);
    // Flow rate is stored as a scaled value, so we divide by SCALING_FACTOR
    // to get the actual accrued amount in token units
    let scaled_accrued = grant
        .flow_rate
        .checked_mul(elapsed_i128)
        .ok_or(Error::MathOverflow)?;
    let accrued = scaled_accrued
        .checked_div(SCALING_FACTOR)
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
        grant.status = GrantStatus::Completed;
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
    pub fn initialize(
        env: Env,
        admin: Address,
        grant_token: Address,
        treasury: Address,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::GrantToken, &grant_token);
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        env.storage()
            .instance()
            .set(&DataKey::GrantIds, &Vec::<u64>::new(&env));
        Ok(())
    }

    pub fn create_grant(
        env: Env,
        grant_id: u64,
        recipient: Address,
        total_amount: i128,
        flow_rate: i128,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;

        if total_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        if flow_rate < 0 {
            return Err(Error::InvalidRate);
        }

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
            last_claim_time: now,
            status: GrantStatus::Active,
        };

        env.storage().instance().set(&key, &grant);
        let mut ids = read_grant_ids(&env);
        ids.push_back(grant_id);
        env.storage().instance().set(&DataKey::GrantIds, &ids);
        Ok(())
    }

    pub fn cancel_grant(env: Env, grant_id: u64) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let mut grant = read_grant(&env, grant_id)?;

        if grant.status != GrantStatus::Active {
            return Err(Error::InvalidState);
        }

        settle_grant(&mut grant, env.ledger().timestamp())?;
        grant.flow_rate = 0;
        grant.status = GrantStatus::Cancelled;
        write_grant(&env, grant_id, &grant);

        Ok(())
    }

    pub fn get_grant(env: Env, grant_id: u64) -> Result<Grant, Error> {
        let grant = read_grant(&env, grant_id)?;
        preview_grant_at_now(&env, &grant)
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

        if grant.status == GrantStatus::Cancelled {
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

        if accounted > grant.total_amount {
            return Err(Error::InvalidState);
        }

        if grant.withdrawn == grant.total_amount {
            grant.status = GrantStatus::Completed;
        }

        grant.last_claim_time = env.ledger().timestamp();
        write_grant(&env, grant_id, &grant);
        Ok(())
    }

    /// Anyone may call. Cancel an active grant if the grantee has not claimed in 90+ days; return remaining funds to treasury.
    pub fn slash_inactive_grant(env: Env, grant_id: u64) -> Result<(), Error> {
        let mut grant = read_grant(&env, grant_id)?;

        if grant.status != GrantStatus::Active {
            return Err(Error::InvalidState);
        }

        let now = env.ledger().timestamp();
        settle_grant(&mut grant, now)?;

        if grant.status != GrantStatus::Active {
            write_grant(&env, grant_id, &grant);
            return Err(Error::InvalidState);
        }

        let inactive_secs = now.saturating_sub(grant.last_claim_time);
        if inactive_secs < INACTIVITY_THRESHOLD_SECS {
            return Err(Error::GrantNotInactive);
        }

        let remaining = grant
            .total_amount
            .checked_sub(grant.withdrawn)
            .ok_or(Error::MathOverflow)?;

        grant.flow_rate = 0;
        grant.status = GrantStatus::Cancelled;
        write_grant(&env, grant_id, &grant);

        if remaining > 0 {
            let contract = env.current_contract_address();
            let token = read_grant_token(&env)?;
            let treasury = read_treasury(&env)?;
            let client = token::Client::new(&env, &token);
            client.transfer(&contract, &treasury, remaining);
        }

        Ok(())
        write_grant(&env, grant_id, &grant);
        Ok(())
    }

    pub fn update_rate(env: Env, grant_id: u64, new_rate: i128) -> Result<(), Error> {
        require_admin_auth(&env)?;

        if new_rate < 0 {
            return Err(Error::InvalidRate);
        }

        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Active {
            return Err(Error::InvalidState);
        }

        let old_rate = grant.flow_rate;

        settle_grant(&mut grant, env.ledger().timestamp())?;

        if grant.status != GrantStatus::Active {
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

    /// Rescue stray tokens sent directly to the contract. Admin-only. Ensures contract_balance - amount >= total_allocated_funds for the grant token.
    pub fn rescue_tokens(
        env: Env,
        token_address: Address,
        amount: i128,
        to: Address,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let contract = env.current_contract_address();
        let client = token::Client::new(&env, &token_address);
        let contract_balance = client.balance(&contract);

        let total_allocated = if token_address == read_grant_token(&env)? {
            total_allocated_funds(&env)?
        } else {
            0
        };

        let after_rescue = contract_balance
            .checked_sub(amount)
            .ok_or(Error::MathOverflow)?;
        if after_rescue < total_allocated {
            return Err(Error::RescueWouldViolateAllocated);
        }

        client.transfer(&contract, &to, amount);
        Ok(())
    }
}

mod test;
