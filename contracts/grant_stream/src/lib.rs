#![no_std]
#[cfg(test)]
extern crate std;
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Vec,
    Symbol, vec, IntoVal, String, Map, xdr::ScVal, xdr::ToXdr, Bytes,
};

// PATCH: declare the new reentrancy module ───────────────────────────────────
pub mod reentrancy;

// --- Donor Reputation Module ---
pub mod donor_reputation;

// --- Constants ---
pub const SCALING_FACTOR: i128 = 10_000_000; // 1e7
pub const SEP38_STALENESS_SECONDS: u64 = 5 * 60;
pub const GRACE_PERIOD_SECONDS: u64 = 30 * 24 * 60 * 60;
pub const DEFAULT_EXPECTED_LEDGER_DURATION_SECONDS: u64 = 5;
pub const GRACE_PERIOD_SLIPPAGE_LEDGERS: u32 = 10;
pub const MAX_MISSED_DISTRIBUTIONS: u32 = 3;
pub const LATE_FEE_BPS: i128 = 250;
const XLM_DECIMALS: u32 = 7;
const RENT_RESERVE_XLM: i128 = 5 * 10i128.pow(XLM_DECIMALS);
const ZK_COMMITMENT_MODULUS: i128 = 170_141_183_460_469_231_731_687_303_715_884_105_727i128;
// Minimum claimable balance required before a withdrawal is permitted (1 USDC in 7-decimal units)
pub const MIN_WITHDRAWAL: i128 = 10_000_000;
const RATE_INCREASE_TIMELOCK_SECS: u64 = 48 * 60 * 60;
const INACTIVITY_THRESHOLD_SECS: u64 = 90 * 24 * 60 * 60;
const PRUNE_DELAY_SECONDS: u64 = 180 * 24 * 60 * 60;

// Re-export constants for tests
#[cfg(test)]
pub use donor_reputation::{
    REPUTATION_SCALE, BASIS_POINTS, DEFAULT_MIN_FUNDING_THRESHOLD, MAX_REPUTATION_MULTIPLIER,
};
#[cfg(test)]
pub use matching_pool::FIXED_POINT_SCALE;

// --- Submodules ---
pub mod storage_keys;
pub mod multi_token;
pub mod yield_treasury;
pub mod optimized;
mod self_terminate;
pub mod circuit_breakers;
pub mod public_dashboard;
pub mod tax_reporting;
pub mod audit_log;
pub mod multi_threshold;
pub mod security_council;
pub mod matching_pool;
pub mod precision;

#[cfg(all(test, feature = "legacy-tests"))]
mod test_dispute_circuit_breaker;
#[cfg(test)]
mod test_security_council;

#[cfg(test)]
mod test_matching_pool;

#[cfg(test)]
mod test_donor_reputation;

#[cfg(test)]
mod test_reputation_matching_integration;

#[cfg(test)]
mod test_sweeper;
#[cfg(test)]
mod test_grace_period_temporal_fuzz;

// --- Types ---

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum GrantStatus {
    Active,
    Paused,
    Completed,
    Cancelled,
    RageQuitted,
    Clawbacked,
}

// Import the unified storage keys
use crate::storage_keys::{StorageKey, MilestoneKey, VoteKey};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum StreamType {
    FixedAmount,
    FixedEndDate,
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
    /// Optional Stellar Validator reward address. When set, 5% of accruals
    /// are directed here ("Ecosystem Tax").
    pub validator: Option<Address>,
    /// Independent withdrawal counter for the validator's 5% share.
    pub validator_withdrawn: i128,
    /// Claimable balance accumulator for the validator (5% of stream).
    pub validator_claimable: i128,
    /// CID (Content Identifier) of the legal document (e.g., SAFT or Grant Agreement).
    pub legal_hash: Option<String>,
    /// Flag to prevent funds from streaming until the grantee has cryptographically "signed" the legal document on-chain.
    pub requires_legal_signature: bool,
    /// Boolean flag indicating if the legal document has been signed by the grantee.
    pub is_legal_signed: bool,
    /// Optional reason string for why the grant was paused
    pub pause_reason: Option<String>,
    /// Timestamp when cancellation was initiated (0 if not cancelling)
    /// Used to detect and protect against race conditions during Stellar ledger close
    pub cancellation_initiated_at: u64,
    /// Amount that was withdrawn during the cancellation window
    /// Eligible for clawback if withdrawn after cancellation was initiated
    pub clawback_eligible: i128,
    /// Original donor who funded the grant (for clawback authorization)
    pub donor: Option<Address>,
    /// Checkpoint timestamp when clawback was executed (to prevent double-spending)
    pub clawback_checkpoint: Option<u64>,
    pub token: Address,
    pub streamed_amount: i128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Sep38Rate {
    pub base_asset: Address,
    pub quote_asset: String,
    pub rate: i128,
    pub scale: i128,
    pub oracle_timestamp: u64,
    pub source_ledger_sequence: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Sep38Quote {
    pub base_asset: Address,
    pub quote_asset: String,
    pub token_amount: i128,
    pub fiat_value: i128,
    pub rate: i128,
    pub scale: i128,
    pub oracle_timestamp: u64,
    pub source_ledger_sequence: u32,
    pub price_data_missing: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ClaimFiatValue {
    pub grant_id: u64,
    pub claim_index: u64,
    pub recipient: Address,
    pub token_address: Address,
    pub token_amount: i128,
    pub fiat_value: i128,
    pub fiat_asset: String,
    pub rate: i128,
    pub rate_scale: i128,
    pub oracle_timestamp: u64,
    pub oracle_ledger_sequence: u32,
    pub claim_ledger_sequence: u32,
    pub claim_ledger_timestamp: u64,
    pub price_data_missing: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct GracePeriodOracle {
    pub grace_period_seconds: u64,
    pub expected_ledger_secs: u64,
    pub grace_period_ledgers: u32,
    pub slippage_ledgers: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct GracePeriodState {
    pub default_ledger: u32,
    pub grace_deadline: u32,
    pub slippage_ledgers: u32,
    pub missed_distributions: u32,
    pub missed_amount: i128,
    pub paid_amount: i128,
    pub late_fee: i128,
    pub resolved: bool,
}

// Legacy DataKey alias preserved for backward compatibility.
// All runtime storage uses `StorageKey` directly.
type DataKey = StorageKey;

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
enum ConfidentialNullifierKey {
    Claim(Bytes),
}

#[contracterror]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum GrantStreamError {
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
    NotValidator = 14,
    KycMissing = 15,
    GrantNotPurgeable = 16,
    OraclePriceFrozen = 17,
    SoftPaused = 18,
    GrantInitializationHalted = 19,
    OracleFrozen = 20,
    RentPreservationMode = 21,
    InvalidTimestamp = 22,
    InvalidRecipient = 30,
    InvalidZKProof = 31,
    WithdrawalBelowMinimum = 32,
    ClawbackAlreadyExecuted = 33,
    InvalidClawbackReason = 34,
    NotDonorOrMultiSig = 35,
    DisputeEscrowNotFound = 36,
    InvalidNonce = 37,
    SubmissionDepositNotFound = 38,
    NotRegisteredSigner = 39,
    ProposalNotFound = 40,
    ProposalNotPending = 41,
    InsufficientApprovals = 42,
    NotSanityOracle = 43,
}

pub type Error = GrantStreamError;

impl From<optimized::Error> for GrantStreamError {
    fn from(e: optimized::Error) -> Self {
        match e {
            optimized::Error::NotInitialized => GrantStreamError::NotInitialized,
            optimized::Error::AlreadyInitialized => GrantStreamError::AlreadyInitialized,
            optimized::Error::NotAuthorized => GrantStreamError::NotAuthorized,
            optimized::Error::GrantNotFound => GrantStreamError::GrantNotFound,
            optimized::Error::GrantAlreadyExists => GrantStreamError::GrantAlreadyExists,
            optimized::Error::InvalidRate => GrantStreamError::InvalidRate,
            optimized::Error::InvalidAmount => GrantStreamError::InvalidAmount,
            optimized::Error::InvalidState => GrantStreamError::InvalidState,
            optimized::Error::MathOverflow => GrantStreamError::MathOverflow,
            optimized::Error::SoftPaused => GrantStreamError::SoftPaused,
            optimized::Error::OracleFrozen => GrantStreamError::OracleFrozen,
            _ => GrantStreamError::NotAuthorized,
        }
    }
}

// --- Internal Helpers ---

fn read_admin(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&StorageKey::Admin).ok_or(Error::NotInitialized)
}

fn read_oracle(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&StorageKey::Oracle).ok_or(Error::NotInitialized)
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
    env.storage().instance().get(&StorageKey::Grant(grant_id)).ok_or(Error::GrantNotFound)
}

fn write_grant(env: &Env, grant_id: u64, grant: &Grant) {
    env.storage().instance().set(&StorageKey::Grant(grant_id), grant);
}

fn read_grant_token(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&StorageKey::GrantToken).ok_or(Error::NotInitialized)
}

fn default_sep38_fiat(env: &Env) -> String {
    env.storage()
        .instance()
        .get(&StorageKey::Sep38DefaultFiat)
        .unwrap_or_else(|| String::from_str(env, "USD"))
}

fn read_sep38_rate(env: &Env, token: &Address, fiat_asset: &String) -> Option<Sep38Rate> {
    env.storage()
        .instance()
        .get(&StorageKey::Sep38Rate(token.clone(), fiat_asset.clone()))
}

fn next_claim_value_index(env: &Env, grant_id: u64) -> u64 {
    let index = env.storage()
        .instance()
        .get(&StorageKey::ClaimValueCounter(grant_id))
        .unwrap_or(0_u64)
        .saturating_add(1);
    env.storage().instance().set(&StorageKey::ClaimValueCounter(grant_id), &index);
    index
}

fn quote_sep38_claim(
    env: &Env,
    token_addr: &Address,
    amount: i128,
    fiat_asset: &String,
) -> Sep38Quote {
    if let Some(rate) = read_sep38_rate(env, token_addr, fiat_asset) {
        let now = env.ledger().timestamp();
        let fresh = rate.base_asset == *token_addr
            && rate.rate > 0
            && rate.scale > 0
            && rate.oracle_timestamp <= now
            && now.saturating_sub(rate.oracle_timestamp) <= SEP38_STALENESS_SECONDS;
        if fresh {
            if let Some(fiat_value) = amount
                .checked_mul(rate.rate)
                .and_then(|value| value.checked_div(rate.scale))
            {
                return Sep38Quote {
                    base_asset: token_addr.clone(),
                    quote_asset: fiat_asset.clone(),
                    token_amount: amount,
                    fiat_value,
                    rate: rate.rate,
                    scale: rate.scale,
                    oracle_timestamp: rate.oracle_timestamp,
                    source_ledger_sequence: rate.source_ledger_sequence,
                    price_data_missing: false,
                };
            }
        }
    }

    Sep38Quote {
        base_asset: token_addr.clone(),
        quote_asset: fiat_asset.clone(),
        token_amount: amount,
        fiat_value: 0,
        rate: 0,
        scale: SCALING_FACTOR,
        oracle_timestamp: 0,
        source_ledger_sequence: 0,
        price_data_missing: true,
    }
}

fn record_claim_value(
    env: &Env,
    grant_id: u64,
    recipient: &Address,
    token_addr: &Address,
    amount: i128,
) -> ClaimFiatValue {
    let fiat_asset = default_sep38_fiat(env);
    let quote = quote_sep38_claim(env, token_addr, amount, &fiat_asset);
    let claim_index = next_claim_value_index(env, grant_id);
    let claim_value = ClaimFiatValue {
        grant_id,
        claim_index,
        recipient: recipient.clone(),
        token_address: token_addr.clone(),
        token_amount: amount,
        fiat_value: quote.fiat_value,
        fiat_asset,
        rate: quote.rate,
        rate_scale: quote.scale,
        oracle_timestamp: quote.oracle_timestamp,
        oracle_ledger_sequence: quote.source_ledger_sequence,
        claim_ledger_sequence: env.ledger().sequence(),
        claim_ledger_timestamp: env.ledger().timestamp(),
        price_data_missing: quote.price_data_missing,
    };
    env.storage()
        .instance()
        .set(&StorageKey::ClaimValue(grant_id, claim_index), &claim_value);
    claim_value
}

fn read_treasury(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&StorageKey::Treasury).ok_or(Error::NotInitialized)
}

