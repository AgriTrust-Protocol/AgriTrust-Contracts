#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, Symbol, Vec};

pub const FRACTIONAL_TOKENS_PER_TITLE_NFT: i128 = 100_000;
pub const MIN_HOLDING_PERIOD_SECONDS: u64 = 90 * 24 * 60 * 60;
pub const REVERIFICATION_PERIOD_SECONDS: u64 = 365 * 24 * 60 * 60;
pub const REDEMPTION_WINDOW_SECONDS: u64 = 30 * 24 * 60 * 60;
pub const MAX_HOLDING_BPS: i128 = 1_000; // 10%
pub const BPS_DENOMINATOR: i128 = 10_000;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClaimTopic {
    Kyc,
    AccreditedInvestor,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentityRecord {
    pub identity: Address,
    pub issuer: Address,
    pub kyc_expiry: u64,
    pub accredited_expiry: u64,
    pub last_verified: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RedemptionRequest {
    pub holder: Address,
    pub amount: i128,
    pub requested_at: u64,
    pub expires_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    TrustedIssuer(Address),
    Identity(Address),
    TitleNft,
    TitleCustodian,
    Balance(Address),
    TotalSupply,
    LastReceivedAt(Address),
    MerkleRoot(u32),
    Claimed(u32, Address),
    RedemptionWindowStart,
    Redemption(Address),
}

fn admin(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic!("not initialized"))
}

fn require_admin(env: &Env) {
    admin(env).require_auth();
}

fn read_balance(env: &Env, holder: Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Balance(holder))
        .unwrap_or(0)
}

fn is_verified_at(env: &Env, investor: Address, now: u64) -> bool {
    let Some(record): Option<IdentityRecord> =
        env.storage().persistent().get(&DataKey::Identity(investor))
    else {
        return false;
    };
    let trusted = env
        .storage()
        .persistent()
        .get(&DataKey::TrustedIssuer(record.issuer))
        .unwrap_or(false);
    trusted
        && record.kyc_expiry >= now
        && record.accredited_expiry >= now
        && record.last_verified + REVERIFICATION_PERIOD_SECONDS >= now
}

#[contract]
pub struct IdentityRegistry;

#[contractimpl]
impl IdentityRegistry {
    pub fn initialize(env: Env, administrator: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage()
            .instance()
            .set(&DataKey::Admin, &administrator);
    }

    pub fn add_trusted_issuer(env: Env, issuer: Address) {
        require_admin(&env);
        env.storage()
            .persistent()
            .set(&DataKey::TrustedIssuer(issuer.clone()), &true);
        env.events()
            .publish((Symbol::new(&env, "trusted_issuer_added"), issuer), ());
    }

    pub fn register_identity(
        env: Env,
        investor: Address,
        identity: Address,
        issuer: Address,
        kyc_expiry: u64,
        accredited_expiry: u64,
    ) {
        issuer.require_auth();
        let trusted = env
            .storage()
            .persistent()
            .get(&DataKey::TrustedIssuer(issuer.clone()))
            .unwrap_or(false);
        if !trusted {
            panic!("untrusted issuer");
        }
        let record = IdentityRecord {
            identity,
            issuer,
            kyc_expiry,
            accredited_expiry,
            last_verified: env.ledger().timestamp(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::Identity(investor.clone()), &record);
        env.events()
            .publish((Symbol::new(&env, "identity_registered"), investor), ());
    }

    pub fn revoke_identity(env: Env, investor: Address) {
        require_admin(&env);
        env.storage()
            .persistent()
            .remove(&DataKey::Identity(investor.clone()));
        env.events()
            .publish((Symbol::new(&env, "identity_revoked"), investor), ());
    }

    pub fn is_verified(env: Env, investor: Address) -> bool {
        is_verified_at(&env, investor, env.ledger().timestamp())
    }
}

#[contract]
pub struct FarmlandToken;

#[contractimpl]
impl FarmlandToken {
    pub fn initialize_token(
        env: Env,
        administrator: Address,
        title_nft: Address,
        title_custodian: Address,
    ) {
        if !env.storage().instance().has(&DataKey::Admin) {
            env.storage()
                .instance()
                .set(&DataKey::Admin, &administrator);
        }
        env.storage().instance().set(&DataKey::TitleNft, &title_nft);
        env.storage()
            .instance()
            .set(&DataKey::TitleCustodian, &title_custodian);
    }

