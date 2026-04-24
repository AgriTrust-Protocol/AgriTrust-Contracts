#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env,
};

#[contract]
pub struct GrantContract;

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum GrantStatus {
    Active,
    Completed,
    Cancelled,
}

#[derive(Clone)]
#[contracttype]
pub struct JointGrantInfo {
    pub partner: Address,
}

#[derive(Clone)]
#[contracttype]
pub struct Grant {
    pub grantor: Address,
    pub recipient: Address,
    pub asset_id: Address,
    pub total_amount: i128,
    pub withdrawn: i128,
    pub claimable: i128,
    pub flow_rate: i128,
    pub last_update_ts: u64,
    pub rate_updated_at: u64,
    pub status: GrantStatus,
    pub joint_info: Option<JointGrantInfo>,    // Issue #223
    pub sorosusu_debt_service: bool,           // Issue #213
    pub total_volume_serviced: i128,           // Track for Issue #233
}

#[derive(Clone)]
#[contracttype]
pub struct ConvictionVote {
    pub voter: Address,
    pub amount: i128,
    pub conviction: i128,
    pub last_update: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct OptimisticGrant {
    pub recipient: Address,
    pub amount: i128,
    pub submitter: Address,
    pub created_at: u64,
    pub challenged: bool,
    pub challenger: Option<Address>,
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    Grant(u64),
    TotalDeposited,
    TotalWithdrawn,
    TotalPending,
    Conviction(Address, u64),
    Optimistic(u64),
    ProtocolConfig,
}

#[derive(Clone)]
#[contracttype]
pub struct ProtocolConfig {
    pub sorosusu_address: Address,
    pub treasury_address: Address,
    pub sbt_minter_address: Address,
    pub debt_divert_bps: i128, // e.g., 2000 for 20%
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
    ConfigNotSet = 10,
    JointAuthRequired = 11,
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

fn get_total_pending(env: &Env) -> i128 {
    env.storage().instance().get(&DataKey::TotalPending).unwrap_or(0)
}

fn set_total_pending(env: &Env, amount: i128) {
    env.storage().instance().set(&DataKey::TotalPending, &amount);
}

fn write_grant(env: &Env, grant_id: u64, grant: &Grant) {
    env.storage()
        .instance()
        .set(&DataKey::Grant(grant_id), grant);
}

fn read_config(env: &Env) -> Result<ProtocolConfig, Error> {
    env.storage()
        .instance()
        .get(&DataKey::ProtocolConfig)
        .ok_or(Error::ConfigNotSet)
}

fn is_in_default(env: &Env, sorosusu: &Address, user: &Address) -> bool {
    env.invoke_contract::<bool>(sorosusu, &symbol_short!("is_deflt"), soroban_sdk::vec![env, user.clone()])
}

fn mint_sbt(env: &Env, config: &ProtocolConfig, grant_id: u64, recipient: &Address) {
    let _: () = env.invoke_contract(
        &config.sbt_minter_address,
        &symbol_short!("mint_sbt"),
        soroban_sdk::vec![env, grant_id, recipient.clone()],
    );
}

const TAX_THRESHOLD: i128 = 100_000_0000000; // $100,000 in 7-decimal places
const TAX_BPS: i128 = 1; // 0.01%

fn settle_grant(env: &Env, grant: &mut Grant, now: u64) -> Result<(), Error> {
    if now < grant.last_update_ts {
        return Err(Error::InvalidState);
    }

    let elapsed = now - grant.last_update_ts;
    grant.last_update_ts = now;

    if grant.status != GrantStatus::Active || elapsed == 0 || grant.flow_rate == 0 {
        return Ok(());
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

    let remaining = grant
        .total_amount
        .checked_sub(accounted)
        .ok_or(Error::MathOverflow)?;

    let delta = if accrued > remaining {
        remaining
    } else {
        accrued
    };

    if delta == 0 {
        return Ok(());
    }

    let config = read_config(env)?;
    let mut net_delta = delta;

    if grant.total_amount >= TAX_THRESHOLD {
        let tax = delta.checked_mul(TAX_BPS).unwrap().checked_div(10000).unwrap();
        if tax > 0 {
            env.invoke_contract::<()>(
                &config.treasury_address,
                &symbol_short!("deposit"),
                soroban_sdk::vec![env, tax],
            );
            net_delta = net_delta.checked_sub(tax).ok_or(Error::MathOverflow)?;
        }
    }

    if grant.sorosusu_debt_service {
        if is_in_default(env, &config.sorosusu_address, &grant.recipient) {
            let debt_service = delta.checked_mul(config.debt_divert_bps).unwrap().checked_div(10000).unwrap();
            if debt_service > 0 {
                env.invoke_contract::<()>(
                    &config.sorosusu_address,
                    &symbol_short!("repay"),
                    soroban_sdk::vec![env, grant.recipient.clone(), debt_service],
                );
                net_delta = net_delta.checked_sub(debt_service).ok_or(Error::MathOverflow)?;
            }
        }
    }

    grant.claimable = grant
        .claimable
        .checked_add(net_delta)
        .ok_or(Error::MathOverflow)?;

    grant.total_volume_serviced = grant.total_volume_serviced.checked_add(delta).ok_or(Error::MathOverflow)?;

    let new_accounted = grant
        .withdrawn
        .checked_add(grant.claimable)
        .ok_or(Error::MathOverflow)?;

    if new_accounted >= grant.total_amount {
        grant.status = GrantStatus::Completed;
    }

    Ok(())
}

fn preview_grant_at_now(env: &Env, grant: &Grant) -> Result<Grant, Error> {
    let mut preview = grant.clone();
    settle_grant(env, &mut preview, env.ledger().timestamp())?;
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

    pub fn set_protocol_config(
        env: Env,
        sorosusu: Address,
        treasury: Address,
        sbt_minter: Address,
        debt_divert_bps: i128,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let config = ProtocolConfig {
            sorosusu_address: sorosusu,
            treasury_address: treasury,
            sbt_minter_address: sbt_minter,
            debt_divert_bps,
        };
        env.storage().instance().set(&DataKey::ProtocolConfig, &config);
        Ok(())
    }

    pub fn create_grant(
        env: Env,
        grant_id: u64,
        grantor: Address,
        recipient: Address,
        asset_id: Address,
        total_amount: i128,
        flow_rate: i128,
        partner: Option<Address>, // For joint-grant
        auto_debt_service: bool,
    ) -> Result<(), Error> {
        grantor.require_auth();

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

        let joint_info = partner.map(|p| JointGrantInfo { partner: p });

        let now = env.ledger().timestamp();
        let grant = Grant {
            grantor: grantor.clone(),
            recipient: recipient.clone(),
            asset_id: asset_id.clone(),
            total_amount,
            withdrawn: 0,
            claimable: 0,
            flow_rate,
            last_update_ts: now,
            rate_updated_at: now,
            status: GrantStatus::Active,
            joint_info,
            sorosusu_debt_service: auto_debt_service,
            total_volume_serviced: 0,
        };

        write_grant(&env, grant_id, &grant);

        let pending = get_total_pending(&env);
        set_total_pending(&env, pending + total_amount);

        env.events().publish(
            (
                symbol_short!("created"),
                grant_id,
                grantor,
                recipient,
                asset_id,
            ),
            total_amount,
        );

        Ok(())
    }

    pub fn cancel_grant(env: Env, grant_id: u64) -> Result<(), Error> {
        let mut grant = read_grant(&env, grant_id)?;
        grant.grantor.require_auth();

        if grant.status != GrantStatus::Active {
            return Err(Error::InvalidState);
        }

        settle_grant(&env, &mut grant, env.ledger().timestamp())?;
        
        let pending = get_total_pending(&env);
        let remaining = grant.total_amount - grant.withdrawn;
        set_total_pending(&env, pending - remaining);

        grant.flow_rate = 0;
        grant.status = GrantStatus::Cancelled;
        write_grant(&env, grant_id, &grant);

        env.events().publish(
            (
                symbol_short!("cancelled"),
                grant_id,
                grant.grantor.clone(),
                grant.recipient.clone(),
                grant.asset_id.clone(),
            ),
            grant.claimable,
        );

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

        if let Some(ref joint) = grant.joint_info {
            grant.recipient.require_auth();
            joint.partner.require_auth();
        } else {
            grant.recipient.require_auth();
        }

        settle_grant(&env, &mut grant, env.ledger().timestamp())?;

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

        if grant.withdrawn >= grant.total_amount {
            grant.status = GrantStatus::Completed;
            let config = read_config(&env)?;
            mint_sbt(&env, &config, grant_id, &grant.recipient);
        }

        write_grant(&env, grant_id, &grant);

        let pending = get_total_pending(&env);
        set_total_pending(&env, pending - amount);

        env.events().publish(
            (
                symbol_short!("withdraw"),
                grant_id,
                grant.grantor.clone(),
                grant.recipient.clone(),
                grant.asset_id.clone(),
            ),
            amount,
        );

        Ok(())
    }

    pub fn split_and_separate(env: Env, grant_id: u64, new_grant_id: u64) -> Result<(), Error> {
        let mut grant = read_grant(&env, grant_id)?;
        
        if let Some(joint) = grant.joint_info {
            grant.recipient.require_auth();
            joint.partner.require_auth();

            settle_grant(&env, &mut grant, env.ledger().timestamp())?;

            let remaining_total = grant.total_amount.checked_sub(grant.withdrawn).ok_or(Error::MathOverflow)?;
            let half_remaining = remaining_total.checked_div(2).ok_or(Error::MathOverflow)?;
            let half_rate = grant.flow_rate.checked_div(2).ok_or(Error::MathOverflow)?;

            grant.total_amount = grant.withdrawn.checked_add(half_remaining).ok_or(Error::MathOverflow)?;
            grant.flow_rate = half_rate;
            grant.joint_info = None;
            write_grant(&env, grant_id, &grant);

            let now = env.ledger().timestamp();
            let partner_grant = Grant {
                grantor: grant.grantor.clone(),
                recipient: joint.partner,
                asset_id: grant.asset_id.clone(),
                total_amount: half_remaining,
                withdrawn: 0,
                claimable: 0,
                flow_rate: half_rate,
                last_update_ts: now,
                rate_updated_at: now,
                status: GrantStatus::Active,
                joint_info: None,
                sorosusu_debt_service: false,
                total_volume_serviced: 0,
            };
            write_grant(&env, new_grant_id, &partner_grant);

            Ok(())
        } else {
            Err(Error::InvalidState)
        }
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

        settle_grant(&env, &mut grant, env.ledger().timestamp())?;

        if grant.status != GrantStatus::Active {
            write_grant(&env, grant_id, &grant);
            return Err(Error::InvalidState);
        }

        grant.flow_rate = new_rate;
        grant.rate_updated_at = grant.last_update_ts;

        write_grant(&env, grant_id, &grant);

        env.events().publish(
            (
                symbol_short!("rateupdt"),
                grant_id,
                grant.grantor.clone(),
                grant.recipient.clone(),
                grant.asset_id.clone(),
            ),
            (old_rate, new_rate),
        );

        Ok(())
    }

    pub fn calculate_pool_health(env: Env, asset_id: Address, volatility_bps: u32) -> Result<u32, Error> {
        let pending = get_total_pending(&env);
        if pending == 0 {
            return Ok(10000); // 1.0 (bps)
        }

        let balance: i128 = 1_000_000; // Mocked balance

        let raw_ratio = if pending > 0 {
            (balance * 10000) / pending
        } else {
            10000
        };

        let volatility_adj = 10000u32.saturating_sub(volatility_bps);
        let health = (raw_ratio as u32 * volatility_adj) / 10000;

        if health < 2000 { // 0.2 threshold
            env.events().publish(
                (symbol_short!("risk_warn"), asset_id),
                (health, pending),
            );
        }

        Ok(health)
    }

    pub fn cast_conviction_vote(env: Env, voter: Address, grant_id: u64, amount: i128) -> Result<(), Error> {
        voter.require_auth();
        
        let key = DataKey::Conviction(voter.clone(), grant_id);
        let mut vote: ConvictionVote = env.storage().instance().get(&key).unwrap_or(ConvictionVote {
            voter: voter.clone(),
            amount: 0,
            conviction: 0,
            last_update: env.ledger().timestamp(),
        });

        let now = env.ledger().timestamp();
        let elapsed = now.saturating_sub(vote.last_update);
        
        let alpha = 9000; // 0.9 decay
        
        for _ in 0..elapsed.min(10) { // Limit iterations for safety
            vote.conviction = (vote.conviction * alpha) / 10000;
        }
        
        vote.amount = amount;
        vote.conviction = vote.conviction.checked_add(amount).ok_or(Error::MathOverflow)?;
        vote.last_update = now;

        env.storage().instance().set(&key, &vote);
        
        env.events().publish(
            (symbol_short!("voted"), voter, grant_id),
            vote.conviction,
        );

        Ok(())
    }

    pub fn submit_optimistic_grant(env: Env, grant_id: u64, recipient: Address, amount: i128, submitter: Address) -> Result<(), Error> {
        submitter.require_auth();
        
        if amount > 500 {
            return Err(Error::InvalidAmount);
        }

        let key = DataKey::Optimistic(grant_id);
        if env.storage().instance().has(&key) {
            return Err(Error::AlreadyInitialized);
        }

        let grant = OptimisticGrant {
            recipient,
            amount,
            submitter,
            created_at: env.ledger().timestamp(),
            challenged: false,
            challenger: None,
        };

        env.storage().instance().set(&key, &grant);
        
        env.events().publish(
            (symbol_short!("opt_sub"), grant_id),
            amount,
        );

        Ok(())
    }

    pub fn challenge_optimistic_grant(env: Env, grant_id: u64, challenger: Address) -> Result<(), Error> {
        challenger.require_auth();
        
        let key = DataKey::Optimistic(grant_id);
        let mut grant: OptimisticGrant = env.storage().instance().get(&key).ok_or(Error::GrantNotFound)?;

        let now = env.ledger().timestamp();
        if now > grant.created_at + 172800 { // 48 hours
            return Err(Error::InvalidState);
        }

        grant.challenged = true;
        grant.challenger = Some(challenger.clone());
        
        env.storage().instance().set(&key, &grant);
        
        env.events().publish(
            (symbol_short!("opt_chal"), grant_id),
            challenger,
        );

        Ok(())
    }
}

mod test;