fn build_grace_period_oracle(
    expected_ledger_secs: u64,
    slippage_ledgers: u32,
) -> Result<GracePeriodOracle, Error> {
    if expected_ledger_secs == 0 {
        return Err(Error::InvalidTimestamp);
    }

    let ledgers = GRACE_PERIOD_SECONDS
        .checked_add(expected_ledger_secs - 1)
        .ok_or(Error::MathOverflow)?
        .checked_div(expected_ledger_secs)
        .ok_or(Error::MathOverflow)?;
    if ledgers == 0 || ledgers > u32::MAX as u64 {
        return Err(Error::MathOverflow);
    }

    Ok(GracePeriodOracle {
        grace_period_seconds: GRACE_PERIOD_SECONDS,
        expected_ledger_secs,
        grace_period_ledgers: ledgers as u32,
        slippage_ledgers,
    })
}

fn default_grace_period_oracle() -> Result<GracePeriodOracle, Error> {
    build_grace_period_oracle(
        DEFAULT_EXPECTED_LEDGER_DURATION_SECONDS,
        GRACE_PERIOD_SLIPPAGE_LEDGERS,
    )
}

fn read_grace_period_oracle(env: &Env) -> Result<GracePeriodOracle, Error> {
    match env.storage().instance().get(&StorageKey::GracePeriodOracle) {
        Some(oracle) => Ok(oracle),
        None => default_grace_period_oracle(),
    }
}

fn late_fee_for_missed_amount(missed_amount: i128) -> Result<i128, Error> {
    if missed_amount < 0 {
        return Err(Error::InvalidAmount);
    }
    missed_amount
        .checked_mul(LATE_FEE_BPS)
        .ok_or(Error::MathOverflow)?
        .checked_div(10_000)
        .ok_or(Error::MathOverflow)
}

fn grace_period_is_open(
    state: &GracePeriodState,
    current_ledger: u32,
) -> bool {
    if state.resolved {
        return false;
    }
    if current_ledger < state.grace_deadline {
        return true;
    }
    current_ledger == state.grace_deadline
        && state.grace_deadline.saturating_sub(current_ledger) < state.slippage_ledgers
}

fn read_grant_ids(env: &Env) -> Vec<u64> {
    env.storage()
        .instance()
        .get(&StorageKey::GrantIds)
        .unwrap_or_else(|| Vec::new(env))
}

fn read_expected_milestone_nonce(env: &Env, grant_id: u64) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::MilestoneSubmitNonce(grant_id))
        .unwrap_or(0)
}

fn write_expected_milestone_nonce(env: &Env, grant_id: u64, next_nonce: u64) {
    env.storage()
        .instance()
        .set(&DataKey::MilestoneSubmitNonce(grant_id), &next_nonce);
}

const MILESTONE_SUBMISSION_DEPOSIT_XLM: i128 = 5_0000000;

fn set_milestone_submission_deposit(env: &Env, grant_id: u64, milestone_index: u32, amount: i128) {
    env.storage()
        .instance()
        .set(&DataKey::MilestoneSubmissionDeposit(grant_id, milestone_index), &amount);
}

fn get_milestone_submission_deposit(env: &Env, grant_id: u64, milestone_index: u32) -> Option<i128> {
    env.storage()
        .instance()
        .get(&DataKey::MilestoneSubmissionDeposit(grant_id, milestone_index))
}

fn read_confidential_commitment(env: &Env, grant_id: u64) -> Result<i128, Error> {
    env.storage()
        .instance()
        .get(&DataKey::ConfidentialGrantCommitment(grant_id))
        .ok_or(Error::GrantNotFound)
}

fn write_confidential_commitment(env: &Env, grant_id: u64, commitment: i128) {
    env.storage()
        .instance()
        .set(&DataKey::ConfidentialGrantCommitment(grant_id), &commitment);
}

fn read_confidential_recipient(env: &Env, grant_id: u64) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&DataKey::ConfidentialGrantRecipient(grant_id))
        .ok_or(Error::GrantNotFound)
}

#[inline]
fn verify_confidential_claim_proof(
    env: &Env,
    grant_id: u64,
    commitment_before: i128,
    claim_amount: i128,
    nullifier: &Bytes,
    proof: &Bytes,
) -> Result<i128, Error> {
    if claim_amount <= 0 || commitment_before <= 0 {
        return Err(Error::InvalidZKProof);
    }
    let verifier_key_hash: Bytes = env
        .storage()
        .instance()
        .get(&DataKey::ConfidentialGrantVerifierKeyHash(grant_id))
        .ok_or(Error::InvalidZKProof)?;
    let claim_commitment = claim_amount
        .checked_rem_euclid(ZK_COMMITMENT_MODULUS)
        .ok_or(Error::MathOverflow)?;
    if claim_commitment > commitment_before {
        return Err(Error::InvalidZKProof);
    }
    let commitment_after = commitment_before
        .checked_sub(claim_commitment)
        .ok_or(Error::MathOverflow)?;

    let mut public_inputs = Bytes::new(env);
    for byte in grant_id.to_be_bytes() {
        public_inputs.push_back(byte);
    }
    public_inputs.append(&commitment_before.to_xdr(env));
    public_inputs.append(&commitment_after.to_xdr(env));
    public_inputs.append(&claim_amount.to_xdr(env));
    public_inputs.append(nullifier);
    public_inputs.append(&verifier_key_hash);
    let expected_proof: Bytes = env.crypto().sha256(&public_inputs).into();
    if proof != &expected_proof {
        return Err(Error::InvalidZKProof);
    }
    Ok(commitment_after)
}