    pub fn mint_from_title(env: Env, to: Address) {
        require_admin(&env);
        let now = env.ledger().timestamp();
        if !is_verified_at(&env, to.clone(), now) {
            panic!("recipient not verified");
        }
        if env
            .storage()
            .persistent()
            .get::<_, i128>(&DataKey::TotalSupply)
            .unwrap_or(0)
            != 0
        {
            panic!("already fractionalized");
        }
        env.storage().persistent().set(
            &DataKey::Balance(to.clone()),
            &FRACTIONAL_TOKENS_PER_TITLE_NFT,
        );
        env.storage()
            .persistent()
            .set(&DataKey::TotalSupply, &FRACTIONAL_TOKENS_PER_TITLE_NFT);
        env.storage()
            .persistent()
            .set(&DataKey::LastReceivedAt(to.clone()), &now);
        env.events().publish(
            (Symbol::new(&env, "title_fractionalized"), to),
            FRACTIONAL_TOKENS_PER_TITLE_NFT,
        );
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        Self::enforce_transfer(&env, from.clone(), to.clone(), amount);
        let from_balance = read_balance(&env, from.clone());
        let to_balance = read_balance(&env, to.clone());
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from.clone()), &(from_balance - amount));
        env.storage()
            .persistent()
            .set(&DataKey::Balance(to.clone()), &(to_balance + amount));
        env.storage().persistent().set(
            &DataKey::LastReceivedAt(to.clone()),
            &env.ledger().timestamp(),
        );
        env.events()
            .publish((Symbol::new(&env, "transfer"), from, to), amount);
    }

    pub fn enforce_transfer(env: &Env, from: Address, to: Address, amount: i128) {
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let now = env.ledger().timestamp();
        if !is_verified_at(env, from.clone(), now) || !is_verified_at(env, to.clone(), now) {
            panic!("identity not verified");
        }
        let received_at = env
            .storage()
            .persistent()
            .get::<_, u64>(&DataKey::LastReceivedAt(from.clone()))
            .unwrap_or(0);
        if received_at + MIN_HOLDING_PERIOD_SECONDS > now {
            panic!("holding period active");
        }
        let from_balance = read_balance(env, from.clone());
        if from_balance < amount {
            panic!("insufficient balance");
        }
        let total = env
            .storage()
            .persistent()
            .get::<_, i128>(&DataKey::TotalSupply)
            .unwrap_or(0);
        let max = (total * MAX_HOLDING_BPS) / BPS_DENOMINATOR;
        if read_balance(env, to) + amount > max {
            panic!("max holding exceeded");
        }
    }

    pub fn burn_for_redemption(env: Env, holder: Address, amount: i128) {
        holder.require_auth();
        RedemptionManager::request_redemption(env, holder, amount);
    }

    pub fn balance(env: Env, holder: Address) -> i128 {
        read_balance(&env, holder)
    }
    pub fn total_supply(env: Env) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0)
    }
}

#[contract]
pub struct DividendDistributor;

#[contractimpl]
impl DividendDistributor {
    pub fn publish_distribution(env: Env, distribution_id: u32, merkle_root: BytesN<32>) {
        require_admin(&env);
        env.storage()
            .persistent()
            .set(&DataKey::MerkleRoot(distribution_id), &merkle_root);
        env.events().publish(
            (Symbol::new(&env, "dividend_root"), distribution_id),
            merkle_root,
        );
    }

    pub fn claim_dividend(
        env: Env,
        distribution_id: u32,
        holder: Address,
        amount: i128,
        merkle_proof: Vec<BytesN<32>>,
    ) {
        holder.require_auth();
        if !is_verified_at(&env, holder.clone(), env.ledger().timestamp()) {
            panic!("holder not verified");
        }
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let _root: BytesN<32> = env
            .storage()
            .persistent()
            .get(&DataKey::MerkleRoot(distribution_id))
            .unwrap_or_else(|| panic!("unknown distribution"));
        if merkle_proof.is_empty() {
            panic!("merkle proof required");
        }
        let claimed_key = DataKey::Claimed(distribution_id, holder.clone());
        if env
            .storage()
            .persistent()
            .get(&claimed_key)
            .unwrap_or(false)
        {
            panic!("already claimed");
        }
        env.storage().persistent().set(&claimed_key, &true);
        env.events().publish(
            (Symbol::new(&env, "dividend_claimed"), holder),
            (distribution_id, amount),
        );
    }
}

#[contract]
pub struct RedemptionManager;

#[contractimpl]
impl RedemptionManager {
    pub fn open_redemption_window(env: Env, starts_at: u64) {
        require_admin(&env);
        env.storage()
            .persistent()
            .set(&DataKey::RedemptionWindowStart, &starts_at);
    }

    pub fn request_redemption(env: Env, holder: Address, amount: i128) {
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let now = env.ledger().timestamp();
        let start = env
            .storage()
            .persistent()
            .get::<_, u64>(&DataKey::RedemptionWindowStart)
            .unwrap_or_else(|| panic!("redemption closed"));
        if now < start || now > start + REDEMPTION_WINDOW_SECONDS {
            panic!("redemption closed");
        }
        let balance = read_balance(&env, holder.clone());
        if balance < amount {
            panic!("insufficient balance");
        }
        env.storage()
            .persistent()
            .set(&DataKey::Balance(holder.clone()), &(balance - amount));
        let total = env
            .storage()
            .persistent()
            .get::<_, i128>(&DataKey::TotalSupply)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::TotalSupply, &(total - amount));
        let request = RedemptionRequest {
            holder: holder.clone(),
            amount,
            requested_at: now,
            expires_at: start + REDEMPTION_WINDOW_SECONDS,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Redemption(holder.clone()), &request);
        env.events()
            .publish((Symbol::new(&env, "redemption_requested"), holder), amount);
    }
}

#[contract]
pub struct ComplianceModule;

#[contractimpl]
impl ComplianceModule {
    pub fn can_receive_dividends(env: Env, holder: Address) -> bool {
        is_verified_at(&env, holder, env.ledger().timestamp())
    }
    pub fn can_vote(env: Env, holder: Address) -> bool {
        is_verified_at(&env, holder, env.ledger().timestamp())
    }
    pub fn next_reverification_due(env: Env, holder: Address) -> u64 {
        let record: IdentityRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Identity(holder))
            .unwrap_or_else(|| panic!("identity not found"));
        record.last_verified + REVERIFICATION_PERIOD_SECONDS
    }
}

#[cfg(test)]
mod test;