fn total_allocated_funds(env: &Env) -> Result<i128, Error> {
    let mut total = 0_i128;
    let ids = read_grant_ids(env);
    for i in 0..ids.len() {
        let grant_id = ids.get(i).unwrap();
        if let Some(grant) = env.storage().instance().get::<_, Grant>(&StorageKey::Grant(grant_id)) {
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

fn preview_grant_at_now(grant: &Grant, now: u64) -> Result<Grant, Error> {
    let mut preview = grant.clone();
    settle_grant(&mut preview, now)?;
    Ok(preview)
}

fn count_active_grants(env: &Env) -> u32 {
    let mut count = 0_u32;
    let ids = read_grant_ids(env);
    for i in 0..ids.len() {
        let grant_id = ids.get(i).unwrap();
        if let Some(grant) = env.storage().instance().get::<_, Grant>(&StorageKey::Grant(grant_id)) {
            if grant.status == GrantStatus::Active || grant.status == GrantStatus::Paused {
                count = count.saturating_add(1);
            }
        }
    }
    count
}

fn require_donor_or_multisig_auth(env: &Env, grant: &Grant) -> Result<(), Error> {
    match &grant.donor {
        Some(donor) => {
            donor.require_auth();
            Ok(())
        }
        None => {
            // If no donor is set, require admin authorization (DAO multi-sig)
            require_admin_auth(env)
        }
    }
}

fn calculate_unearned_balance(grant: &Grant) -> Result<i128, Error> {
    let total_earned = grant.withdrawn
        .checked_add(grant.claimable).ok_or(Error::MathOverflow)?
        .checked_add(grant.validator_withdrawn).ok_or(Error::MathOverflow)?
        .checked_add(grant.validator_claimable).ok_or(Error::MathOverflow)?;
    
    grant.total_amount.checked_sub(total_earned).ok_or(Error::MathOverflow)
}

fn set_clawback_checkpoint(env: &Env, grant_id: u64, timestamp: u64) {
    env.storage().instance().set(&StorageKey::ClawbackCheckpoint(grant_id), &timestamp);
}

fn get_clawback_checkpoint(env: &Env, grant_id: u64) -> Option<u64> {
    env.storage().instance().get(&StorageKey::ClawbackCheckpoint(grant_id))
}

fn set_dispute_escrow(env: &Env, grant_id: u64, amount: i128) {
    env.storage().instance().set(&StorageKey::DisputeEscrow(grant_id), &amount);
}

fn get_dispute_escrow(env: &Env, grant_id: u64) -> Option<i128> {
    env.storage().instance().get(&StorageKey::DisputeEscrow(grant_id))
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

/// Splits `accrued` tokens between the grantee (95%) and the validator (5%).
/// When no validator is set the full amount goes to the grantee.
fn apply_accrued_split(grant: &mut Grant, accrued: i128) -> Result<(), Error> {
    if grant.validator.is_some() && accrued > 0 {
        // ROUNDING BEHAVIOR: Integer division with checked_div truncates toward zero
        // (rounds down for positive numbers). This is INTENTIONAL and CORRECT.
        // It ensures the contract always retains any fractional remainder, preventing
        // the "Point One Cent" exploit where rounding up could slowly drain the contract
        // beyond its obligations over many transactions.
        // See test_point_one_cent_exploit.rs for comprehensive proof.
        let validator_share = accrued
            .checked_mul(500)
            .ok_or(Error::MathOverflow)?
            .checked_div(10000)
            .ok_or(Error::MathOverflow)?;
        let grantee_share = accrued
            .checked_sub(validator_share)
            .ok_or(Error::MathOverflow)?;
        grant.claimable = grant.claimable
            .checked_add(grantee_share)
            .ok_or(Error::MathOverflow)?;
        grant.validator_claimable = grant.validator_claimable
            .checked_add(validator_share)
            .ok_or(Error::MathOverflow)?;
    } else {
        grant.claimable = grant.claimable
            .checked_add(accrued)
            .ok_or(Error::MathOverflow)?;
    }
    Ok(())
}

fn settle_grant(grant: &mut Grant, now: u64) -> Result<(), Error> {
    if now < grant.last_update_ts { return Err(Error::InvalidState); }
    
    let elapsed = now - grant.last_update_ts;
    if elapsed == 0 {
        return Ok(());
    }

    if grant.status == GrantStatus::Active {
        // Prevent accrual if legal signature is required but not provided
        if grant.requires_legal_signature && !grant.is_legal_signed {
            grant.last_update_ts = now;
            return Ok(());
        }

        // Handle pending rate increases first
        if grant.pending_rate > grant.flow_rate && grant.effective_timestamp != 0 && now >= grant.effective_timestamp {
            let switch_ts = grant.effective_timestamp;
            // Settle up to switch_ts at old rate
            let pre_elapsed = switch_ts - grant.last_update_ts;
            let pre_accrued = calculate_accrued(grant, pre_elapsed, switch_ts)?;
            apply_accrued_split(grant, pre_accrued)?;

            // Apply new rate
            grant.flow_rate = grant.pending_rate;
            grant.rate_updated_at = switch_ts;
            grant.pending_rate = 0;
            grant.effective_timestamp = 0;
            grant.last_update_ts = switch_ts;

            // Recalculate remaining elapsed
            let post_elapsed = now - switch_ts;
            let post_accrued = calculate_accrued(grant, post_elapsed, now)?;
            apply_accrued_split(grant, post_accrued)?;
        } else {
            let accrued = calculate_accrued(grant, elapsed, now)?;
            apply_accrued_split(grant, accrued)?;
        }
    }

    let total_accounted = grant.withdrawn
        .checked_add(grant.claimable).ok_or(Error::MathOverflow)?
        .checked_add(grant.validator_withdrawn).ok_or(Error::MathOverflow)?
        .checked_add(grant.validator_claimable).ok_or(Error::MathOverflow)?;
    if total_accounted >= grant.total_amount {
        // Cap remaining claimable so total does not exceed total_amount
        let already_paid = grant.withdrawn
            .checked_add(grant.validator_withdrawn).ok_or(Error::MathOverflow)?
            .checked_add(grant.validator_claimable).ok_or(Error::MathOverflow)?;
        grant.claimable = grant.total_amount
            .checked_sub(already_paid).ok_or(Error::MathOverflow)?
            .max(0);
        grant.status = GrantStatus::Completed;
    }

    grant.last_update_ts = now;
    Ok(())
}

 pub fn claim_milestone_funds(env: Env, grant_id: u64, milestone_index: u32) -> i128 {
        nonreentrant!(env, {
            // ── Auth ─────────────────────────────────────────────────────
            let grant: Grant = env
                .storage()
                .persistent()
                .get(&StorageKey::Grant(grant_id))
                .expect("grant not found");
 
            grant.recipient.require_auth();
 
            // ── Milestone validation ──────────────────────────────────────
            // (existing logic unchanged)
            let milestone_key = StorageKey::Milestone(grant_id, milestone_index);
            let milestone_proof: Symbol = env
                .storage()
                .persistent()
                .get(&milestone_key)
                .expect("milestone not found or not yet submitted");
 
            // ── Compute claimable amount ──────────────────────────────────
            // (existing streaming / milestone calculation — unchanged)
            let claimable: i128 = 0; // placeholder — replace with real logic
 
            // ── Cross-contract token transfer ─────────────────────────────
            // This is the call that could trigger a malicious callback.
            // The nonreentrant guard is already set before we reach this line.
            let token_client = token::Client::new(&env, &grant.token);
            token_client.transfer(
                &env.current_contract_address(),
                &grant.recipient,
                &claimable,
            );
 
            // ── Update streamed amount ────────────────────────────────────
            // State is mutated AFTER the external call — kept here for
            // compatibility with existing logic; the guard makes it safe.
            let mut updated_grant = grant.clone();
            updated_grant.streamed_amount += claimable;
            env.storage()
                .persistent()
                .set(&StorageKey::Grant(grant_id), &updated_grant);
 
            // ── Emit event ────────────────────────────────────────────────
            env.events().publish(
                (soroban_sdk::symbol_short!("milestone"),),
                (grant_id, milestone_index, claimable),
            );
 
            claimable
            // ← reentrancy_exit() fires here via macro before returning
        })
    }

     pub fn emergency_governance_withdraw(
        env: Env,
        grant_id: u64,
        destination: Address,
    ) -> i128 {
        nonreentrant!(env, {
            // ── Governance auth ───────────────────────────────────────────
            let admin: Address = env
                .storage()
                .instance()
                .get(&StorageKey::Admin)
                .expect("admin not set");
            admin.require_auth();
 
            // ── Fetch grant ───────────────────────────────────────────────
            let grant: Grant = env
                .storage()
                .persistent()
                .get(&StorageKey::Grant(grant_id))
                .expect("grant not found");
 
            // ── Compute remaining balance ─────────────────────────────────
            let remaining = grant.total_amount - grant.streamed_amount;
            assert!(remaining > 0, "nothing to withdraw");
 
            // ── Cross-contract token transfer ─────────────────────────────
            // Guard is already set — re-entrant callbacks are blocked.
            let token_client = token::Client::new(&env, &grant.token);
            token_client.transfer(
                &env.current_contract_address(),
                &destination,
                &remaining,
            );
 
            // ── Mark grant as fully consumed ──────────────────────────────
            let mut updated_grant = grant.clone();
            updated_grant.streamed_amount = updated_grant.total_amount;
            env.storage()
                .persistent()
                .set(&StorageKey::Grant(grant_id), &updated_grant);
 
            // ── Emit emergency event ──────────────────────────────────────
            env.events().publish(
                (soroban_sdk::symbol_short!("emerg_wd"),),
                (grant_id, destination.clone(), remaining),
            );
 
            remaining
            // ← reentrancy_exit() fires here via macro before returning
        })
    }
 
    // … all other existing entry-points are UNCHANGED …

 

fn calculate_accrued(grant: &Grant, elapsed: u64, now: u64) -> Result<i128, Error> {
    let elapsed_i128 = i128::from(elapsed);
    let base_accrued = grant.flow_rate.checked_mul(elapsed_i128).ok_or(Error::MathOverflow)?;

    let multiplier = calculate_warmup_multiplier(grant, now);
    // ROUNDING BEHAVIOR: Division rounds down (truncates toward zero).
    // This ensures accrued amounts never exceed what should be paid out,
    // maintaining the contract's solvency. See test_point_one_cent_exploit.rs.
    let accrued = base_accrued
        .checked_mul(multiplier)
        .ok_or(Error::MathOverflow)?
        .checked_div(10000)
        .ok_or(Error::MathOverflow)?;

    Ok(accrued)
}

// --- Contract Implementation ---

#[contract]
pub struct GrantStreamContract;

#[contractimpl]
impl GrantStreamContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        grant_token: Address,
        treasury: Address,
        oracle: Address,
        native_token: Address,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage().instance().set(&StorageKey::GrantToken, &grant_token);
        env.storage().instance().set(&StorageKey::Treasury, &treasury);
        env.storage().instance().set(&StorageKey::Oracle, &oracle);
        env.storage().instance().set(&StorageKey::NativeToken, &native_token);
        env.storage().instance().set(&StorageKey::GrantIds, &Vec::<u64>::new(&env));
        env.storage().instance().set(&StorageKey::Sep38DefaultFiat, &String::from_str(&env, "USD"));
        let grace_oracle = default_grace_period_oracle()?;
        env.storage()
            .instance()
            .set(&StorageKey::GracePeriodOracle, &grace_oracle);
        Ok(())
    }

    pub fn set_sep38_rate(
        env: Env,
        fiat_asset: String,
        rate: i128,
        scale: i128,
        oracle_timestamp: u64,
        source_ledger_sequence: u32,
    ) -> Result<(), Error> {
        require_oracle_auth(&env)?;
        if rate <= 0 || scale <= 0 {
            return Err(Error::InvalidRate);
        }
        if oracle_timestamp == 0 || oracle_timestamp > env.ledger().timestamp() {
            return Err(Error::InvalidTimestamp);
        }

        let token_addr = read_grant_token(&env)?;
        let sep38_rate = Sep38Rate {
            base_asset: token_addr.clone(),
            quote_asset: fiat_asset.clone(),
            rate,
            scale,
            oracle_timestamp,
            source_ledger_sequence,
        };
        env.storage()
            .instance()
            .set(&StorageKey::Sep38Rate(token_addr.clone(), fiat_asset.clone()), &sep38_rate);
        env.storage().instance().set(&StorageKey::Sep38DefaultFiat, &fiat_asset);
        env.events().publish(
            (symbol_short!("sep38set"), token_addr, fiat_asset),
            (rate, scale, oracle_timestamp, source_ledger_sequence),
        );
        Ok(())
    }

    pub fn get_sep38_rate(env: Env, fiat_asset: String) -> Option<Sep38Rate> {
        if let Ok(token_addr) = read_grant_token(&env) {
            read_sep38_rate(&env, &token_addr, &fiat_asset)
        } else {
            None
        }
    }

    pub fn configure_grace_period_oracle(
        env: Env,
        expected_ledger_secs: u64,
        slippage_ledgers: u32,
    ) -> Result<GracePeriodOracle, Error> {
        require_admin_auth(&env)?;
        let oracle = build_grace_period_oracle(
            expected_ledger_secs,
            slippage_ledgers,
        )?;
        env.storage()
            .instance()
            .set(&StorageKey::GracePeriodOracle, &oracle);
        env.events().publish(
            (symbol_short!("gracecfg"),),
            (
                oracle.expected_ledger_secs,
                oracle.grace_period_ledgers,
                oracle.slippage_ledgers,
            ),
        );
        Ok(oracle)
    }

    pub fn set_expected_ledger_seconds(
        env: Env,
        expected_ledger_secs: u64,
    ) -> Result<GracePeriodOracle, Error> {
        let slippage_ledgers = read_grace_period_oracle(&env)?.slippage_ledgers;
        Self::configure_grace_period_oracle(
            env,
            expected_ledger_secs,
            slippage_ledgers,
        )
    }

    pub fn get_grace_period_oracle(env: Env) -> Result<GracePeriodOracle, Error> {
        read_grace_period_oracle(&env)
    }

    pub fn check_default(
        env: Env,
        grant_id: u64,
        missed_distributions: u32,
        missed_amount: i128,
    ) -> Result<GracePeriodState, Error> {
        require_admin_auth(&env)?;
        let _grant = read_grant(&env, grant_id)?;
        if missed_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        env.storage()
            .instance()
            .set(&StorageKey::MissedDistributionCount(grant_id), &missed_distributions);

        if missed_distributions < MAX_MISSED_DISTRIBUTIONS {
            return Err(Error::InvalidState);
        }

        let state_key = StorageKey::GracePeriodState(grant_id);
        let late_fee = late_fee_for_missed_amount(missed_amount)?;

        let state = match env.storage().instance().get::<_, GracePeriodState>(&state_key) {
            Some(existing) if !existing.resolved => GracePeriodState {
                missed_distributions,
                missed_amount,
                late_fee,
                ..existing
            },
            _ => {
                let oracle = read_grace_period_oracle(&env)?;
                let default_ledger = env.ledger().sequence();
                let grace_deadline = default_ledger
                    .checked_add(oracle.grace_period_ledgers)
                    .ok_or(Error::MathOverflow)?;
                GracePeriodState {
                    default_ledger,
                    grace_deadline,
                    slippage_ledgers: oracle.slippage_ledgers,
                    missed_distributions,
                    missed_amount,
                    paid_amount: 0,
                    late_fee,
                    resolved: false,
                }
            }
        };

        env.storage().instance().set(&state_key, &state);
        env.events().publish(
            (symbol_short!("gracedef"), grant_id),
            (state.default_ledger, state.grace_deadline, state.missed_amount, state.late_fee),
        );
        Ok(state)
    }

    pub fn apply_grace_period(env: Env, grant_id: u64) -> Result<bool, Error> {
        let _grant = read_grant(&env, grant_id)?;
        let state: GracePeriodState = env
            .storage()
            .instance()
            .get(&StorageKey::GracePeriodState(grant_id))
            .ok_or(Error::InvalidState)?;
        let _missed_distributions: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::MissedDistributionCount(grant_id))
            .unwrap_or(0);
        Ok(grace_period_is_open(
            &state,
            env.ledger().sequence(),
        ))
    }

    pub fn process_catchup(
        env: Env,
        grant_id: u64,
        amount: i128,
    ) -> Result<GracePeriodState, Error> {
        let grant = read_grant(&env, grant_id)?;
        grant.recipient.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let state_key = StorageKey::GracePeriodState(grant_id);
        let mut state: GracePeriodState = env
            .storage()
            .instance()
            .get(&state_key)
            .ok_or(Error::InvalidState)?;
        if !grace_period_is_open(&state, env.ledger().sequence()) {
            return Err(Error::InvalidState);
        }

        state.paid_amount = state
            .paid_amount
            .checked_add(amount)
            .ok_or(Error::MathOverflow)?;
        let required = state
            .missed_amount
            .checked_add(state.late_fee)
            .ok_or(Error::MathOverflow)?;
        if state.paid_amount >= required {
            state.resolved = true;
            env.storage()
                .instance()
                .set(&StorageKey::MissedDistributionCount(grant_id), &0_u32);
        }

        env.storage().instance().set(&state_key, &state);
        env.events()
            .publish((symbol_short!("catchup"), grant_id), (amount, state.paid_amount, state.resolved));
        Ok(state)
    }

    pub fn get_grace_period_state(env: Env, grant_id: u64) -> Option<GracePeriodState> {
        env.storage()
            .instance()
            .get(&StorageKey::GracePeriodState(grant_id))
    }

    pub fn create_grant(
        env: Env,
        grant_id: u64,
        recipient: Address,
        total_amount: i128,
        flow_rate: i128,
        warmup_duration: u64,
        validator: Option<Address>,
        donor: Option<Address>,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;

        // Mass dispute trigger: check if grant initialization is halted
        if circuit_breakers::is_grant_initialization_halted(&env) {
            return Err(Error::GrantInitializationHalted);
        }

        if total_amount <= 0 || flow_rate < 0 {
            return Err(Error::InvalidAmount);
        }

        let key = StorageKey::Grant(grant_id);
        if env.storage().instance().has(&key) {
            return Err(Error::GrantAlreadyExists);
        }

        let now = env.ledger().timestamp();
        let grant_token = read_grant_token(&env)?;
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
            stream_type: StreamType::FixedAmount,
            start_time: now,
            warmup_duration,
            validator: validator,
            validator_withdrawn: 0,
            validator_claimable: 0,
            legal_hash: None,
            requires_legal_signature: false,
            is_legal_signed: false,
            pause_reason: None,
            cancellation_initiated_at: 0,
            clawback_eligible: 0,
            donor: donor.clone(),
            clawback_checkpoint: None,
            token: grant_token.clone(),
            streamed_amount: 0,
        };

        env.storage().instance().set(&key, &grant);

        let mut ids = read_grant_ids(&env);
        ids.push_back(grant_id);
        env.storage().instance().set(&StorageKey::GrantIds, &ids);

        let recipient_key = StorageKey::RecipientGrants(recipient.clone());
        let mut user_grants: Vec<u64> = env.storage().instance().get(&recipient_key).unwrap_or(vec![&env]);
        user_grants.push_back(grant_id);
        env.storage().instance().set(&recipient_key, &user_grants);

        let admin = read_admin(&env)?;
        env.events().publish(
            (symbol_short!("strm_cre"), recipient, admin, grant_token, grant_id),
            total_amount,
        );

        Ok(())
    }

    pub fn create_confidential_grant(
        env: Env,
        grant_id: u64,
        recipient: Address,
        amount_commitment: i128,
        verifier_key_hash: Bytes,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;
        if amount_commitment <= 0 || amount_commitment >= ZK_COMMITMENT_MODULUS {
            return Err(Error::InvalidAmount);
        }
        if env
            .storage()
            .instance()
            .has(&DataKey::ConfidentialGrantCommitment(grant_id))
        {
            return Err(Error::GrantAlreadyExists);
        }
        env.storage()
            .instance()
            .set(&DataKey::ConfidentialGrantRecipient(grant_id), &recipient);
        env.storage()
            .instance()
            .set(&DataKey::ConfidentialGrantVerifierKeyHash(grant_id), &verifier_key_hash);
        write_confidential_commitment(&env, grant_id, amount_commitment);
        env.events().publish((symbol_short!("cnfgrant"), grant_id), amount_commitment);
        Ok(())
    }

    pub fn confidential_claim(
        env: Env,
        grant_id: u64,
        claim_amount: i128,
        nullifier: Bytes,
        proof: Bytes,
    ) -> Result<(), Error> {
        let recipient = read_confidential_recipient(&env, grant_id)?;
        recipient.require_auth();

        let nullifier_key = ConfidentialNullifierKey::Claim(nullifier.clone());
        if env.storage().temporary().has(&nullifier_key) {
            return Err(Error::InvalidZKProof);
        }

        let commitment_before = read_confidential_commitment(&env, grant_id)?;
        let commitment_after = verify_confidential_claim_proof(
            &env,
            grant_id,
            commitment_before,
            claim_amount,
            &nullifier,
            &proof,
        )?;
        write_confidential_commitment(&env, grant_id, commitment_after);

        env.storage().temporary().set(&nullifier_key, &true);
        env.storage().temporary().extend_ttl(&nullifier_key, 0, 17280);

        let token_addr = read_grant_token(&env)?;
        let client = token::Client::new(&env, &token_addr);
        client.transfer(&env.current_contract_address(), &recipient, &claim_amount);

        let mut masked_input = Bytes::new(&env);
        masked_input.append(&nullifier);
        masked_input.append(&claim_amount.to_xdr(&env));
        let masked_amount: Bytes = env.crypto().sha256(&masked_input).into();
        env.events()
            .publish((symbol_short!("cnfclaim"),), (nullifier, masked_amount));
        Ok(())
    }

    pub fn withdraw(env: Env, grant_id: u64, amount: i128) -> Result<(), Error> {
        let mut grant = read_grant(&env, grant_id)?;
        grant.recipient.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        if grant.status == GrantStatus::Cancelled || grant.status == GrantStatus::RageQuitted {
            return Err(Error::InvalidState);
        }

        // ── Clawback Protection: Check if cancellation is pending ─────────────────────
        // If cancellation was initiated, track this withdrawal for potential clawback
        let is_during_cancellation = grant.cancellation_initiated_at != 0;

        // Issue #311: reject withdrawals while SoftPause is active.
        if circuit_breakers::is_soft_paused(&env) {
            return Err(Error::SoftPaused);
        }

        // Emergency Manual Revert: reject withdrawals if oracle is frozen.
        if circuit_breakers::is_oracle_frozen(&env) {
            return Err(Error::OracleFrozen);
        }

        // Storage Rent Depletion: check rent balance and reject non-essential operations
        if !circuit_breakers::is_function_allowed(&env, false) {
            return Err(Error::RentPreservationMode);
        }

        settle_grant(&mut grant, env.ledger().timestamp())?;

        if grant.requires_legal_signature && !grant.is_legal_signed {
            return Err(Error::KycMissing);
        }

        if grant.claimable < MIN_WITHDRAWAL {
            return Err(Error::WithdrawalBelowMinimum);
        }

        if amount > grant.claimable {
            return Err(Error::InvalidAmount);
        }

        grant.claimable = grant.claimable.checked_sub(amount).ok_or(Error::MathOverflow)?;
        grant.withdrawn = grant.withdrawn.checked_add(amount).ok_or(Error::MathOverflow)?;
        grant.last_claim_time = env.ledger().timestamp();

        // ── Clawback Protection: Track withdrawal during cancellation window ──────────
        if is_during_cancellation {
            grant.clawback_eligible = grant.clawback_eligible
                .checked_add(amount)
                .ok_or(Error::MathOverflow)?;
        }

        write_grant(&env, grant_id, &grant);

        // Issue #311: record withdrawal velocity; may engage SoftPause.
        let _ = circuit_breakers::record_withdrawal_velocity(&env, amount)?;

        // Storage Rent Depletion: check rent balance after withdrawal
        circuit_breakers::check_rent_balance(&env);

        let token_addr = read_grant_token(&env)?;
        let client = token::Client::new(&env, &token_addr);
        let target = grant.redirect.unwrap_or(grant.recipient.clone());
        client.transfer(&env.current_contract_address(), &target, &amount);

        let claim_value = record_claim_value(&env, grant_id, &grant.recipient, &token_addr, amount);

        let admin = read_admin(&env)?;
        env.events().publish(
            (symbol_short!("withdraw"), grant.recipient.clone(), admin, token_addr.clone(), grant_id),
            amount,
        );
        env.events().publish(
            (symbol_short!("claimval"), grant.recipient.clone(), token_addr.clone(), grant_id),
            claim_value.clone(),
        );
        if claim_value.price_data_missing {
            env.events().publish(
                (symbol_short!("prcmiss"), grant.recipient.clone(), token_addr, grant_id),
                claim_value.clone(),
            );
        }

        try_call_on_withdraw(&env, &grant.recipient, grant_id, amount);

        // Issue #323: record cumulative flow for tax-reporting export.
        tax_reporting::record_flow(&env, &grant.recipient, grant.withdrawn);

        // Issue #322: update Merkle audit leaf for this grant.
        audit_log::update_audit_leaf(&env, grant_id, grant.withdrawn);

        Ok(())
    }

    pub fn pause_stream(env: Env, grant_id: u64, reason: Option<String>) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Active { return Err(Error::InvalidState); }
        
        settle_grant(&mut grant, env.ledger().timestamp())?;
        grant.status = GrantStatus::Paused;
        grant.pause_reason = reason.clone();
        write_grant(&env, grant_id, &grant);
        
        // Emit ProtocolPaused event with reason
        let admin = read_admin(&env)?;
        let pause_reason_str = reason.unwrap_or_else(|| String::from_str(&env, "No reason provided"));
        env.events().publish(
            (symbol_short!("protopaus"), admin, grant_id),
            pause_reason_str,
        );
        Ok(())
    }

    pub fn resume_stream(env: Env, grant_id: u64) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Paused { return Err(Error::InvalidState); }

        grant.status = GrantStatus::Active;
        grant.last_update_ts = env.ledger().timestamp();
        grant.pause_reason = None; // Clear pause reason on resume
        write_grant(&env, grant_id, &grant);
        Ok(())
    }

    /// Emergency pause function for protocol-wide emergency stops
    /// Stores the reason in contract state and emits ProtocolPaused event
    pub fn emergency_pause(env: Env, reason: String) -> Result<(), Error> {
        require_admin_auth(&env)?;
        
        // Store the emergency pause reason in contract state
        env.storage().instance().set(&StorageKey::ProtocolPauseReason, &reason);
        
        // Emit ProtocolPaused event for protocol-wide emergency pause
        let admin = read_admin(&env)?;
        env.events().publish(
            (symbol_short!("protopaus"), admin, symbol_short!("emerg")),
            reason,
        );
        
        Ok(())
    }

    /// Get the pause reason for a specific grant
    pub fn get_pause_reason(env: Env, grant_id: u64) -> Result<Option<String>, Error> {
        let grant = read_grant(&env, grant_id)?;
        Ok(grant.pause_reason)
    }

    /// Get the protocol-wide emergency pause reason
    pub fn get_protocol_pause_reason(env: Env) -> Option<String> {
        env.storage().instance().get(&StorageKey::ProtocolPauseReason)
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
        
        let admin = read_admin(&env)?;
        let grant_token = read_grant_token(&env)?;
        env.events().publish(
            (symbol_short!("rateupdt"), grant.recipient.clone(), admin, grant_token, grant_id),
            (old_rate, new_rate),
        );
        Ok(())
    }

    pub fn apply_kpi_multiplier(env: Env, grant_id: u64, multiplier: i128) -> Result<(), Error> {
        require_oracle_auth(&env)?;
        if multiplier <= 0 { return Err(Error::InvalidRate); }

        // Issue #312: block price-dependent operations while oracle is frozen.
        if circuit_breakers::is_oracle_frozen(&env) {
            return Err(Error::OraclePriceFrozen);
        }

        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Active { return Err(Error::InvalidState); }

        settle_grant(&mut grant, env.ledger().timestamp())?;
        
        let old_rate = grant.flow_rate;
        grant.flow_rate = grant.flow_rate.checked_mul(multiplier).ok_or(Error::MathOverflow)? / 10000;
        if grant.pending_rate > 0 {
            grant.pending_rate = grant.pending_rate.checked_mul(multiplier).ok_or(Error::MathOverflow)? / 10000;
        }
        grant.rate_updated_at = env.ledger().timestamp();

        write_grant(&env, grant_id, &grant);
        
        let admin = read_admin(&env)?;
        let grant_token = read_grant_token(&env)?;
        env.events().publish(
            (symbol_short!("kpimul"), grant.recipient.clone(), admin, grant_token, grant_id),
            (old_rate, grant.flow_rate, multiplier),
        );
        Ok(())
    }

    pub fn rage_quit(env: Env, grant_id: u64) -> Result<(), Error> {
        let mut grant = read_grant(&env, grant_id)?;
        grant.recipient.require_auth();

        if grant.status != GrantStatus::Paused { return Err(Error::InvalidState); }

        settle_grant(&mut grant, env.ledger().timestamp())?;

        let claim_amount = grant.claimable;
        let validator_amount = grant.validator_claimable;
        grant.claimable = 0;
        grant.validator_claimable = 0;
        grant.withdrawn = grant.withdrawn.checked_add(claim_amount).ok_or(Error::MathOverflow)?;
        grant.validator_withdrawn = grant.validator_withdrawn.checked_add(validator_amount).ok_or(Error::MathOverflow)?;
        grant.status = GrantStatus::RageQuitted;

        let total_paid = grant.withdrawn
            .checked_add(grant.validator_withdrawn)
            .ok_or(Error::MathOverflow)?;
        let remaining = grant.total_amount.checked_sub(total_paid).ok_or(Error::MathOverflow)?;
        write_grant(&env, grant_id, &grant);

        let token_addr = read_grant_token(&env)?;
        let client = token::Client::new(&env, &token_addr);
        client.transfer(&env.current_contract_address(), &grant.recipient, &claim_amount);

        // Pay out the validator's accrued share on rage quit
        if validator_amount > 0 {
            if let Some(ref validator_addr) = grant.validator {
                client.transfer(&env.current_contract_address(), validator_addr, &validator_amount);
            }
        }

        if remaining > 0 {
            let treasury = read_treasury(&env)?;
            client.transfer(&env.current_contract_address(), &treasury, &remaining);
        }

        Ok(())
    }

    pub fn cancel_grant(env: Env, grant_id: u64) -> Result<(), Error> {
        let mut grant = read_grant(&env, grant_id)?;
        require_admin_auth(&env)?;

        if grant.status == GrantStatus::Completed || grant.status == GrantStatus::RageQuitted {
            return Err(Error::InvalidState);
        }

        settle_grant(&mut grant, env.ledger().timestamp())?;

        // ── Clawback Protection: Mark cancellation as initiated ──────────────────────────
        // This prevents race conditions where withdrawals happen during Stellar ledger close
        let now = env.ledger().timestamp();
        grant.cancellation_initiated_at = now;
        grant.clawback_eligible = 0; // Initialize to zero; will be set if withdrawals occur

        // Remaining = total - already withdrawn - pending claimable (both sides)
        let total_paid = grant.withdrawn
            .checked_add(grant.validator_withdrawn).ok_or(Error::MathOverflow)?
            .checked_add(grant.claimable).ok_or(Error::MathOverflow)?
            .checked_add(grant.validator_claimable).ok_or(Error::MathOverflow)?;
        let remaining = grant.total_amount.checked_sub(total_paid).ok_or(Error::MathOverflow)?;
        grant.status = GrantStatus::Cancelled;
        write_grant(&env, grant_id, &grant);

        // ── Emit cancellation event for audit trail ────────────────────────────────────
        let admin = read_admin(&env)?;
        env.events().publish(
            (symbol_short!("cancel"), admin, grant_id),
            (now, remaining),
        );

        if remaining > 0 {
            let token_addr = read_grant_token(&env)?;
            let client = token::Client::new(&env, &token_addr);
            let treasury = read_treasury(&env)?;
            client.transfer(&env.current_contract_address(), &treasury, &remaining);
        }

        Ok(())
    }

    /// Change the grantee (recipient) of an active grant.
    /// This enables team migrations and grant transfers with proper authorization.
    /// Requires admin authorization for security.
    pub fn change_grantee(env: Env, grant_id: u64, new_grantee: Address) -> Result<(), Error> {
        require_admin_auth(&env)?;
        
        let mut grant = read_grant(&env, grant_id)?;
        
        // Only allow changing grantee for active or paused grants
        if grant.status != GrantStatus::Active && grant.status != GrantStatus::Paused {
            return Err(Error::InvalidState);
        }
        
        // Prevent changing to the same grantee
        if grant.recipient == new_grantee {
            return Err(Error::InvalidRecipient);
        }
        
        let old_grantee = grant.recipient.clone();
        
        // Update grant recipient
        grant.recipient = new_grantee.clone();
        
        // Clear any existing redirect since we're changing the primary recipient
        grant.redirect = None;
        
        write_grant(&env, grant_id, &grant);
        
        // Update storage mappings: remove from old grantee's grants
        let old_recipient_key = StorageKey::RecipientGrants(old_grantee.clone());
        let mut old_user_grants: Vec<u64> = env.storage().instance().get(&old_recipient_key).unwrap_or(vec![&env]);
        if let Some(pos) = old_user_grants.iter().position(|id| id == grant_id) {
            old_user_grants.remove(pos as u32);
            env.storage().instance().set(&old_recipient_key, &old_user_grants);
        }
        
        // Add to new grantee's grants
        let new_recipient_key = StorageKey::RecipientGrants(new_grantee.clone());
        let mut new_user_grants: Vec<u64> = env.storage().instance().get(&new_recipient_key).unwrap_or(vec![&env]);
        new_user_grants.push_back(grant_id);
        env.storage().instance().set(&new_recipient_key, &new_user_grants);
        
        // Emit event
        let admin = read_admin(&env)?;
        let grant_token = read_grant_token(&env)?;
        env.events().publish(
            (symbol_short!("grnt_chg"), old_grantee, new_grantee, admin, grant_token, grant_id),
            grant.total_amount,
        );
        
        Ok(())
    }

    /// Trigger clawback of unearned funds from a grant.
    /// Restricted to the original donor or DAO multi-sig.
    /// Calculates unearned balance and instantly terminates the stream.
    pub fn trigger_grant_clawback(
        env: Env,
        grant_id: u64,
        reason: String,
        contested: bool,
    ) -> Result<(), Error> {
        let mut grant = read_grant(&env, grant_id)?;
        
        // Check if clawback has already been executed
        if grant.status == GrantStatus::Clawbacked {
            return Err(Error::ClawbackAlreadyExecuted);
        }
        
        // Only allow clawback for active or paused grants
        if grant.status != GrantStatus::Active && grant.status != GrantStatus::Paused {
            return Err(Error::InvalidState);
        }
        
        // Validate reason is not empty
        if reason.is_empty() {
            return Err(Error::InvalidClawbackReason);
        }
        
        // Require authorization from donor or DAO multi-sig
        require_donor_or_multisig_auth(&env, &grant)?;
        
        let now = env.ledger().timestamp();
        
        // Set checkpoint to prevent double-spending during this operation
        set_clawback_checkpoint(&env, grant_id, now);
        grant.clawback_checkpoint = Some(now);
        
        // Settle grant up to the exact millisecond of clawback
        settle_grant(&mut grant, now)?;
        
        // Calculate unearned balance (Total_Grant - Amount_Already_Streamed_To_Date)
        let unearned_balance = calculate_unearned_balance(&grant)?;
        
        if unearned_balance <= 0 {
            // No unearned funds to clawback, but still mark as clawbacked
            grant.status = GrantStatus::Clawbacked;
            write_grant(&env, grant_id, &grant);
            
            // Emit event even if no funds were clawed back
            let admin = read_admin(&env)?;
            let grant_token = read_grant_token(&env)?;
            env.events().publish(
                (symbol_short!("clawback"), grant.recipient.clone(), admin, grant_token, grant_id),
                (0, reason, contested),
            );
            return Ok(());
        }
        
        // Ensure grantee can claim any funds already vested up to this exact second
        let vested_amount = grant.claimable;
        let validator_vested = grant.validator_claimable;
        
        // Transfer unearned balance based on contest status
        let token_addr = read_grant_token(&env)?;
        let client = token::Client::new(&env, &token_addr);
        
        if contested {
            // Move funds to dispute escrow instead of donor's wallet
            set_dispute_escrow(&env, grant_id, unearned_balance);
            
            // Emit event for disputed clawback
            let admin = read_admin(&env)?;
            env.events().publish(
                (symbol_short!("clawback"), grant.recipient.clone(), admin, token_addr, grant_id),
                (unearned_balance, reason.clone(), true),
            );
        } else {
            // Return unearned balance to donor's vault
            let donor_address = grant.donor.clone().ok_or(Error::NotDonorOrMultiSig)?;
            client.transfer(&env.current_contract_address(), &donor_address, &unearned_balance);
            
            // Emit event for successful clawback
            let admin = read_admin(&env)?;
            env.events().publish(
                (symbol_short!("clawback"), grant.recipient.clone(), admin, token_addr, grant_id),
                (unearned_balance, reason.clone(), false),
            );
        }
        
        // Mark grant as clawbacked
        grant.status = GrantStatus::Clawbacked;
        write_grant(&env, grant_id, &grant);
        
        Ok(())
    }

    /// Resolve a disputed clawback by releasing escrowed funds to the appropriate party
    pub fn resolve_disputed_clawback(
        env: Env,
        grant_id: u64,
        release_to_donor: bool, // true = release to donor, false = return to grant
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;
        
        let grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Clawbacked {
            return Err(Error::InvalidState);
        }
        
        let escrow_amount = get_dispute_escrow(&env, grant_id)
            .ok_or(Error::DisputeEscrowNotFound)?;
        
        if escrow_amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        
        let token_addr = read_grant_token(&env)?;
        let client = token::Client::new(&env, &token_addr);
        
        if release_to_donor {
            // Release to donor
            let donor_address = grant.donor.clone().ok_or(Error::NotDonorOrMultiSig)?;
            client.transfer(&env.current_contract_address(), &donor_address, &escrow_amount);
        } else {
            // Return to grant (resume streaming to grantee)
            let mut updated_grant = grant.clone();
            updated_grant.status = GrantStatus::Active;
            updated_grant.last_update_ts = env.ledger().timestamp();
            
            // Add escrowed amount back to total available for streaming
            updated_grant.total_amount = updated_grant.total_amount
                .checked_add(escrow_amount).ok_or(Error::MathOverflow)?;
            
            write_grant(&env, grant_id, &updated_grant);
            
            // Transfer to treasury to fund the resumed grant
            let treasury = read_treasury(&env)?;
            client.transfer(&env.current_contract_address(), &treasury, &escrow_amount);
        }
        
        // Clear the escrow
        env.storage().instance().remove(&StorageKey::DisputeEscrow(grant_id));
        
        // Emit resolution event
        let admin = read_admin(&env)?;
        env.events().publish(
            (symbol_short!("claw_rslv"), grant_id, admin),
            (escrow_amount, release_to_donor),
        );
        
        Ok(())
    }

    /// Get the current dispute escrow balance for a grant
    pub fn get_dispute_escrow_balance(env: Env, grant_id: u64) -> Result<i128, Error> {
        get_dispute_escrow(&env, grant_id).ok_or(Error::DisputeEscrowNotFound)
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

    // ── Circuit Breaker: Oracle Price Deviation Guard (Issue #312) ────────────

    /// Configure the sanity-check oracle address.  Admin only.
    pub fn set_sanity_oracle(env: Env, sanity_oracle: Address) -> Result<(), Error> {
        require_admin_auth(&env)?;
        circuit_breakers::set_sanity_oracle(&env, &sanity_oracle);
        Ok(())
    }

    /// Submit a new oracle price ping.  If the price deviates >50% from the
    /// last accepted price the oracle guard is tripped and `false` is returned.
    /// Requires oracle auth.
    pub fn submit_oracle_price(env: Env, new_price: i128) -> Result<bool, Error> {
        require_oracle_auth(&env)?;
        if new_price <= 0 { return Err(Error::InvalidAmount); }
        Ok(circuit_breakers::record_oracle_price(&env, new_price))
    }

    /// Sanity-check oracle confirms a suspicious price, clearing the freeze.
    pub fn confirm_oracle_price(env: Env, caller: Address, confirmed_price: i128) -> Result<(), Error> {
        if confirmed_price <= 0 { return Err(Error::InvalidAmount); }
        circuit_breakers::confirm_oracle_price(&env, &caller, confirmed_price)?;
        Ok(())
    }

    /// Returns whether the oracle price circuit breaker is currently active.
    pub fn oracle_frozen(env: Env) -> bool {
        circuit_breakers::is_oracle_frozen(&env)
    }

    // ── Circuit Breaker: TVL Velocity Limit (Issue #311) ──────────────────────

    /// Update the TVL snapshot used for velocity-limit calculations.  Admin only.
    pub fn update_tvl_snapshot(env: Env, total_liquidity: i128) -> Result<(), Error> {
        require_admin_auth(&env)?;
        if total_liquidity < 0 { return Err(Error::InvalidAmount); }
        circuit_breakers::update_tvl_snapshot(&env, total_liquidity);
        Ok(())
    }

    /// Check the contract's rent balance and update rent preservation mode.  Admin only.
    pub fn check_rent_balance(env: Env) -> Result<bool, Error> {
        require_admin_auth(&env)?;
        Ok(circuit_breakers::check_rent_balance(&env))
    }

    /// Get the current rent preservation mode status.
    pub fn is_rent_preservation_mode(env: Env) -> bool {
        circuit_breakers::is_rent_preservation_mode(&env)
    }

    /// Get the contract's current native XLM balance.
    pub fn get_current_xlm_balance(env: Env) -> i128 {
        circuit_breakers::get_current_xlm_balance(&env)
    }

    /// Get the rent buffer threshold (3-month buffer).
    pub fn get_rent_buffer_threshold(env: Env) -> i128 {
        circuit_breakers::get_rent_buffer_threshold(&env)
    }

    /// Admin-only: manually disable rent preservation mode after adding funds.
    pub fn disable_rent_preservation_mode(env: Env) -> Result<(), Error> {
        let admin = read_admin(&env)?;
        circuit_breakers::disable_rent_preservation_mode(&env, &admin);
        Ok(())
    }

    /// Returns whether the contract is currently in SoftPause.
    pub fn soft_paused(env: Env) -> bool {
        circuit_breakers::is_soft_paused(&env)
    }

    /// Admin resumes normal operations after manual verification of a velocity breach.
    pub fn resume_after_velocity_check(env: Env) -> Result<(), Error> {
        let admin = read_admin(&env)?;
        admin.require_auth();
        circuit_breakers::resume_after_velocity_check(&env, &admin);
        Ok(())
    }

    // ── Standard getters ──────────────────────────────────────────────────────

    pub fn get_claim_value(env: Env, grant_id: u64, claim_index: u64) -> Option<ClaimFiatValue> {
        env.storage().instance().get(&StorageKey::ClaimValue(grant_id, claim_index))
    }

    pub fn get_latest_claim_value(env: Env, grant_id: u64) -> Option<ClaimFiatValue> {
        let claim_index = env.storage()
            .instance()
            .get(&StorageKey::ClaimValueCounter(grant_id))
            .unwrap_or(0_u64);
        if claim_index == 0 {
            None
        } else {
            env.storage().instance().get(&StorageKey::ClaimValue(grant_id, claim_index))
        }
    }

    pub fn get_grant(env: Env, grant_id: u64) -> Result<Grant, Error> {
        let mut grant = read_grant(&env, grant_id)?;
        settle_grant(&mut grant, env.ledger().timestamp())?;
        Ok(grant)
    }

    pub fn claimable(env: Env, grant_id: u64) -> i128 {
        if let Ok(mut grant) = read_grant(&env, grant_id) {
            let _ = settle_grant(&mut grant, env.ledger().timestamp());
            grant.claimable
        } else {
            0
        }
    }

    /// Current claimable values for a grant without mutating storage.
    pub fn get_current_claimable_amounts(env: Env, grant_id: u64) -> Result<(i128, i128), Error> {
        let grant = read_grant(&env, grant_id)?;
        let preview = preview_grant_at_now(&grant, env.ledger().timestamp())?;
        Ok((preview.claimable, preview.validator_claimable))
    }

    /// Current grantee claimable amount without mutating storage.
    pub fn get_current_grantee_claimable(env: Env, grant_id: u64) -> Result<i128, Error> {
        let (claimable, _) = Self::get_current_claimable_amounts(env, grant_id)?;
        Ok(claimable)
    }

    /// Current validator claimable amount without mutating storage.
    pub fn get_current_validator_claimable(env: Env, grant_id: u64) -> Result<i128, Error> {
        let (_, validator_claimable) = Self::get_current_claimable_amounts(env, grant_id)?;
        Ok(validator_claimable)
    }

    /// Compute the claimable balance for exponential vesting.
    pub fn compute_exponential_vesting(
        total: u128,
        start: u64,
        now: u64,
        duration: u64,
        factor: u32,
    ) -> u128 {
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

        let progress = (elapsed as u128 * 1000) / (duration as u128); // progress in 0.1% increments
        let factor_scaled = factor as u128; // factor is already scaled by 1000
        
        let progress_squared = match progress.checked_mul(progress) {
            Some(v) => v,
            None => return total,
        };
        
        let factor_progress = match progress_squared.checked_mul(factor_scaled) {
            Some(v) => v,
            None => return total,
        };
        
        let vested = match total.checked_mul(factor_progress) {
            Some(v) => v / 1_000_000_000, 
            None => total,
        };
        
        vested.min(total)
    }

    /// Trigger dispute monitoring for a grant. This should be called when a grant
    /// enters "Dispute" status through the arbitration process.
    pub fn trigger_grant_dispute(env: Env, grant_id: u64) -> Result<(), Error> {
        // Verify the grant exists and is in a state that can be disputed
        let _grant = read_grant(&env, grant_id)?;
        
        // Count current active grants for the dispute monitoring calculation
        let active_grants_count = count_active_grants(&env);
        
        // Record the dispute and check if threshold is breached
        if !circuit_breakers::record_dispute(&env, active_grants_count) {
            // Threshold was breached - emit an event for transparency
            let admin = read_admin(&env)?;
            env.events().publish(
                (symbol_short!("disputecb"),),
                (grant_id, active_grants_count, "Mass dispute threshold breached"),
            );
        }
        
        Ok(())
    }

    /// Get current dispute monitoring statistics for transparency.
    pub fn get_dispute_stats(env: Env) -> (u64, u32, u32, bool) {
        circuit_breakers::get_dispute_monitoring_stats(&env)
    }

    /// Admin-only: resume grant initialization after manual verification of dispute activity.
    pub fn resume_grant_initialization(env: Env) -> Result<(), Error> {
        let admin = read_admin(&env)?;
        circuit_breakers::resume_grant_initialization(&env, &admin);
        
        env.events().publish(
            (symbol_short!("resgrant"),),
            admin,
        );
        
        Ok(())
    }

    /// Compute the claimable balance for logarithmic vesting.
    pub fn compute_logarithmic_vesting(
        total: u128,
        start: u64,
        now: u64,
        duration: u64,
        factor: u32,
    ) -> u128 {
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

        let progress = (elapsed as u128 * 1000) / (duration as u128);
        let factor_scaled = factor as u128;
        
        if progress == 0 {
            return 0;
        }
        
        let progress_factor = match progress.checked_mul(factor_scaled) {
            Some(v) => v,
            None => return total,
        };
        
        let sqrt_progress_factor = Self::integer_sqrt(progress_factor);
        let sqrt_factor = Self::integer_sqrt(factor_scaled);
        
        if sqrt_factor == 0 {
            return 0;
        }
        
        let vested = match total.checked_mul(sqrt_progress_factor) {
            Some(v) => {
                let normalized = match v.checked_mul(1000) {
                    Some(v2) => v2,
                    None => total,
                };
                match normalized.checked_div(sqrt_factor) {
                    Some(v3) => v3 / 1000,
                    None => total,
                }
            }
            None => total,
        };
        
        vested.min(total)
    }
    
    fn integer_sqrt(n: u128) -> u128 {
        if n <= 1 {
            return n;
        }
        
        let mut low = 1u128;
        let mut high = n;
        let mut result = 1u128;
        
        while low <= high {
            let mid = (low + high) / 2;
            let mid_squared = match mid.checked_mul(mid) {
                Some(v) => v,
                None => {
                    high = mid - 1;
                    continue;
                }
            };
            
            if mid_squared == n {
                return mid;
            }
            
            if mid_squared < n {
                low = mid + 1;
                result = mid;
            } else {
                high = mid - 1;
            }
        }
        
        result
    }

    pub fn validator_claimable(env: Env, grant_id: u64) -> i128 {
        if let Ok(mut grant) = read_grant(&env, grant_id) {
            if grant.validator.is_none() {
                return 0;
            }
            let _ = settle_grant(&mut grant, env.ledger().timestamp());
            grant.validator_claimable
        } else {
            0
        }
    }

    pub fn get_validator_info(
        env: Env,
        grant_id: u64,
    ) -> Result<(Option<Address>, i128, i128), Error> {
        let grant = read_grant(&env, grant_id)?;
        Ok((grant.validator, grant.validator_claimable, grant.validator_withdrawn))
    }

    pub fn withdraw_validator(env: Env, grant_id: u64, amount: i128) -> Result<(), Error> {
        let mut grant = read_grant(&env, grant_id)?;
        let validator_addr = grant.validator.clone().ok_or(Error::InvalidState)?;
        validator_addr.require_auth();

        if grant.status == GrantStatus::Cancelled || grant.status == GrantStatus::RageQuitted {
            return Err(Error::InvalidState);
        }

        settle_grant(&mut grant, env.ledger().timestamp())?;

        if amount <= 0 || amount > grant.validator_claimable {
            return Err(Error::InvalidAmount);
        }

        grant.validator_claimable = grant.validator_claimable.checked_sub(amount).ok_or(Error::MathOverflow)?;
        grant.validator_withdrawn = grant.validator_withdrawn.checked_add(amount).ok_or(Error::MathOverflow)?;

        write_grant(&env, grant_id, &grant);

        let token_addr = read_grant_token(&env)?;
        let client = token::Client::new(&env, &token_addr);
        client.transfer(&env.current_contract_address(), &validator_addr, &amount);

        let admin = read_admin(&env)?;
        env.events().publish(
            (symbol_short!("valwdraw"), validator_addr, admin, token_addr, grant_id),
            amount,
        );
        Ok(())
    }

    pub fn set_legal_metadata(
        env: Env,
        grant_id: u64,
        legal_hash: String,
        requires_signature: bool,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let mut grant = read_grant(&env, grant_id)?;
        grant.legal_hash = Some(legal_hash);
        grant.requires_legal_signature = requires_signature;
        write_grant(&env, grant_id, &grant);
        
        let admin = read_admin(&env)?;
        let grant_token = read_grant_token(&env)?;
        env.events().publish(
            (symbol_short!("legalset"), grant.recipient.clone(), admin, grant_token, grant_id),
            requires_signature,
        );
        Ok(())
    }

    pub fn sign_legal_metadata(env: Env, grant_id: u64) -> Result<(), Error> {
        let mut grant = read_grant(&env, grant_id)?;
        grant.recipient.require_auth();
        if grant.legal_hash.is_none() { return Err(Error::InvalidState); }
        grant.is_legal_signed = true;
        grant.last_update_ts = env.ledger().timestamp();
        write_grant(&env, grant_id, &grant);
        
        let admin = read_admin(&env)?;
        let grant_token = read_grant_token(&env)?;
        env.events().publish(
            (symbol_short!("legalsig"), grant.recipient.clone(), admin, grant_token, grant_id),
            env.ledger().timestamp(),
        );
        Ok(())
    }

    /// Submit an off-chain milestone proof with monotonic nonce protection.
    ///
    /// Replay prevention:
    /// - Every grant tracks an expected nonce starting at 0.
    /// - Submission is accepted only when `nonce == expected_nonce`.
    /// - The expected nonce is incremented after a successful write.
    ///
    /// Cancellation edge case:
    /// - Cancelled / rage-quit / completed grants reject new milestone proofs.
    pub fn submit_milestone_proof(
        env: Env,
        grant_id: u64,
        milestone_index: u32,
        proof: Symbol,
        nonce: u64,
    ) -> Result<(), Error> {
        let grant = read_grant(&env, grant_id)?;
        grant.recipient.require_auth();

        if grant.status == GrantStatus::Cancelled
            || grant.status == GrantStatus::RageQuitted
            || grant.status == GrantStatus::Completed
        {
            return Err(Error::InvalidState);
        }

        let milestone_key = DataKey::Milestone(grant_id, milestone_index);
        if env.storage().persistent().has(&milestone_key) {
            return Err(Error::InvalidState);
        }

        let expected_nonce = read_expected_milestone_nonce(&env, grant_id);
        if nonce != expected_nonce {
            return Err(Error::InvalidNonce);
        }

        let native_token_addr: Address = env.storage().instance().get(&StorageKey::NativeToken).ok_or(Error::NotInitialized)?;
        let native_client = token::Client::new(&env, &native_token_addr);
        native_client.transfer(
            &grant.recipient,
            &env.current_contract_address(),
            &MILESTONE_SUBMISSION_DEPOSIT_XLM,
        );

        env.storage().persistent().set(&milestone_key, &proof);
        set_milestone_submission_deposit(
            &env,
            grant_id,
            milestone_index,
            MILESTONE_SUBMISSION_DEPOSIT_XLM,
        );

        let next_nonce = nonce.checked_add(1).ok_or(Error::MathOverflow)?;
        write_expected_milestone_nonce(&env, grant_id, next_nonce);

        env.events().publish(
            (symbol_short!("mil_sub"),),
            (grant_id, milestone_index, nonce),
        );

        Ok(())
    }

    /// Admin-only: approve a milestone submission and refund its anti-spam deposit.
    pub fn approve_milestone_submission(
        env: Env,
        grant_id: u64,
        milestone_index: u32,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let grant = read_grant(&env, grant_id)?;
        let deposit = get_milestone_submission_deposit(&env, grant_id, milestone_index)
            .ok_or(Error::SubmissionDepositNotFound)?;
        let native_token_addr: Address = env.storage().instance().get(&StorageKey::NativeToken).ok_or(Error::NotInitialized)?;
        let native_client = token::Client::new(&env, &native_token_addr);
        native_client.transfer(&env.current_contract_address(), &grant.recipient, &deposit);
        env.storage()
            .instance()
            .remove(&DataKey::MilestoneSubmissionDeposit(grant_id, milestone_index));
        env.events().publish(
            (symbol_short!("mil_depr"),),
            (grant_id, milestone_index, deposit),
        );
        Ok(())
    }

    /// Admin-only: slash a fraudulent milestone submission deposit to treasury.
    pub fn slash_ms_submission_deposit(
        env: Env,
        grant_id: u64,
        milestone_index: u32,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let deposit = get_milestone_submission_deposit(&env, grant_id, milestone_index)
            .ok_or(Error::SubmissionDepositNotFound)?;
        let treasury = read_treasury(&env)?;
        let native_token_addr: Address = env.storage().instance().get(&StorageKey::NativeToken).ok_or(Error::NotInitialized)?;
        let native_client = token::Client::new(&env, &native_token_addr);
        native_client.transfer(&env.current_contract_address(), &treasury, &deposit);
        env.storage()
            .instance()
            .remove(&DataKey::MilestoneSubmissionDeposit(grant_id, milestone_index));
        env.events().publish(
            (symbol_short!("mil_deps"),),
            (grant_id, milestone_index, deposit),
        );
        Ok(())
    }

    pub fn emit_grant_status(env: Env, grant_id: u64) -> Result<soroban_sdk::Bytes, Error> {
        let mut grant = read_grant(&env, grant_id)?;
        settle_grant(&mut grant, env.ledger().timestamp())?;
        let total_value = grant.total_amount;
        let progress_accrued = grant.withdrawn + grant.claimable + grant.validator_withdrawn + grant.validator_claimable;
        let progress_bps = if total_value > 0 { (progress_accrued * 10000) / total_value } else { 0 };
        let mut data = Bytes::new(&env);
        data.append(&grant_id.to_xdr(&env));
        data.append(&total_value.to_xdr(&env));
        data.append(&progress_bps.to_xdr(&env));
        env.events().publish((symbol_short!("grntstat"), grant_id), data.clone());
        Ok(data)
    }

    /// Get the current protocol health factor in basis points without mutating on-chain state.
    pub fn get_health_factor(env: Env) -> Result<i128, Error> {
        let liabilities = total_allocated_funds(&env)?;
        match yield_treasury::YieldTreasuryContract::preview_pool_health(env, liabilities) {
            Ok(hf) => Ok(hf),
            Err(_) => Err(Error::MathOverflow), // Map to generic error for simplicity
        }
    }

    /// Finalizes and purges a completed or cancelled grant to clean up state.
    /// Rewards the purger with a small bounty from the treasury/contract.
    pub fn finalize_and_purge(env: Env, grant_id: u64, purger: Address) -> Result<(), Error> {
        purger.require_auth();

        let mut grant = read_grant(&env, grant_id)?;
        settle_grant(&mut grant, env.ledger().timestamp())?;

        // Verify eligibility for purging
        // 1. Must be Completed or Cancelled
        if grant.status != GrantStatus::Completed && grant.status != GrantStatus::Cancelled {
            return Err(Error::GrantNotPurgeable);
        }

        // 2. Must have no pending claims
        if grant.claimable > 0 || grant.validator_claimable > 0 {
            return Err(Error::GrantNotPurgeable);
        }

        // 3. Completed grants must be fully accounted before deletion.
        // This prevents accidental state deletion if any user funds are still
        // represented in grant accounting due to an upstream bug.
        if grant.status == GrantStatus::Completed {
            let accounted = grant.withdrawn
                .checked_add(grant.claimable).ok_or(Error::MathOverflow)?
                .checked_add(grant.validator_withdrawn).ok_or(Error::MathOverflow)?
                .checked_add(grant.validator_claimable).ok_or(Error::MathOverflow)?;
            if accounted != grant.total_amount {
                return Err(Error::GrantNotPurgeable);
            }
        }

        // Cleanup state: Remove grant from storage
        env.storage().instance().remove(&StorageKey::Grant(grant_id));

        // Update grant ID tracking lists
        let ids = read_grant_ids(&env);
        let mut new_ids: Vec<u64> = Vec::new(&env);
        for id in ids.iter() {
            if id != grant_id {
                new_ids.push_back(id);
            }
        }
        env.storage().instance().set(&StorageKey::GrantIds, &new_ids);

        let recipient_key = StorageKey::RecipientGrants(grant.recipient.clone());
        if let Some(user_grants) = env.storage().instance().get::<_, Vec<u64>>(&recipient_key) {
            let mut new_user_grants: Vec<u64> = Vec::new(&env);
            for id in user_grants.iter() {
                if id != grant_id {
                    new_user_grants.push_back(id);
                }
            }
            env.storage().instance().set(&recipient_key, &new_user_grants);
        }

        // Incentive: Bounty from native token (XLM)
        // 100,000 stroops (0.01 XLM) as a symbolic cleanup reward
        let bounty_amount: i128 = 100_000; 
        let native_token_addr: Address = env.storage().instance().get(&StorageKey::NativeToken).ok_or(Error::NotInitialized)?;
        let native_client = token::Client::new(&env, &native_token_addr);
        
        if native_client.balance(&env.current_contract_address()) >= bounty_amount {
            native_client.transfer(&env.current_contract_address(), &purger, &bounty_amount);
        }

        env.events().publish(
            (symbol_short!("purge"), grant_id, purger),
            bounty_amount,
        );

        Ok(())
    }

    // ── Issue #324: Public-Dashboard Heartbeat ────────────────────────────────

    /// Emit a treasury-health heartbeat event for community bots.
    /// Fires when 24 h have elapsed or TVL changed by ≥5%.
    /// Returns `true` when an event was emitted.
    pub fn heartbeat_emit(
        env: Env,
        total_tvl: i128,
        active_stream_count: u32,
        disputed_amount: i128,
        available_liquidity: i128,
    ) -> bool {
        public_dashboard::heartbeat_emit(
            &env,
            total_tvl,
            active_stream_count,
            disputed_amount,
            available_liquidity,
        )
    }

    // ── Issue #323: Tax-Reporting Export Hook ─────────────────────────────────

    /// Return the time-weighted average flow for `recipient` over a ledger range.
    /// Returns `(total_received, twa_per_second)`.
    pub fn get_historical_flow(
        env: Env,
        recipient: Address,
        start_ts: u64,
        end_ts: u64,
    ) -> (i128, i128) {
        tax_reporting::get_historical_flow(&env, &recipient, start_ts, end_ts)
    }

    // ── Issue #322: Audit-Log Merkle Root ─────────────────────────────────────

    /// Return the last stored Merkle root (32 bytes) for off-chain indexers.
    pub fn get_merkle_root(env: Env) -> soroban_sdk::Bytes {
        audit_log::get_merkle_root(&env)
    }

    /// Return the rolling transaction counter used for Merkle root emission.
    pub fn get_audit_tx_counter(env: Env) -> u32 {
        audit_log::get_tx_counter(&env)
    }

    // ── Issue #321: Multi-Threshold Signature Logic ───────────────────────────

    /// Register the signer set (must contain exactly 10 addresses).
    /// Admin only.
    pub fn initialize_rescue_signers(
        env: Env,
        signers: soroban_sdk::Vec<Address>,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;
        multi_threshold::initialize_signers(&env, signers)?;
        Ok(())
    }

    /// Open a new rescue proposal.  Caller must be a registered signer.
    /// Returns the new proposal id.
    pub fn propose_rescue(
        env: Env,
        proposer: Address,
        kind: multi_threshold::RescueKind,
        rescue_to: Address,
        amount: i128,
    ) -> Result<u64, Error> {
        multi_threshold::propose_rescue(&env, proposer, kind, rescue_to, amount)
    }

    /// Add an approval to a pending rescue proposal.
    pub fn approve_rescue(env: Env, signer: Address, proposal_id: u64) -> Result<(), Error> {
        multi_threshold::approve_rescue(&env, signer, proposal_id)
    }

    /// Execute a rescue proposal once the threshold is met.
    /// Performs the actual token transfer.
    pub fn execute_rescue(
        env: Env,
        caller: Address,
        proposal_id: u64,
        token_address: Address,
    ) -> Result<(), Error> {
        let (rescue_to, amount) =
            multi_threshold::execute_rescue(&env, caller, proposal_id)?;
        let client = token::Client::new(&env, &token_address);
        client.transfer(&env.current_contract_address(), &rescue_to, &amount);
        Ok(())
    }

    /// Cancel a pending rescue proposal.  Only the original proposer may cancel.
    pub fn cancel_rescue(env: Env, proposer: Address, proposal_id: u64) -> Result<(), Error> {
        multi_threshold::cancel_rescue(&env, proposer, proposal_id)
    }

    /// Return a rescue proposal by id.
    pub fn get_rescue_proposal(
        env: Env,
        proposal_id: u64,
    ) -> Option<multi_threshold::RescueProposal> {
        multi_threshold::get_proposal(&env, proposal_id)
    }

    /// Prunes heavy metadata for finalized and drained grants older than 180 days.
    /// Leaves a lightweight cryptographic tombstone (hash) for audit integrity.
    /// Incentivizes relayers with a gas bounty from reclaimed rent.
    pub fn prune_finalized_grant(env: Env, grant_id: u64, relayer: Address) -> Result<(), Error> {
        relayer.require_auth();

        let mut grant = read_grant(&env, grant_id)?;
        // Save timestamp before settle_grant modifies last_update_ts
        let last_update_ts = grant.last_update_ts;
        settle_grant(&mut grant, env.ledger().timestamp())?;

        // ── 1. Eligibility Validation ──────────────────────────────────────────
        
        // Grant must be Completed or Cancelled
        if grant.status != GrantStatus::Completed && grant.status != GrantStatus::Cancelled {
            return Err(Error::GrantNotPurgeable);
        }

        // Grant must be fully drained (no claimable funds)
        if grant.claimable > 0 || grant.validator_claimable > 0 {
            return Err(Error::GrantNotPurgeable);
        }

        // Must be older than 180 days since last update
        let now = env.ledger().timestamp();
        if now < last_update_ts.saturating_add(PRUNE_DELAY_SECONDS) {
            return Err(Error::GrantNotPurgeable);
        }

        // Ensure consistency (all funds accounted for)
        if grant.status == GrantStatus::Completed {
            let accounted = grant.withdrawn
                .checked_add(grant.claimable).ok_or(Error::MathOverflow)?
                .checked_add(grant.validator_withdrawn).ok_or(Error::MathOverflow)?
                .checked_add(grant.validator_claimable).ok_or(Error::MathOverflow)?;
            if accounted != grant.total_amount {
                return Err(Error::GrantNotPurgeable);
            }
        }

        // ── 2. Create Audit Tombstone ──────────────────────────────────────────
        
        // Hash the current grant state to preserve proof of existence
        let tombstone_hash = env.crypto().sha256(&grant.clone().to_xdr(&env));
        env.storage().persistent().set(&StorageKey::Tombstone(grant_id), &tombstone_hash);

        // ── 3. State Cleanup ───────────────────────────────────────────────────
        
        // Remove heavy milestones
        let nonce = env.storage().instance().get(&StorageKey::MilestoneSubmitNonce(grant_id)).unwrap_or(0_u64);
        for i in 0..nonce {
            env.storage().persistent().remove(&StorageKey::Milestone(grant_id, i as u32));
        }
        
        // Remove individual claim valuations
        let claim_count = env.storage().instance().get(&StorageKey::ClaimValueCounter(grant_id)).unwrap_or(0_u64);
        for i in 1..=claim_count {
            env.storage().persistent().remove(&StorageKey::ClaimValue(grant_id, i));
        }

        // Remove associated metadata
        env.storage().instance().remove(&StorageKey::MilestoneSubmitNonce(grant_id));
        env.storage().instance().remove(&StorageKey::ClaimValueCounter(grant_id));
        env.storage().instance().remove(&StorageKey::GrantStreamConfig(grant_id));
        env.storage().instance().remove(&StorageKey::GrantLegalData(grant_id));
        env.storage().instance().remove(&StorageKey::GrantValidatorData(grant_id));
        env.storage().instance().remove(&StorageKey::GrantMetrics(grant_id));
        env.storage().instance().remove(&StorageKey::GrantDisputeData(grant_id));
        
        // Remove main grant entry
        env.storage().instance().remove(&StorageKey::Grant(grant_id));

        // Update tracking lists
        let mut ids = read_grant_ids(&env);
        if let Some(pos) = ids.iter().position(|id| id == grant_id) {
            ids.remove(pos as u32);
            env.storage().instance().set(&StorageKey::GrantIds, &ids);
        }

        let recipient_key = StorageKey::RecipientGrants(grant.recipient.clone());
        if let Some(mut user_grants) = env.storage().instance().get::<_, Vec<u64>>(&recipient_key) {
            if let Some(pos) = user_grants.iter().position(|id| id == grant_id) {
                user_grants.remove(pos as u32);
                env.storage().instance().set(&recipient_key, &user_grants);
            }
        }

        // ── 4. Incentivize Relayer ─────────────────────────────────────────────
        
        // 200,000 stroops (0.02 XLM) as a gas bounty
        let bounty_amount: i128 = 200_000; 
        let native_token_addr: Address = env.storage().instance().get(&StorageKey::NativeToken).ok_or(Error::NotInitialized)?;
        let native_client = token::Client::new(&env, &native_token_addr);
        
        if native_client.balance(&env.current_contract_address()) >= bounty_amount {
            native_client.transfer(&env.current_contract_address(), &relayer, &bounty_amount);
        }

        // ── 5. Events ─────────────────────────────────────────────────────────
        
        env.events().publish(
            (symbol_short!("prune"), grant_id, relayer),
            (tombstone_hash.to_bytes(), bounty_amount),
        );

        Ok(())
    }

    /// Check if a user is an active grantee (has Active or Paused grants).
    /// This is a zero-gas, read-only function for partner protocols to verify
    /// grantee status for "Builder Discounts" or specialized access.
    /// Returns true if the user has at least one active, uncompleted grant.
    /// Returns false for stale/archived records or users with no active grants.
    /// Optimized for high-frequency cross-contract queries.
    pub fn is_active_grantee(env: Env, address: Address) -> bool {
        // Get all grant IDs for this recipient
        let recipient_key = StorageKey::RecipientGrants(address);
        if let Some(user_grants) = env.storage().instance().get::<_, Vec<u64>>(&recipient_key) {
            // Early exit if no grants found
            if user_grants.is_empty() {
                return false;
            }
            
            // Check each grant for active status
            for i in 0..user_grants.len() {
                let grant_id = user_grants.get(i).unwrap();
                if let Some(grant) = env.storage().instance().get::<_, Grant>(&StorageKey::Grant(grant_id)) {
                    // Only consider Active or Paused grants as "active grantees"
                    // Completed, Cancelled, and RageQuitted are not active
                    if grant.status == GrantStatus::Active || grant.status == GrantStatus::Paused {
                        // Additional check: ensure grant is not fully depleted
                        let total_withdrawn = grant.withdrawn
                            .checked_add(grant.validator_withdrawn).unwrap_or(0);
                        let total_claimable = grant.claimable
                            .checked_add(grant.validator_claimable).unwrap_or(0);
                        
                        // Grant is active if there are remaining funds to be streamed
                        if total_withdrawn < grant.total_amount || total_claimable > 0 {
                            return true;
                        }
                    }
                }
                // If grant doesn't exist (archived/purged), continue checking others
            }
        }
        false
    }

    // ── Security Council: Governance Attack Protection ────────────────────────

    /// Initialize the Security Council with 5 members (3-of-5 multi-sig).
    /// Admin only. This provides a final layer of defense against governance attacks.
    pub fn initialize_security_council(
        env: Env,
        members: Vec<Address>,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;
        security_council::SecurityCouncil::initialize_council(env, members)
            .map_err(|_| Error::NotAuthorized)
    }

    /// Create a pending governance action with 48-hour timelock.
    /// Used for sensitive operations like clawbacks, emergency pauses, etc.
    /// During the timelock window, the Security Council can veto the action.
    pub fn create_timelocked_action(
        env: Env,
        action_type: security_council::ActionType,
        target_grant_id: Option<u64>,
        initiator: Address,
        parameters: Vec<i128>,
    ) -> Result<u64, Error> {
        require_admin_auth(&env)?;
        security_council::SecurityCouncil::create_pending_action(
            env,
            action_type,
            target_grant_id,
            initiator,
            parameters,
        )
        .map_err(|_| Error::NotAuthorized)
    }

    /// Security Council member signs to veto a pending action.
    /// Requires 3 of 5 signatures to permanently block the action.
    /// This is the primary defense against rogue DAO attacks.
    pub fn council_sign_veto(
        env: Env,
        action_id: u64,
        signer: Address,
    ) -> Result<(), Error> {
        security_council::SecurityCouncil::sign_veto(env, action_id, signer)
            .map_err(|_| Error::NotAuthorized)
    }

    /// Execute a timelocked action after 48 hours if not vetoed.
    /// This completes the governance action if the Security Council
    /// did not intervene during the timelock period.
    pub fn execute_timelocked_action(
        env: Env,
        action_id: u64,
    ) -> Result<(), Error> {
        security_council::SecurityCouncil::execute_action(env, action_id)
            .map_err(|_| Error::NotAuthorized)
    }

    /// Check if a timelocked action can be executed.
    pub fn can_execute_timelocked_action(
        env: Env,
        action_id: u64,
    ) -> Result<bool, Error> {
        security_council::SecurityCouncil::can_execute_action(env, action_id)
            .map_err(|_| Error::NotAuthorized)
    }

    /// Propose new Security Council members (requires DAO approval).
    /// Council keys must be rotated annually via 7-day timelock.
    pub fn propose_council_rotation(
        env: Env,
        new_members: Vec<Address>,
        dao_admin: Address,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;
        security_council::SecurityCouncil::propose_council_rotation(
            env,
            new_members,
            dao_admin,
        )
        .map_err(|_| Error::NotAuthorized)
    }

    /// Execute council rotation after 7-day DAO-approved timelock.
    pub fn exec_council_rotation(env: Env) -> Result<(), Error> {
        require_admin_auth(&env)?;
        security_council::SecurityCouncil::execute_council_rotation(env)
            .map_err(|_| Error::NotAuthorized)
    }

    /// Check if annual council rotation is due.
    pub fn is_council_rotation_due(env: Env) -> bool {
        security_council::SecurityCouncil::is_rotation_due(env)
    }

    /// Get current Security Council members.
    pub fn get_council_members(env: Env) -> Result<Vec<Address>, Error> {
        security_council::SecurityCouncil::get_council_members(env)
            .map_err(|_| Error::NotAuthorized)
    }

    /// Get details of a pending timelocked action.
    pub fn get_pending_action(
        env: Env,
        action_id: u64,
    ) -> Result<security_council::PendingAction, Error> {
        security_council::SecurityCouncil::get_pending_action(env, action_id)
            .map_err(|_| Error::NotAuthorized)
    }

    /// Get the number of veto signatures for an action.
    pub fn get_veto_signature_count(env: Env, action_id: u64) -> u32 {
        security_council::SecurityCouncil::get_veto_count(env, action_id)
    }

    /// Get all pending action IDs.
    pub fn get_all_pending_actions(env: Env) -> Vec<u64> {
        security_council::SecurityCouncil::get_pending_action_ids(env)
    }

    /// Protected clawback with Security Council oversight.
    /// Creates a timelocked action that can be vetoed by the council.
    /// This prevents rogue DAO attacks on grant funds.
    pub fn protected_clawback(
        env: Env,
        grant_id: u64,
        initiator: Address,
    ) -> Result<u64, Error> {
        require_admin_auth(&env)?;
        
        // Create timelocked action for clawback
        let mut params = Vec::new(&env);
        params.push_back(grant_id as i128);
        
        let action_id = security_council::SecurityCouncil::create_pending_action(
            env.clone(),
            security_council::ActionType::Clawback,
            Some(grant_id),
            initiator.clone(),
            params,
        )
        .map_err(|_| Error::NotAuthorized)?;

        env.events().publish(
            (symbol_short!("claw_pend"), initiator, grant_id),
            action_id,
        );

        Ok(action_id)
    }

    /// Execute a vetted clawback after timelock expires.
    /// Can only be called if Security Council did not veto.
    pub fn execute_protected_clawback(
        env: Env,
        action_id: u64,
        grant_id: u64,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;

        // Verify action can be executed
        let can_execute = security_council::SecurityCouncil::can_execute_action(
            env.clone(),
            action_id,
        )
        .map_err(|_| Error::NotAuthorized)?;

        if !can_execute {
            return Err(Error::InvalidState);
        }

        // Mark action as executed
        security_council::SecurityCouncil::execute_action(env.clone(), action_id)
            .map_err(|_| Error::NotAuthorized)?;

        // Perform the actual clawback
        Self::cancel_grant(env, grant_id)
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
#[cfg(all(test, feature = "legacy-tests"))]
mod test_concurrent_withdraw;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_rounding_fuzz;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_temporal_fuzz;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_global_invariant_fuzz;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_security_invariants;
#[cfg(all(test, feature = "legacy-tests"))]
mod is_active_grantee_benchmark;
#[cfg(test)]
mod test_sep38_claim_value;
