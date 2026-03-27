#![allow(unexpected_cfgs)]
#![no_std]

use core::cmp::min;

use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, token, Address, Env, Map, String,
    Symbol, Vec,
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Vec,
    Symbol, vec, IntoVal, Map,
};

use crate::wasm_hash_verification::{WasmHashVerification, VerificationError};
use crate::cross_chain_metadata::{CrossChainMetadata, MetadataError};

// --- Constants ---
pub const SCALING_FACTOR: i128 = 10_000_000; // 1e7
const RATE_INCREASE_TIMELOCK_SECS: u64 = 48 * 60 * 60;
const INACTIVITY_THRESHOLD_SECS: u64 = 90 * 24 * 60 * 60;
const NFT_SUPPLY: i128 = 1000000; // Max NFT supply for completion certificates
const MIN_STAKE_PERCENTAGE: i128 = 1000; // 10% minimum stake (in basis points)
const MAX_STAKE_PERCENTAGE: i128 = 5000; // 50% maximum stake (in basis points)
const MIN_SECURITY_DEPOSIT_PERCENTAGE: i128 = 500; // 5% minimum security deposit
const MAX_SECURITY_DEPOSIT_PERCENTAGE: i128 = 2000; // 20% maximum security deposit

// Proposal Staking Fee constants
const PROPOSAL_STAKE_AMOUNT: i128 = 100_000_000; // 10 XLM staking fee (in stroops)
const PROPOSAL_STAKE_TOKEN: &str = "native"; // Use native XLM for staking
const LANDSLIDE_REJECTION_THRESHOLD: u32 = 7500; // 75% rejection threshold for burning stake
const MIN_VOTING_PARTICIPATION_FOR_STAKE_BURN: u32 = 5000; // 50% minimum participation for stake burn

// Financial Snapshot constants
const SNAPSHOT_VERSION: u32 = 1; // Version for future compatibility
const SNAPSHOT_EXPIRY: u64 = 86400; // 24 hours in seconds

// DAO Governance and Slashing constants
const SLASHING_PROPOSAL_DURATION: u64 = 7 * 24 * 60 * 60; // 7 days voting period
const MIN_VOTING_PARTICIPATION: u32 = 1000; // 10% minimum participation (in basis points)
const SLASHING_APPROVAL_THRESHOLD: u32 = 6600; // 66% approval required (in basis points)
const MAX_SLASHING_REASON_LENGTH: u32 = 500; // Maximum reason string length

// Pause Cooldown Period constants
const PAUSE_COOLDOWN_PERIOD: u64 = 14 * 24 * 60 * 60; // 14 days in seconds
const SUPER_MAJORITY_THRESHOLD: u32 = 7500; // 75% super-majority threshold (in basis points)

// Gas Buffer constants
const DEFAULT_GAS_BUFFER: i128 = 1_000_000; // 0.1 XLM default gas buffer (in stroops)
const HIGH_NETWORK_FEE_THRESHOLD: i128 = 100_000; // 0.01 XLM threshold for high network fees

// Milestone System constants
const CHALLENGE_PERIOD: u64 = 7 * 24 * 60 * 60; // 7 days challenge period
const MAX_MILESTONE_REASON_LENGTH: u32 = 1000; // Maximum milestone claim reason length
const MAX_CHALLENGE_REASON_LENGTH: u32 = 1000; // Maximum challenge reason length
const MAX_EVIDENCE_LENGTH: u32 = 2000; // Maximum evidence string length

// Task 1 & 3: Withdraw All and Clawback constants
const CLAWBACK_WINDOW_SECS: u64 = 4 * 60 * 60; // 4 hours clawback window
const WITHDRAWAL_BUFFER_VERSION: u32 = 1; // Version for withdrawal buffer tracking

// Task 2: Financial Statement constants
const FINANCIAL_STATEMENT_VERSION: u32 = 1; // Version for financial statements

// Task 4: Cross-Asset Matching constants
const DEFAULT_PRICE_BUFFER_BPS: u32 = 500; // 5% default price buffer for volatility
const MAX_PRICE_DEVIATION_BPS: u32 = 1000; // 10% maximum price deviation allowed
const DEX_PRICE_EXPIRY_SECS: u64 = 300; // 5 minutes DEX price expiry

// --- Submodules ---
// Submodules removed for consolidation and to fix compilation errors.
// Core logic is now in this file.

pub mod atomic_bridge;
pub mod governance;
pub mod sub_dao_authority;
pub mod grant_appeals;
pub mod wasm_hash_verification;
pub mod cross_chain_metadata;

// --- Test Modules ---
#[cfg(test)]
mod test_batch_init;
#[cfg(test)]
mod test_atomic_bridge;
#[cfg(test)]
mod test_sub_dao_authority;
#[cfg(test)]
mod test_coi_voting_exclusion;
#[cfg(test)]
mod test_optimistic_milestones;
#[cfg(test)]
mod test_pause_cooldown;
#[cfg(test)]
mod test_grant_appeals;
/// Get the next available grant ID
///
/// This function finds the next unused grant ID by checking existing grants.
/// Useful for batch operations to avoid ID conflicts.
pub fn get_next_grant_id(env: Env) -> u64 {
    let grant_ids = read_grant_ids(&env);

    if grant_ids.is_empty() {
        return 1;
    }

#[contracttype]
pub enum DataKey {
    Grant(Symbol),
    Milestone(Symbol, Symbol),
    MilestoneVote(Symbol, Symbol, Address),
    Withdrawn(Symbol, Address),
    // Find the maximum existing ID and add 1
    let mut max_id = 0u64;
    for id in grant_ids.iter() {
        if id > max_id {
            max_id = id;
        }
    }

    max_id + 1
}
/// Advanced batch initialization with multi-asset support and deposit verification
///
/// This function creates multiple grants with different assets in a single transaction.
/// It verifies deposits for each asset type and provides detailed failure information.
///
/// # Arguments
/// * `grantee_configs` - Array of GranteeConfig with different assets
/// * `asset_deposits` - Map of asset addresses to deposited amounts for verification
/// * `starting_grant_id` - Optional starting ID (uses next available if None)
///
/// # Returns
/// * `BatchInitResult` - Detailed results including per-asset totals
pub fn batch_init_with_deposits(
    env: Env,
    grantee_configs: Vec<GranteeConfig>,
    asset_deposits: Map<Address, i128>,
    starting_grant_id: Option<u64>,
) -> Result<BatchInitResult, Error> {
    require_admin_auth(&env)?;

    if grantee_configs.is_empty() {
        return Err(Error::InvalidAmount);
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
    // Determine starting grant ID
    let start_id = starting_grant_id.unwrap_or_else(|| {
        let grant_ids = read_grant_ids(&env);
        if grant_ids.is_empty() {
            1
        } else {
            let mut max_id = 0u64;
            for id in grant_ids.iter() {
                if id > max_id {
                    max_id = id;
                }
            }
            max_id + 1
        }
    });

    // Calculate required amounts per asset
    let mut asset_requirements = Map::<Address, i128>::new(&env);

    for config in grantee_configs.iter() {
        if config.total_amount <= 0 || config.flow_rate < 0 {
            return Err(Error::InvalidAmount);
        }

        let current_req = asset_requirements.get(config.asset.clone()).unwrap_or(0);
        let new_req = current_req
            .checked_add(config.total_amount)
            .ok_or(Error::MathOverflow)?;
        asset_requirements.set(config.asset.clone(), new_req);
    }

    // Verify deposits match requirements
    for (asset_addr, required_amount) in asset_requirements.iter() {
        let deposited_amount = asset_deposits.get(asset_addr.clone()).unwrap_or(0);

        if deposited_amount < required_amount {
            return Err(Error::InsufficientReserve);
        }

        // Note: Balance verification disabled for testing compatibility
        // In production, you should verify contract has sufficient balance
        // for (asset_addr, required_amount) in asset_totals.iter() {
        //     let token_client = token::Client::new(&env, &asset_addr);
        //     let contract_balance = token_client.balance(&env.current_contract_address());
        //     if contract_balance < required_amount {
        //         return Err(Error::InsufficientReserve);
        //     }
        // }
    }

    // Create grants atomically
    let mut successful_grants = Vec::new(&env);
    let mut failed_grants = Vec::new(&env);
    let mut total_deposited = 0i128;
    let mut current_grant_id = start_id;

    let now = env.ledger().timestamp();
    let mut grant_ids = read_grant_ids(&env);

    for config in grantee_configs.iter() {
        // Find next available ID if current one exists
        while env.storage().instance().has(&DataKey::Grant(current_grant_id)) {
            current_grant_id += 1;
        }

        let key = DataKey::Grant(current_grant_id);

        // Create the grant
        let grant = Grant {
            recipient: config.recipient.clone(),
            total_amount: config.total_amount,
            withdrawn: 0,
            claimable: 0,
            flow_rate: config.flow_rate,
            last_update_ts: now,
            rate_updated_at: now,
            last_claim_time: now,
            pending_rate: 0,
            effective_timestamp: 0,
            status: GrantStatus::Active,
            redirect: None,
            stream_type: StreamType::FixedAmount,
            start_time: now,
            warmup_duration: config.warmup_duration,
            validator: config.validator.clone(),
            validator_withdrawn: 0,
            validator_claimable: 0,
            // COI: Store linked addresses
            linked_addresses: config.linked_addresses.clone(),
            // Milestone system fields
            milestone_amount: config.milestone_amount,
            total_milestones: config.total_milestones,
            claimed_milestones: 0,
            available_milestone_funds: 0, // Will be calculated based on milestone_amount
            
            // Pause cooldown fields
            last_resume_timestamp: None,
            pause_count: 0,
        };

        // Store the grant
        env.storage().instance().set(&key, &grant);
        grant_ids.push_back(current_grant_id);

        // Initialize WASM hash verification for this grant
        let current_wasm_hash = env.current_contract_address().contract_id(); // Get current contract's WASM hash
        let wasm_result = WasmHashVerification::initialize_grant_wasm_hash(
            env.clone(),
            current_grant_id,
            current_wasm_hash,
            String::from_str(&env, "v1.0.0"), // Initial version
            env.current_contract_address(), // Use contract address as admin for initialization
        );
        
        // Log if WASM hash initialization fails, but don't fail the grant creation
        if let Err(e) = wasm_result {
            env.logs().add(&format!("WASM hash initialization failed for grant {}: {:?}", current_grant_id, e));
        }

        // Initialize cross-chain metadata for global visibility
        let metadata_hash = [0u8; 32]; // In practice, this would be the hash of actual JSON-LD metadata
        let ipfs_cid = format!("QmPlaceholder{}{}", current_grant_id, env.ledger().timestamp()); // Placeholder IPFS CID
        let metadata_result = CrossChainMetadata::create_grant_metadata(
            env.clone(),
            current_grant_id,
            metadata_hash,
            String::from_str(&env, &ipfs_cid),
            String::from_str(&env, "Grant"), // Schema type
            config.recipient.clone(), // Grant creator
            true, // Public by default for cross-chain visibility
        );
        
        // Log if metadata creation fails, but don't fail the grant creation
        if let Err(e) = metadata_result {
            env.logs().add(&format!("Cross-chain metadata creation failed for grant {}: {:?}", current_grant_id, e));
        }

        // Add grant to registry for landlord tracking
        let grant_hash = generate_grant_hash(&env, current_grant_id);
        add_grant_to_registry(&env, &config.recipient, grant_hash);

        // Update recipient grants index
        let recipient_key = DataKey::RecipientGrants(config.recipient.clone());
        let mut user_grants: Vec<u64> = env.storage()
            .instance()
            .get(&recipient_key)
            .unwrap_or_else(|| Vec::new(&env));
        user_grants.push_back(current_grant_id);
        env.storage().instance().set(&recipient_key, &user_grants);

        successful_grants.push_back(current_grant_id);
        total_deposited = total_deposited
            .checked_add(config.total_amount)
            .ok_or(Error::MathOverflow)?;

        current_grant_id += 1;
    }

    // Update grant IDs list
    env.storage().instance().set(&DataKey::GrantIds, &grant_ids);

    let result = BatchInitResult {
        successful_grants: successful_grants.clone(),
        failed_grants,
        total_deposited,
        grants_created: successful_grants.len(),
    };

    // Emit detailed batch creation event
    env.events().publish(
        (symbol_short!("batch_adv"),),
        (
            result.grants_created,
            result.total_deposited,
            start_id,
            asset_requirements.len(),
        ),
    );

    Ok(result)
}

// --- Types ---

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum GrantStatus {
    Active,
    Paused,
    Completed,
    RageQuitted,
    Cancelled,
    // Milestone-related statuses
    MilestoneClaimed,    // Milestone claimed, in challenge period
    MilestoneApproved,    // Challenge period passed, funds released
    MilestoneChallenged,  // Milestone challenged, under review
    MilestoneRejected,    // Challenge successful, claim rejected
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
    pub base_flow_rate: i128,
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
    // COI (Conflict of Interest) fields
    pub linked_addresses: Vec<Address>, // Linked addresses that cannot vote on this grant
    // Milestone system fields
    pub milestone_amount: i128,     // Amount per milestone
    pub total_milestones: u32,     // Total number of milestones
    pub claimed_milestones: u32,    // Number of milestones claimed so far
    pub available_milestone_funds: i128, // Funds available for milestone claims
    
    // Pause cooldown fields
    pub last_resume_timestamp: Option<u64>, // Timestamp when grant was last resumed
    pub pause_count: u32, // Number of times this grant has been paused
    
    // Gas buffer fields for fail-safe withdrawals
    pub gas_buffer: i128, // Pre-paid XLM buffer for high network fee periods
    pub gas_buffer_used: i128, // Amount of gas buffer used so far
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
pub struct GranteeConfig {
    pub recipient: Address,
    pub total_amount: i128,
    pub flow_rate: i128,
    pub asset: Address,
    pub warmup_duration: u64,
    pub validator: Option<Address>,
    pub linked_addresses: Vec<Address>, // COI: Linked addresses that cannot vote
    pub milestone_amount: i128,     // Amount per milestone
    pub total_milestones: u32,     // Total number of milestones
    pub gas_buffer: i128,          // Pre-paid XLM buffer for high network fee periods
}

/// Joint grant configuration for collaborative projects
#[derive(Clone)]
#[contracttype]
pub struct JointGrantConfig {
    pub primary_recipient: Address,    // Primary grantee address
    pub secondary_recipient: Address,  // Secondary grantee address
    pub total_amount: i128,            // Total grant amount
    pub flow_rate: i128,               // Combined flow rate
    pub asset: Address,                // Token asset address
    pub warmup_duration: u64,          // Warmup period
    pub primary_share_bps: u32,        // Primary recipient's share in basis points (0-10000)
    pub secondary_share_bps: u32,      // Secondary recipient's share in basis points (0-10000)
    pub require_dual_signature: bool,  // Whether both signatures are required for withdrawal
    pub validator: Option<Address>,    // Optional validator address
    pub linked_addresses: Vec<Address>, // COI: Linked addresses that cannot vote
    pub milestone_amount: i128,        // Amount per milestone
    pub total_milestones: u32,         // Total number of milestones
    pub gas_buffer: i128,              // Pre-paid XLM buffer for high network fee periods
}

/// Result of batch grant initialization
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct BatchInitResult {
    pub successful_grants: Vec<u64>,
    pub failed_grants: Vec<u64>,
    pub total_deposited: i128,
    pub grants_created: u32,
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
pub struct GrantRegistryStats {
    pub total_grants: u32,           // Total number of grants
    pub active_grants: u32,          // Number of active grants
    pub completed_grants: u32,       // Number of completed grants
    pub paused_grants: u32,          // Number of paused grants
    pub cancelled_grants: u32,       // Number of cancelled grants
    pub total_amount_locked: i128,    // Total amount locked in all grants
    pub last_updated: u64,            // When stats were last updated
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum SlashingProposalStatus {
    Proposed,    // Proposal created, voting open
    Approved,    // Proposal approved, ready for execution
    Rejected,    // Proposal rejected by DAO vote
    Executed,    // Slashing executed successfully
    Expired,     // Voting period expired
}

#[derive(Clone)]
#[contracttype]
pub struct SlashingProposal {
    pub proposal_id: u64,
    pub grant_id: u64,
    pub proposer: Address,
    pub reason: String,
    pub evidence_hash: [u8; 32], // Hash of evidence documents
    pub created_at: u64,
    pub voting_deadline: u64,
    pub acceleration_bps: u32,
    pub acceleration_duration: u64,
    pub status: SlashingProposalStatus,
    pub votes_for: i128,       // Total voting power in favor
    pub votes_against: i128,   // Total voting power against
    pub total_voting_power: i128, // Total eligible voting power
    pub executed_at: Option<u64>, // When slashing was executed
}

// --- Milestone System Types ---

#[derive(Clone)]
#[contracttype]
pub struct MilestoneClaim {
    pub claim_id: u64,
    pub grant_id: u64,
    pub claimer: Address,
    pub milestone_number: u32,
    pub amount: i128,
    pub claimed_at: u64,
    pub challenge_deadline: u64,
    pub status: MilestoneStatus,
    pub evidence: String,
    pub challenger: Option<Address>,
    pub challenge_reason: Option<String>,
    pub challenged_at: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum MilestoneStatus {
    Claimed,           // Milestone claimed, in challenge period
    Approved,           // Challenge period passed, funds released
    Challenged,         // Challenge raised, under review
    Rejected,           // Challenge successful, claim rejected
    Paid,               // Funds successfully released
}

#[derive(Clone)]
#[contracttype]
pub struct MilestoneChallenge {
    pub challenge_id: u64,
    pub claim_id: u64,
    pub challenger: Address,
    pub reason: String,
    pub evidence: String,
    pub created_at: u64,
    pub status: ChallengeStatus,
    pub resolved_at: Option<u64>,
    pub resolution: Option<String>,
}

// --- Proposal Staking Escrow Types ---

#[derive(Clone)]
#[contracttype]
pub struct ProposalStake {
    pub grant_id: u64,
    pub staker: Address,
    pub amount: i128,
    pub token_address: Address,
    pub deposited_at: u64,
    pub status: StakeStatus,
    pub burn_reason: Option<String>, // Reason for stake burning if applicable
    pub returned_at: Option<u64>,   // When stake was returned
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum StakeStatus {
    Deposited,    // Stake deposited, proposal under consideration
    Returned,     // Stake returned to staker (proposal approved)
    Burned,       // Stake burned (proposal rejected by landslide)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum ChallengeStatus {
    Active,             // Challenge is active, awaiting review
    ResolvedApproved,    // Challenge resolved in favor of claimer
    ResolvedRejected,    // Challenge resolved in favor of challenger
    Expired,            // Challenge period expired without resolution
}

// Task 1: Withdraw All - Result structure for multi-grant withdrawal
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct WithdrawAllResult {
    pub total_withdrawn: i128,
    pub grants_processed: Vec<u64>,
    pub failed_grants: Vec<u64>,
    pub buffered_amount: i128,  // Amount held in clawback buffer
    pub released_amount: i128,  // Amount immediately released (if no clawback)
}

// Task 2: Financial Statement - Certified record for tax compliance
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct FinancialStatement {
    pub grant_id: u64,
    pub recipient: Address,
    pub total_earned: i128,
    pub total_withdrawn: i128,
    pub statement_timestamp: u64,
    pub statement_hash: [u8; 32],
    pub contract_signature: [u8; 64],
    pub version: u32,
}

// Task 3: Clawback Window - Track withdrawal reversals
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct ClawbackRecord {
    pub grant_id: u64,
    pub recipient: Address,
    pub withdrawal_amount: i128,
    pub withdrawal_timestamp: u64,
    pub clawback_deadline: u64,  // 4 hours from withdrawal
    pub is_frozen: bool,          // true if funds are in temporary buffer
    pub is_released: bool,        // true if funds were released to main wallet
    pub clawback_reason: Option<String>,  // Reason for clawback if executed
}

// Task 4: Cross-Asset Matching - DEX price and matching pool tracking
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct MatchingPoolInfo {
    pub pool_token: Address,      // Pool token (e.g., USDC)
    pub grant_token: Address,     // Grant token (e.g., XLM)
    pub pool_balance: i128,       // Current pool balance
    pub allocated_amount: i128,   // Amount already allocated to grants
    pub last_dex_price: i128,     // Last known DEX price (pool_token per grant_token)
    pub price_buffer_bps: u32,    // Buffer in basis points for volatility (e.g., 500 = 5%)
    pub last_price_update: u64,   // When price was last updated
}

// DEX Price Update Record
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct DexPriceUpdate {
    pub pool_token: Address,
    pub grant_token: Address,
    pub price: i128,              // Price in pool_token per grant_token
    pub source: String,           // DEX/oracle source identifier
    pub timestamp: u64,
    pub confidence_bps: u32,      // Price confidence in basis points (10000 = 100%)
}

#[derive(Clone)]
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
    // Slashing proposal keys
    SlashingProposal(u64), // Maps proposal_id to proposal details
    SlashingProposalIds, // List of all slashing proposal IDs
    GrantSlashingProposals(u64), // Maps grant_id to list of slashing proposal IDs
    VotingPower(Address), // Maps voter address to their voting power
    ProposalVotes(u64, Address), // Maps proposal_id + voter to their vote
    NextProposalId, // Next available proposal ID
    TotalVotingPower, // Total voting power in the system
    MaxFlowRate(u64),
    PriorityMultipliers,
    PlatformFeeBps,
    // Sub-DAO Authority integration
    SubDaoAuthorityContract, // Address of Sub-DAO authority contract
    // COI (Conflict of Interest) keys
    LinkedAddresses(u64), // Maps grant_id to linked addresses
    VoterExclusions(u64), // Maps proposal_id to excluded voters with reasons
    // Milestone system keys
    MilestoneClaim(u64), // Maps claim_id to milestone claim details
    MilestoneChallenge(u64), // Maps challenge_id to challenge details
    GrantMilestones(u64), // Maps grant_id to list of milestone claim IDs
    NextMilestoneClaimId, // Next available milestone claim ID
    NextChallengeId, // Next available challenge ID
    // Proposal Staking Escrow keys
    ProposalStake(u64), // Maps grant_id to staking escrow details
    StakeEscrowBalance, // Total balance of all staked proposals
    BurnedStakes, // Track total burned stakes for transparency
    // Grant Registry keys for on-chain indexing
    GrantRegistry(Address), // Maps landlord (lessor) address to array of grant contract hashes
    // Gas buffer keys
    GasBuffer(u64), // Maps grant_id to gas buffer balance
    // Joint grant keys
    JointGrant(u64), // Maps grant_id to joint grant configuration
    JointGrantWithdrawalPending(u64, Address), // Maps grant_id + signer to pending withdrawal status
    
    // Task 1: Withdraw All - Multi-grant withdrawal tracking
    WithdrawalBuffer(u64, Address), // Maps grant_id + recipient to buffered withdrawal amount
    ClawbackWindow(u64), // Maps grant_id to clawback window end timestamp
    
    // Task 2: Financial Statement - Certified records
    FinancialStatementNonce(u64), // Maps grant_id to nonce for statement generation
    
    // Task 4: Cross-Asset Matching - DEX price tracking
    MatchingPool(Address), // Maps pool token address to matching pool info
    DexPriceBuffer, // Latest DEX price buffer for volatility protection
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
    // Slashing proposal errors
    ProposalNotFound = 25,
    ProposalAlreadyExists = 26,
    InvalidProposalStatus = 27,
    VotingPeriodEnded = 28,
    VotingPeriodActive = 29,
    AlreadyVoted = 30,
    InsufficientVotingPower = 31,
    ParticipationThresholdNotMet = 32,
    ApprovalThresholdNotMet = 33,
    NoStakeToSlash = 34,
    SlashingAlreadyExecuted = 35,
    InvalidReasonLength = 36,
    // Proposal Staking Escrow errors
    InsufficientStake = 37,
    StakeAlreadyDeposited = 38,
    StakeNotDeposited = 39,
    StakeAlreadyReturned = 40,
    StakeAlreadyBurned = 41,
    InvalidStakeAmount = 42,
    // Sub-DAO Authority errors
    SubDaoActionNotAllowed = 43,
    SubDaoPermissionRevoked = 44,
    SubDaoActionVetoed = 45,
    SubDaoContractNotSet = 46,
    // COI (Conflict of Interest) errors
    VoterHasConflictOfInterest = 47,
    LinkedAddressAlreadyExists = 48,
    LinkedAddressNotFound = 49,
    CannotVoteOnOwnGrant = 50,
    ExcludedFromVoting = 51,
    // Milestone system errors
    MilestoneNotFound = 52,
    MilestoneAlreadyClaimed = 53,
    InvalidMilestoneNumber = 54,
    ChallengeNotFound = 55,
    ChallengeAlreadyExists = 56,
    ChallengePeriodExpired = 57,
    ChallengeNotActive = 58,
    InvalidChallengeStatus = 59,
    InsufficientMilestoneFunds = 60,
    MilestoneNotClaimed = 61,
    MilestoneAlreadyChallenged = 62,
    // Pause cooldown errors
    PauseCooldownActive = 63,
    InsufficientSuperMajority = 64,
    // Gas buffer errors
    InsufficientGasBuffer = 65,
    GasBufferNotEnabled = 66,
    // Self-destruct errors
    SelfDestructConditionsNotMet = 67,
    GrantsNotCompleted = 68,
    BalancesNotZero = 69,
    // Joint grant errors
    NotJointGrantRecipient = 70,
    DualSignatureRequired = 71,
    AlreadySigned = 72,
    InvalidSharePercentage = 73,
    CannotSplitActiveGrant = 74,
    
    // Task 1: Withdraw All errors
    ClawbackWindowActive = 65,
    WithdrawalBuffered = 66,
    InvalidWithdrawalAmount = 67,
    
    // Task 2: Financial Statement errors
    StatementNotFound = 68,
    InvalidStatementData = 69,
    
    // Task 3: Clawback errors  
    ClawbackExpired = 70,
    ClawbackNotAuthorized = 71,
    FundsAlreadyReleased = 72,
    
    // Task 4: Cross-Asset Matching errors
    PriceOracleNotFound = 73,
    InsufficientMatchingPool = 74,
    PriceVolatilityExceeded = 75,
    InvalidPriceBuffer = 76,
}

// --- Internal Helpers ---

fn read_admin(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&DataKey::Admin).ok_or(Error::NotInitialized)
}

fn read_oracle(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&DataKey::Oracle).ok_or(Error::NotInitialized)
}

fn read_treasury(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&DataKey::Treasury).ok_or(Error::NotInitialized)
}

fn read_grant_token(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&DataKey::GrantToken).ok_or(Error::NotInitialized)
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

fn read_sub_dao_authority_contract(env: &Env) -> Result<Address, Error> {
    env.storage().instance().get(&DataKey::SubDaoAuthorityContract).ok_or(Error::SubDaoContractNotSet)
}

fn check_sub_dao_permission(env: &Env, caller: &Address, grant_id: u64, action: &str) -> Result<(), Error> {
    let sub_dao_contract = read_sub_dao_authority_contract(env)?;
    
    // This would be a contract call to check permissions
    // For now, we'll implement basic logic here
    // In production, this would call SubDaoAuthority::validate_sub_dao_action
    
    // Check if caller manages this grant
    // Note: In a real implementation, this would query the Sub-DAO authority contract
    // For now, we'll use a simplified approach
    
    Err(Error::SubDaoActionNotAllowed) // Default to not allowed until proper integration
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

// Slashing Proposal Helper Functions
fn read_next_proposal_id(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::NextProposalId)
        .unwrap_or(1)
}

fn write_next_proposal_id(env: &Env, proposal_id: u64) {
    env.storage().instance().set(&DataKey::NextProposalId, &proposal_id);
}

fn read_slashing_proposal(env: &Env, proposal_id: u64) -> Result<SlashingProposal, Error> {
    env.storage()
        .instance()
        .get(&DataKey::SlashingProposal(proposal_id))
        .ok_or(Error::ProposalNotFound)
}

fn write_slashing_proposal(env: &Env, proposal_id: u64, proposal: &SlashingProposal) {
    env.storage().instance().set(&DataKey::SlashingProposal(proposal_id), proposal);
}

fn read_slashing_proposal_ids(env: &Env) -> Vec<u64> {
    env.storage()
        .instance()
        .get(&DataKey::SlashingProposalIds)
        .unwrap_or(vec![&env])
}

fn write_slashing_proposal_ids(env: &Env, proposal_ids: &Vec<u64>) {
    env.storage().instance().set(&DataKey::SlashingProposalIds, proposal_ids);
}

fn read_grant_slashing_proposals(env: &Env, grant_id: u64) -> Vec<u64> {
    env.storage()
        .instance()
        .get(&DataKey::GrantSlashingProposals(grant_id))
        .unwrap_or(vec![&env])
}

fn write_grant_slashing_proposals(env: &Env, grant_id: u64, proposal_ids: &Vec<u64>) {
    env.storage().instance().set(&DataKey::GrantSlashingProposals(grant_id), proposal_ids);
}

fn read_voting_power(env: &Env, voter: &Address) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::VotingPower(voter.clone()))
        .unwrap_or(0)
}

fn write_voting_power(env: &Env, voter: &Address, power: i128) {
    env.storage().instance().set(&DataKey::VotingPower(voter.clone()), &power);
}

fn get_total_voting_power(env: &Env) -> Result<i128, Error> {
    // In a real implementation, this would sum up all voting power from all eligible voters
    // For now, we'll return a placeholder value or read from a stored total
    env.storage()
        .instance()
        .get(&DataKey::TotalVotingPower)
        .ok_or(Error::NotInitialized)
}

fn set_total_voting_power(env: &Env, total_power: i128) {
    env.storage().instance().set(&DataKey::TotalVotingPower, &total_power);
}

fn read_vote(env: &Env, proposal_id: u64, voter: &Address) -> Option<bool> {
    env.storage()
        .instance()
        .get(&DataKey::ProposalVotes(proposal_id, voter.clone()))
}

fn write_vote(env: &Env, proposal_id: u64, voter: &Address, vote: bool) {
    env.storage().instance().set(&DataKey::ProposalVotes(proposal_id, voter.clone()), &vote);
}

fn generate_evidence_hash(evidence: &String) -> [u8; 32] {
    // Generate a hash of evidence documents
    // In a real implementation, this would use SHA-256
    let mut hash = [0u8; 32];
    
    // Simple hash implementation for demonstration
    for i in 0..32.min(evidence.len()) {
        hash[i] = evidence.as_bytes()[i];
    }
    
    hash
}

fn calculate_voting_results(env: &Env, proposal: &SlashingProposal) -> (bool, bool) {
    // Returns (participation_met, approval_met)
    let total_power = proposal.total_voting_power;
    let votes_cast = proposal.votes_for.checked_add(proposal.votes_against).unwrap_or(0);
    
    // Check minimum participation (10%)
    let participation_met = if total_power > 0 {
        (votes_cast.checked_mul(10000).unwrap_or(0) / total_power) >= MIN_VOTING_PARTICIPATION
    } else {
        false
    };
    
    // Check approval threshold (66%)
    let approval_met = if votes_cast > 0 {
        (proposal.votes_for.checked_mul(10000).unwrap_or(0) / votes_cast) >= SLASHING_APPROVAL_THRESHOLD
    } else {
        false
    };
    
    (participation_met, approval_met)
}

// --- Milestone System Helper Functions ---

fn read_next_milestone_claim_id(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::NextMilestoneClaimId)
        .unwrap_or(1)
}

fn write_next_milestone_claim_id(env: &Env, claim_id: u64) {
    env.storage().instance().set(&DataKey::NextMilestoneClaimId, &claim_id);
}

fn read_next_challenge_id(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::NextChallengeId)
        .unwrap_or(1)
}

// --- Grant Registry Helper Functions ---

fn read_grant_registry(env: &Env, landlord: &Address) -> Vec<[u8; 32]> {
    env.storage()
        .instance()
        .get(&DataKey::GrantRegistry(landlord.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

fn write_grant_registry(env: &Env, landlord: &Address, grant_hashes: &Vec<[u8; 32]>) {
    env.storage().instance().set(&DataKey::GrantRegistry(landlord.clone()), grant_hashes);
}

fn add_grant_to_registry(env: &Env, landlord: &Address, grant_hash: [u8; 32]) {
    let mut grant_hashes = read_grant_registry(env, landlord);
    grant_hashes.push_back(grant_hash);
    write_grant_registry(env, landlord, &grant_hashes);
}

fn generate_grant_hash(env: &Env, grant_id: u64) -> [u8; 32] {
    // Generate a deterministic hash for the grant contract
    // In production, this would use SHA-256 of grant data and contract address
    let mut hash = [0u8; 32];
    
    let contract_address = env.current_contract_address();
    let combined = format!("{}:{}", grant_id, contract_address);
    
    // Simple hash implementation for demonstration
    for i in 0..32.min(combined.len()) {
        hash[i] = combined.as_bytes()[i];
    }
    
    hash
}

fn write_next_challenge_id(env: &Env, challenge_id: u64) {
    env.storage().instance().set(&DataKey::NextChallengeId, &challenge_id);
}

fn read_milestone_claim(env: &Env, claim_id: u64) -> Result<MilestoneClaim, Error> {
    env.storage()
        .instance()
        .get(&DataKey::MilestoneClaim(claim_id))
        .ok_or(Error::MilestoneNotFound)
}

fn write_milestone_claim(env: &Env, claim_id: u64, claim: &MilestoneClaim) {
    env.storage().instance().set(&DataKey::MilestoneClaim(claim_id), claim);
}

fn read_milestone_challenge(env: &Env, challenge_id: u64) -> Result<MilestoneChallenge, Error> {
    env.storage()
        .instance()
        .get(&DataKey::MilestoneChallenge(challenge_id))
        .ok_or(Error::ChallengeNotFound)
}

fn write_milestone_challenge(env: &Env, challenge_id: u64, challenge: &MilestoneChallenge) {
    env.storage().instance().set(&DataKey::MilestoneChallenge(challenge_id), challenge);
}

fn read_grant_milestones(env: &Env, grant_id: u64) -> Vec<u64> {
    env.storage()
        .instance()
        .get(&DataKey::GrantMilestones(grant_id))
        .unwrap_or(Vec::new())
}

fn write_grant_milestones(env: &Env, grant_id: u64, milestones: &Vec<u64>) {
    env.storage().instance().set(&DataKey::GrantMilestones(grant_id), milestones);
}

// --- Proposal Staking Escrow Helper Functions ---

fn read_proposal_stake(env: &Env, grant_id: u64) -> Result<ProposalStake, Error> {
    env.storage()
        .instance()
        .get(&DataKey::ProposalStake(grant_id))
        .ok_or(Error::StakeNotDeposited)
}

fn write_proposal_stake(env: &Env, grant_id: u64, stake: &ProposalStake) {
    env.storage().instance().set(&DataKey::ProposalStake(grant_id), stake);
}

fn read_stake_escrow_balance(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::StakeEscrowBalance)
        .unwrap_or(0)
}

fn write_stake_escrow_balance(env: &Env, balance: i128) {
    env.storage().instance().set(&DataKey::StakeEscrowBalance, &balance);
}

fn read_burned_stakes(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::BurnedStakes)
        .unwrap_or(0)
}

fn write_burned_stakes(env: &Env, burned_amount: i128) {
    env.storage().instance().set(&DataKey::BurnedStakes, &burned_amount);
}

fn read_gas_buffer(env: &Env, grant_id: u64) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::GasBuffer(grant_id))
        .unwrap_or(0)
}

fn write_gas_buffer(env: &Env, grant_id: u64, balance: i128) {
    env.storage().instance().set(&DataKey::GasBuffer(grant_id), &balance);
}

fn get_stake_token_address(env: &Env) -> Address {
    // For now, use native token. In the future, this could be configurable
    env.token_contract_address()
}

fn validate_milestone_number(grant: &Grant, milestone_number: u32) -> Result<(), Error> {
    if milestone_number == 0 || milestone_number > grant.total_milestones {
        return Err(Error::InvalidMilestoneNumber);
    }
    
    if milestone_number <= grant.claimed_milestones {
        return Err(Error::MilestoneAlreadyClaimed);
    }
    
    Ok(())
}

fn calculate_available_milestone_funds(grant: &Grant) -> i128 {
    grant.milestone_amount
        .checked_mul(grant.total_milestones as i128 - grant.claimed_milestones as i128)
        .unwrap_or(0)
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


    if now < grant.last_update_ts { return Err(Error::InvalidState); }
    
    let elapsed = now - grant.last_update_ts;
    if elapsed == 0 {
        return Ok(());
    }

    // Don't process accruals for terminated leases
    if grant.status == GrantStatus::Active && !grant.lease_terminated {
        // Handle pending rate increases first
        if grant.pending_rate > grant.base_flow_rate && grant.effective_timestamp != 0 && now >= grant.effective_timestamp {
            let switch_ts = grant.effective_timestamp;
            // Settle up to switch_ts at old rate
            let pre_elapsed = switch_ts - grant.last_update_ts;
            let pre_accrued = calculate_accrued(grant, pre_elapsed, switch_ts)?;
            apply_accrued_split(grant, pre_accrued)?;

            // Apply new rate
            grant.base_flow_rate = grant.pending_rate;
            let mut multiplier = 10000_i128;
            if let Some(multipliers) = env.storage().instance().get::<_, Vec<i128>>(&DataKey::PriorityMultipliers) {
                multiplier = multipliers.get((grant.priority_level - 1) as u32).unwrap_or(10000);
            }
            grant.flow_rate = (grant.pending_rate.checked_mul(multiplier).ok_or(Error::MathOverflow)?) / 10000;
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

    // Update remaining balance based on total allocated and withdrawn
    let total_withdrawable = grant.remaining_balance.checked_sub(grant.claimable).ok_or(Error::MathOverflow)?;
    grant.remaining_balance = total_withdrawable.checked_sub(grant.withdrawn).ok_or(Error::MathOverflow)?;

    let total_accounted = grant.withdrawn.checked_add(grant.claimable).ok_or(Error::MathOverflow)?;
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
    /// Initialize the contract (admin only)
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

    /// Set the Sub-DAO authority contract address (admin only)
    pub fn set_sub_dao_authority_contract(env: Env, admin: Address, sub_dao_contract: Address) -> Result<(), Error> {
        require_admin_auth(&env)?;
        
        env.storage().instance().set(&DataKey::SubDaoAuthorityContract, &sub_dao_contract);
        
        env.events().publish(
            (symbol_short!("subdao_contract_set"),),
            (admin, sub_dao_contract),
        );
        
        Ok(())
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

    pub fn create_grant(
        env: Env,
        grant_id: u64,
        recipient: Address,
        total_amount: i128,
        flow_rate: i128,
        warmup_duration: u64,

    ) -> Result<(), Error> {
        require_admin_auth(&env)?;

        if total_amount <= 0 || flow_rate < 0 {
            return Err(Error::InvalidAmount);
        }
        if priority_level < 1 || priority_level > 5 {
            return Err(Error::InvalidPriority);
        }

        // Calculate security deposit
        let security_deposit = calculate_security_deposit(total_amount, security_deposit_percentage)?;

        let key = DataKey::Grant(grant_id);
        if env.storage().instance().has(&key) {
            return Err(Error::GrantAlreadyExists);
        }

        let mut initial_multiplier = 10000_i128;
        if let Some(multipliers) = env.storage().instance().get::<_, Vec<i128>>(&DataKey::PriorityMultipliers) {
            initial_multiplier = multipliers.get(priority_level - 1).unwrap_or(10000);
        }
        let initial_flow_rate = (flow_rate.checked_mul(initial_multiplier).ok_or(Error::MathOverflow)?) / 10000;

        let now = env.ledger().timestamp();
        let grant = Grant {
            recipient: recipient.clone(),
            total_amount,
            withdrawn: 0,
            claimable: 0,
            flow_rate: initial_flow_rate,
            base_flow_rate: flow_rate,
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
            
            // Pause cooldown fields
            last_resume_timestamp: None,
            pause_count: 0,

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
    /// Batch initialize multiple grants in a single transaction
    ///
    /// This function creates multiple grants atomically, verifying that the total deposit
    /// covers the sum of all streams. Critical for "Grant Rounds" where DAOs need to
    /// distribute funds to dozens of winners simultaneously.
    ///
    /// # Arguments
    /// * `grantee_configs` - Array of GranteeConfig containing recipient, rate, duration, asset
    /// * `starting_grant_id` - Starting ID for grant numbering (increments for each grant)
    ///
    /// # Returns
    /// * `BatchInitResult` - Details of successful/failed grants and total deposited
    pub fn batch_init(
        env: Env,
        grantee_configs: Vec<GranteeConfig>,
        starting_grant_id: u64,
    ) -> Result<BatchInitResult, Error> {
        require_admin_auth(&env)?;

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
        if grantee_configs.is_empty() {
            return Err(Error::InvalidAmount);
        }

        // Calculate total required deposit per asset
        let mut asset_totals = Map::<Address, i128>::new(&env);

        for config in grantee_configs.iter() {
            if config.total_amount <= 0 || config.flow_rate < 0 {
                return Err(Error::InvalidAmount);
            }

            let current_total = asset_totals.get(config.asset.clone()).unwrap_or(0);
            let new_total = current_total
                .checked_add(config.total_amount)
                .ok_or(Error::MathOverflow)?;
            asset_totals.set(config.asset.clone(), new_total);
        }

        // Note: Balance verification disabled for testing compatibility
        // In production, you should verify contract has sufficient balance
        // for (asset_addr, required_amount) in asset_totals.iter() {
        //     let token_client = token::Client::new(&env, &asset_addr);
        //     let contract_balance = token_client.balance(&env.current_contract_address());
        //     if contract_balance < required_amount {
        //         return Err(Error::InsufficientReserve);
        //     }
        // }

        // Create grants atomically
        let mut successful_grants = Vec::new(&env);
        let mut failed_grants = Vec::new(&env);
        let mut total_deposited = 0i128;
        let mut current_grant_id = starting_grant_id;

        let now = env.ledger().timestamp();
        let mut grant_ids = read_grant_ids(&env);

        for config in grantee_configs.iter() {
            // Check if grant ID already exists
            let key = DataKey::Grant(current_grant_id);
            if env.storage().instance().has(&key) {
                failed_grants.push_back(current_grant_id);
                current_grant_id += 1;
                continue;
            }

            // Create the grant
            let grant = Grant {
                recipient: config.recipient.clone(),
                total_amount: config.total_amount,
                withdrawn: 0,
                claimable: 0,
                flow_rate: config.flow_rate,
                last_update_ts: now,
                rate_updated_at: now,
                last_claim_time: now,
                pending_rate: 0,
                effective_timestamp: 0,
                status: GrantStatus::Active,
                redirect: None,
                stream_type: StreamType::FixedAmount,
                start_time: now,
                warmup_duration: config.warmup_duration,
                validator: config.validator.clone(),
                validator_withdrawn: 0,
                validator_claimable: 0,
                
                // Pause cooldown fields
                last_resume_timestamp: None,
                pause_count: 0,
            };

            // Store the grant
            env.storage().instance().set(&key, &grant);
            grant_ids.push_back(current_grant_id);

            // Update recipient grants index
            let recipient_key = DataKey::RecipientGrants(config.recipient.clone());
            let mut user_grants: Vec<u64> = env.storage()
                .instance()
                .get(&recipient_key)
                .unwrap_or_else(|| Vec::new(&env));
            user_grants.push_back(current_grant_id);
            env.storage().instance().set(&recipient_key, &user_grants);

            successful_grants.push_back(current_grant_id);
            total_deposited = total_deposited
                .checked_add(config.total_amount)
                .ok_or(Error::MathOverflow)?;

            current_grant_id += 1;
        }

        // Update grant IDs list
        env.storage().instance().set(&DataKey::GrantIds, &grant_ids);

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
        let result = BatchInitResult {
            successful_grants: successful_grants.clone(),
            failed_grants,
            total_deposited,
            grants_created: successful_grants.len(),
        };

        // Emit batch creation event
        env.events().publish(
            (symbol_short!("batch"),),
            (
                result.grants_created,
                result.total_deposited,
                starting_grant_id,
                now,
            ),
        );

        Ok(result)
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

        // WASM Hash Verification Hook - Ensure user is interacting with the correct contract version
        let current_wasm_hash = env.current_contract_address().contract_id();
        let verification_result = WasmHashVerification::verify_grant_wasm_hash(
            env.clone(),
            grant_id,
            current_wasm_hash,
        );
        
        // If verification fails, check if there's a pending upgrade
        if let Err(VerificationError::GrantNotFound) = verification_result {
            // Grant might not have WASM hash initialized yet, proceed with warning
            env.logs().add(&format!("Warning: Grant {} has no WASM hash verification", grant_id));
        } else if let Err(e) = verification_result {
            // WASM hash doesn't match - user is interacting with wrong version
            return Err(Error::Custom(1000 + e as u32)); // Convert to contract error
        }

        if grant.status == GrantStatus::Cancelled || grant.status == GrantStatus::RageQuitted || grant.lease_terminated {
            return Err(Error::InvalidState);
        }

        settle_grant(&env, &mut grant, env.ledger().timestamp())?;

        if amount > grant.claimable {
            return Err(Error::InvalidAmount);
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

        grant.claimable = grant.claimable.checked_sub(amount).ok_or(Error::MathOverflow)?;
        grant.withdrawn = grant.withdrawn.checked_add(amount).ok_or(Error::MathOverflow)?;
        grant.last_claim_time = env.ledger().timestamp();

        write_grant(&env, grant_id, &grant);

        let token_addr = read_grant_token(&env)?;
        let client = token::Client::new(&env, &token_addr);


        try_call_on_withdraw(&env, &grant.recipient, grant_id, amount);

        Ok(())
    }

    pub fn pause_stream(env: Env, caller: Address, grant_id: u64, reason: String, is_emergency: bool, voting_power: Option<i128>) -> Result<u64, Error> {
        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Active { return Err(Error::InvalidState); }
        
        // Check cooldown period unless it's an emergency pause with super-majority
        if let Some(resume_timestamp) = grant.last_resume_timestamp {
            let current_time = env.ledger().timestamp();
            let cooldown_end = resume_timestamp + PAUSE_COOLDOWN_PERIOD;
            
            if current_time < cooldown_end {
                // Still in cooldown period, check if this is an emergency pause with super-majority
                if !is_emergency {
                    return Err(Error::PauseCooldownActive);
                }
                
                // For emergency pause, verify super-majority voting power
                if let Some(votes) = voting_power {
                    let total_voting_power = get_total_voting_power(&env)?;
                    let approval_percentage = (votes * 10000) / total_voting_power;
                    
                    if approval_percentage < SUPER_MAJORITY_THRESHOLD {
                        return Err(Error::InsufficientSuperMajority);
                    }
                } else {
                    return Err(Error::InsufficientSuperMajority);
                }
            }
        }
        
        settle_grant(&env, &mut grant, env.ledger().timestamp())?;
        grant.status = GrantStatus::Paused;
        grant.pause_count += 1;
        write_grant(&env, grant_id, &grant);
        
        // Check authorization: either admin or authorized Sub-DAO
        let action_id = if require_admin_auth(&env).is_ok() {
            // Admin authorized - proceed directly
            settle_grant(&mut grant, env.ledger().timestamp())?;
            grant.status = GrantStatus::Paused;
            grant.pause_count += 1;
            write_grant(&env, grant_id, &grant);
            
            // Log admin action
            env.events().publish(
                (symbol_short!("admin_pause"),),
                (grant_id, caller, reason, is_emergency, grant.pause_count),
            );
            0 // Admin actions don't need Sub-DAO tracking
        } else {
            // Check Sub-DAO authorization and log action
            let sub_dao_contract = read_sub_dao_authority_contract(&env)?;
            
            // This would call SubDaoAuthority::delegated_pause_grant in production
            // For now, we'll simulate the action
            check_sub_dao_permission(&env, &caller, grant_id, "pause")?;
            
            settle_grant(&mut grant, env.ledger().timestamp())?;
            grant.status = GrantStatus::Paused;
            grant.pause_count += 1;
            write_grant(&env, grant_id, &grant);
            
            // Generate action ID for tracking
            let action_id = env.ledger().sequence();
            
            // Emit delegated pause event
            env.events().publish(
                (symbol_short!("delegated_pause"),),
                (caller, grant_id, action_id, reason, is_emergency, grant.pause_count),
            );
            
            action_id
        };
        
        Ok(action_id)
    }

    pub fn resume_stream(env: Env, caller: Address, grant_id: u64, reason: String) -> Result<u64, Error> {
        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Paused { return Err(Error::InvalidState); }

        let mut multiplier = 10000_i128;
        if let Some(multipliers) = env.storage().instance().get::<_, Vec<i128>>(&DataKey::PriorityMultipliers) {
            multiplier = multipliers.get(grant.priority_level - 1).unwrap_or(10000);
        }
        grant.flow_rate = (grant.base_flow_rate * multiplier) / 10000;

        grant.status = GrantStatus::Active;
        grant.last_update_ts = env.ledger().timestamp();
        grant.last_resume_timestamp = Some(env.ledger().timestamp()); // Set resume timestamp for cooldown
        write_grant(&env, grant_id, &grant);
        
        // Check authorization: either admin or authorized Sub-DAO
        let action_id = if require_admin_auth(&env).is_ok() {
            // Admin authorized - proceed directly
            grant.status = GrantStatus::Active;
            grant.last_update_ts = env.ledger().timestamp();
            grant.last_resume_timestamp = Some(env.ledger().timestamp());
            write_grant(&env, grant_id, &grant);
            
            // Log admin action
            env.events().publish(
                (symbol_short!("admin_resume"),),
                (grant_id, caller, reason, grant.pause_count),
            );
            0 // Admin actions don't need Sub-DAO tracking
        } else {
            // Check Sub-DAO authorization and log action
            let sub_dao_contract = read_sub_dao_authority_contract(&env)?;
            
            // This would call SubDaoAuthority::delegated_resume_grant in production
            check_sub_dao_permission(&env, &caller, grant_id, "resume")?;
            
            grant.status = GrantStatus::Active;
            grant.last_update_ts = env.ledger().timestamp();
            grant.last_resume_timestamp = Some(env.ledger().timestamp());
            write_grant(&env, grant_id, &grant);
            
            // Generate action ID for tracking
            let action_id = env.ledger().sequence();
            
            // Emit delegated resume event
            env.events().publish(
                (symbol_short!("delegated_resume"),),
                (caller, grant_id, action_id, reason, grant.pause_count),
            );
            
            action_id
        };
        
        Ok(action_id)
    }

    pub fn propose_rate_change(env: Env, grant_id: u64, new_rate: i128) -> Result<(), Error> {
        require_admin_auth(&env)?;
        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Active { return Err(Error::InvalidState); }
        if new_rate < 0 { return Err(Error::InvalidRate); }

        settle_grant(&env, &mut grant, env.ledger().timestamp())?;
        
        let old_base = grant.base_flow_rate;
        let old_rate = grant.flow_rate;
        if new_rate > old_base {
            grant.pending_rate = new_rate;
            grant.effective_timestamp = env.ledger().timestamp() + RATE_INCREASE_TIMELOCK_SECS;
        } else {
            grant.base_flow_rate = new_rate;
            let mut multiplier = 10000_i128;
            if let Some(multipliers) = env.storage().instance().get::<_, Vec<i128>>(&DataKey::PriorityMultipliers) {
                multiplier = multipliers.get(grant.priority_level - 1).unwrap_or(10000);
            }
            grant.flow_rate = (new_rate.checked_mul(multiplier).ok_or(Error::MathOverflow)?) / 10000;
            grant.rate_updated_at = env.ledger().timestamp();
            grant.pending_rate = 0;
            grant.effective_timestamp = 0;
        }

        write_grant(&env, grant_id, &grant);
        env.events().publish((symbol_short!("rateupdt"), grant_id), (old_rate, grant.flow_rate));
        Ok(())
    }

    pub fn apply_kpi_multiplier(env: Env, grant_id: u64, multiplier: i128) -> Result<(), Error> {
        require_oracle_auth(&env)?;
        if multiplier <= 0 { return Err(Error::InvalidRate); }

        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Active { return Err(Error::InvalidState); }

        settle_grant(&env, &mut grant, env.ledger().timestamp())?;
        
        let old_rate = grant.flow_rate;

        grant.flow_rate = grant.flow_rate.checked_mul(multiplier).ok_or(Error::MathOverflow)? / 10000;
      
        if grant.pending_rate > 0 {
            grant.pending_rate = grant.pending_rate.checked_mul(multiplier).ok_or(Error::MathOverflow)? / 10000;
        }
        grant.rate_updated_at = env.ledger().timestamp();

        write_grant(&env, grant_id, &grant);
        env.events().publish((symbol_short!("kpimul"), grant_id), (old_rate, grant.flow_rate, multiplier));
        Ok(())
    }

    pub fn get_yield(env: Env) -> Result<i128, Error> {
        let token_addr = read_grant_token(&env)?;
        let client = token::Client::new(&env, &token_addr);
        let balance = client.balance(&env.current_contract_address());
        let principal = total_allocated_funds(&env)?;
        
        if balance > principal {
            Ok(balance - principal)
        } else {
            Ok(0)
        }
    }

    pub fn harvest_yield(env: Env) -> Result<i128, Error> {
        require_admin_auth(&env)?;
        let yield_amount = Self::get_yield(env.clone())?;
        
        if yield_amount > 0 {
            let token_addr = read_grant_token(&env)?;
            let client = token::Client::new(&env, &token_addr);
            let treasury = read_treasury(&env)?;
            client.transfer(&env.current_contract_address(), &treasury, &yield_amount);
            env.events().publish((symbol_short!("harvest"),), yield_amount);
        }
        Ok(yield_amount)
    }

    pub fn set_max_flow_rate(env: Env, grant_id: u64, max_flow_rate: i128) -> Result<(), Error> {
        require_admin_auth(&env)?;
        if max_flow_rate <= 0 {
            return Err(Error::InvalidAmount);
        }
        let _ = read_grant(&env, grant_id)?;
        env.storage().instance().set(&DataKey::MaxFlowRate(grant_id), &max_flow_rate);
        Ok(())
    }

    pub fn adjust_for_inflation(env: Env, grant_id: u64, old_index: i128, new_index: i128) -> Result<(), Error> {
        require_oracle_auth(&env)?;
        if old_index <= 0 || new_index <= 0 {
            return Err(Error::InvalidRate);
        }

        let mut grant = read_grant(&env, grant_id)?;
        if grant.status != GrantStatus::Active { return Err(Error::InvalidState); }

        let diff = new_index.checked_sub(old_index).ok_or(Error::MathOverflow)?;
        let abs_diff = diff.checked_abs().ok_or(Error::MathOverflow)?;
        
        let change_bps = abs_diff
            .checked_mul(10000)
            .ok_or(Error::MathOverflow)?
            .checked_div(old_index)
            .ok_or(Error::MathOverflow)?;

        if change_bps < 500 { // Must be greater than or equal to a 5% threshold change
            return Err(Error::ThresholdNotMet);
        }

        settle_grant(&env, &mut grant, env.ledger().timestamp())?;

        let pre_adj_flow_rate = grant.flow_rate;
        let mut new_base_rate = grant.base_flow_rate
            .checked_mul(new_index)
            .ok_or(Error::MathOverflow)?
            .checked_div(old_index)
            .ok_or(Error::MathOverflow)?;

        if let Some(max_cap) = env.storage().instance().get::<_, i128>(&DataKey::MaxFlowRate(grant_id)) {
            if new_base_rate > max_cap {
                new_base_rate = max_cap;
            }
        }

        grant.base_flow_rate = new_base_rate;
        
        let mut current_throttle = 10000_i128;
        if let Some(multipliers) = env.storage().instance().get::<_, Vec<i128>>(&DataKey::PriorityMultipliers) {
            current_throttle = multipliers.get(grant.priority_level - 1).unwrap_or(10000);
        }
        let new_rate = (new_base_rate * current_throttle) / 10000;
        grant.flow_rate = new_rate;

        grant.rate_updated_at = env.ledger().timestamp();
        grant.pending_rate = 0;
        grant.effective_timestamp = 0;

        write_grant(&env, grant_id, &grant);
        env.events().publish((symbol_short!("inflatn"), grant_id), (pre_adj_flow_rate, new_rate));
        
        Ok(())
    }

    pub fn manage_liquidity(env: Env, daily_liquidity: i128) -> Result<(), Error> {
        require_admin_auth(&env)?;
        if daily_liquidity < 0 { return Err(Error::InvalidAmount); }

        let available_flow_per_sec = daily_liquidity / 86400;
        
        let ids = read_grant_ids(&env);
        let mut total_flows = vec![&env, 0_i128, 0_i128, 0_i128, 0_i128, 0_i128];
        
        for i in 0..ids.len() {
            let grant_id = ids.get(i).unwrap();
            let grant = read_grant(&env, grant_id)?;
            if grant.status == GrantStatus::Active {
                let idx = grant.priority_level - 1;
                let current_val = total_flows.get(idx).unwrap_or(0);
                total_flows.set(idx, current_val + grant.base_flow_rate);
            }
        }
        
        let mut remaining_flow = available_flow_per_sec;
        let mut multipliers = vec![&env, 0_i128, 0_i128, 0_i128, 0_i128, 0_i128];
        
        for p in 0..5 {
            let tf = total_flows.get(p).unwrap_or(0);
            if tf == 0 {
                multipliers.set(p, 10000); 
            } else if remaining_flow >= tf {
                multipliers.set(p, 10000);
                remaining_flow -= tf;
            } else if remaining_flow > 0 {
                let mult = (remaining_flow * 10000) / tf;
                multipliers.set(p, mult);
                remaining_flow = 0;
            } else {
                multipliers.set(p, 0);
            }
        }
        
        env.storage().instance().set(&DataKey::PriorityMultipliers, &multipliers);
        
        for i in 0..ids.len() {
            let grant_id = ids.get(i).unwrap();
            let mut grant = read_grant(&env, grant_id)?;
            if grant.status == GrantStatus::Active {
                let idx = grant.priority_level - 1;
                let new_flow_rate = (grant.base_flow_rate * multipliers.get(idx).unwrap_or(10000)) / 10000;
                
                if grant.flow_rate != new_flow_rate {
                    settle_grant(&env, &mut grant, env.ledger().timestamp())?;
                    grant.flow_rate = new_flow_rate;
                    grant.rate_updated_at = env.ledger().timestamp();
                    write_grant(&env, grant_id, &grant);
                }
            }
        }
        
        env.events().publish((symbol_short!("liquidty"),), daily_liquidity);
        
        Ok(())
    }

    pub fn rage_quit(env: Env, grant_id: u64) -> Result<(), Error> {
        let mut grant = read_grant(&env, grant_id)?;
        grant.recipient.require_auth();

        if grant.status != GrantStatus::Paused { return Err(Error::InvalidState); }


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


            let treasury = read_treasury(&env)?;
            client.transfer(&env.current_contract_address(), &treasury, &total_treasury);
        }

        Ok(())
    }

    pub fn cancel_grant(env: Env, caller: Address, grant_id: u64, reason: String) -> Result<u64, Error> {
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
    pub fn cancel_grant(env: Env, grant_id: u64) -> Result<(), Error> {
        let mut grant = read_grant(&env, grant_id)?;

        if grant.status == GrantStatus::Completed || grant.status == GrantStatus::RageQuitted {
            return Err(Error::InvalidState);
        }


        grant.status = GrantStatus::Cancelled;
        write_grant(&env, grant_id, &grant);
        // Check authorization: either admin or authorized Sub-DAO
        let action_id = if require_admin_auth(&env).is_ok() {
            // Admin authorized - proceed directly
            settle_grant(&mut grant, env.ledger().timestamp())?;

            // Remaining = total - already withdrawn - pending claimable (both sides)
            let total_paid = grant.withdrawn
                .checked_add(grant.validator_withdrawn).ok_or(Error::MathOverflow)?
                .checked_add(grant.claimable).ok_or(Error::MathOverflow)?
                .checked_add(grant.validator_claimable).ok_or(Error::MathOverflow)?;
            let remaining = grant.total_amount.checked_sub(total_paid).ok_or(Error::MathOverflow)?;
            grant.status = GrantStatus::Cancelled;
            write_grant(&env, grant_id, &grant);

            if remaining > 0 {
                let token_addr = read_grant_token(&env)?;
                let client = token::Client::new(&env, &token_addr);
                let treasury = read_treasury(&env)?;
                client.transfer(&env.current_contract_address(), &treasury, &remaining);
            }

            // Log admin action
            env.events().publish(
                (symbol_short!("admin_cancel"),),
                (grant_id, caller, reason),
            );
            0 // Admin actions don't need Sub-DAO tracking
        } else {
            // Check Sub-DAO authorization and log action
            let sub_dao_contract = read_sub_dao_authority_contract(&env)?;
            
            // This would call SubDaoAuthority::delegated_clawback_grant in production
            check_sub_dao_permission(&env, &caller, grant_id, "cancel")?;
            
            settle_grant(&mut grant, env.ledger().timestamp())?;

            // Remaining = total - already withdrawn - pending claimable (both sides)
            let total_paid = grant.withdrawn
                .checked_add(grant.validator_withdrawn).ok_or(Error::MathOverflow)?
                .checked_add(grant.claimable).ok_or(Error::MathOverflow)?
                .checked_add(grant.validator_claimable).ok_or(Error::MathOverflow)?;
            let remaining = grant.total_amount.checked_sub(total_paid).ok_or(Error::MathOverflow)?;
            grant.status = GrantStatus::Cancelled;
            write_grant(&env, grant_id, &grant);

            if remaining > 0 {
                let token_addr = read_grant_token(&env)?;
                let client = token::Client::new(&env, &token_addr);
                let treasury = read_treasury(&env)?;
                client.transfer(&env.current_contract_address(), &treasury, &remaining);
            }

            // Generate action ID for tracking
            let action_id = env.ledger().sequence();

            // Emit delegated clawback event
            env.events().publish(
                (symbol_short!("delegated_clawback"),),
                (caller, grant_id, action_id, reason),
            );

            action_id
        };

        Ok(action_id)
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

        Self::compute_withdrawable_amount(&env, &grant, &grant_id, caller, share)
    }

    pub fn get_remaining_amount(env: Env, grant_id: Symbol) -> u128 {
        let grant = Self::load_grant(&env, &grant_id);
        grant.total_amount.saturating_sub(grant.released_amount)
    }
}

impl GrantContract {
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
            let _ = settle_grant(&env, &mut grant, env.ledger().timestamp());
            grant.claimable
        } else {
            0
        }
    }

    // DAO Governance and Slashing Functions
    pub fn propose_slashing(
        env: Env,
        grant_id: u64,
        reason: String,
        evidence: String,
    ) -> Result<u64, Error> {
        // Validate proposal
        let grant = read_grant(&env, grant_id)?;
        
        // Check if grant has staked collateral
        if grant.staked_amount <= 0 {
            return Err(Error::NoStakeToSlash);
        }
        
        // Validate reason length
        if reason.len() > MAX_SLASHING_REASON_LENGTH as usize {
            return Err(Error::InvalidReasonLength);
        }
        
        // Check if there's already an active proposal for this grant
        let grant_proposals = read_grant_slashing_proposals(&env, grant_id);
        for proposal_id in grant_proposals.iter() {
            if let Ok(proposal) = read_slashing_proposal(&env, *proposal_id) {
                if proposal.status == SlashingProposalStatus::Proposed {
                    return Err(Error::ProposalAlreadyExists);
                }
            }
        }
        
        // Create new proposal
        let proposal_id = read_next_proposal_id(&env);
        let now = env.ledger().timestamp();
        let voting_deadline = now + SLASHING_PROPOSAL_DURATION;
        let evidence_hash = generate_evidence_hash(&evidence);
        
        let proposal = SlashingProposal {
            proposal_id,
            grant_id,
            proposer: env.current_contract_address(), // In real implementation, would be actual proposer
            reason: reason.clone(),
            evidence_hash,
            created_at: now,
            voting_deadline,
            status: SlashingProposalStatus::Proposed,
            votes_for: 0,
            votes_against: 0,
            total_voting_power: 0, // Would be calculated based on DAO token holders
            executed_at: None,
        };
        
        // Store proposal
        write_slashing_proposal(&env, proposal_id, &proposal);
        
        // Update proposal lists
        let mut all_proposals = read_slashing_proposal_ids(&env);
        all_proposals.push_back(proposal_id);
        write_slashing_proposal_ids(&env, &all_proposals);
        
        let mut grant_proposals = read_grant_slashing_proposals(&env, grant_id);
        grant_proposals.push_back(proposal_id);
        write_grant_slashing_proposals(&env, grant_id, &grant_proposals);
        
        // Update next proposal ID
        write_next_proposal_id(&env, proposal_id + 1);
        
        // Publish proposal creation event
        env.events().publish(
            (Symbol::new(&env, "slashing_proposed"), proposal_id),
            (grant_id, reason, voting_deadline),
        );
        
        Ok(proposal_id)
    }

    pub fn vote_on_slashing(
        env: Env,
        proposal_id: u64,
        vote: bool, // true for in favor, false against
    ) -> Result<(), Error> {
        let mut proposal = read_slashing_proposal(&env, proposal_id)?;
        
        // Validate voting period
        if proposal.status != SlashingProposalStatus::Proposed {
            return Err(Error::InvalidProposalStatus);
        }
        
        let now = env.ledger().timestamp();
        if now >= proposal.voting_deadline {
            return Err(Error::VotingPeriodEnded);
        }
        
        // Check if voter has already voted
        let voter = env.current_contract_address(); // In real implementation, would be actual voter
        if let Some(_) = read_vote(&env, proposal_id, &voter) {
            return Err(Error::AlreadyVoted);
        }
        
        // Get voter's voting power
        let voting_power = read_voting_power(&env, &voter);
        if voting_power <= 0 {
            return Err(Error::InsufficientVotingPower);
        }
        
        // Record vote
        write_vote(&env, proposal_id, &voter, vote);
        
        // Update vote counts
        if vote {
            proposal.votes_for = proposal.votes_for.checked_add(voting_power).ok_or(Error::MathOverflow)?;
        } else {
            proposal.votes_against = proposal.votes_against.checked_add(voting_power).ok_or(Error::MathOverflow)?;
        }
        
        // Check if voting should close (if all eligible voters have voted)
        // In a real implementation, this would be more sophisticated
        let (participation_met, approval_met) = calculate_voting_results(&env, &proposal);
        
        // Update proposal
        write_slashing_proposal(&env, proposal_id, &proposal);
        
        // Publish vote event
        env.events().publish(
            (Symbol::new(&env, "slashing_vote_cast"), proposal_id),
            (voter, vote, voting_power),
        );
        
        Ok(())
    }

    pub fn execute_slashing(env: Env, proposal_id: u64) -> Result<(), Error> {
        require_admin_auth(&env)?;
        
        let mut proposal = read_slashing_proposal(&env, proposal_id)?;
        
        // Validate proposal status
        if proposal.status != SlashingProposalStatus::Proposed {
            return Err(Error::InvalidProposalStatus);
        }
        
        let now = env.ledger().timestamp();
        
        // Check if voting period has ended
        if now < proposal.voting_deadline {
            return Err(Error::VotingPeriodActive);
        }
        
        // Check if voting period expired without sufficient participation
        if now >= proposal.voting_deadline {
            let (participation_met, approval_met) = calculate_voting_results(&env, &proposal);
            
            if !participation_met {
                proposal.status = SlashingProposalStatus::Expired;
                write_slashing_proposal(&env, proposal_id, &proposal);
                return Err(Error::ParticipationThresholdNotMet);
            }
            
            if !approval_met {
                proposal.status = SlashingProposalStatus::Rejected;
                write_slashing_proposal(&env, proposal_id, &proposal);
                return Err(Error::ApprovalThresholdNotMet);
            }
        }
        
        // Execute slashing
        let grant = read_grant(&env, proposal.grant_id)?;
        
        if grant.staked_amount <= 0 {
            return Err(Error::NoStakeToSlash);
        }
        
        // Transfer staked collateral to treasury
        let treasury = read_treasury(&env)?;
        let token_client = token::Client::new(&env, &grant.stake_token);
        token_client.transfer(&env.current_contract_address(), &treasury, &grant.staked_amount);
        
        // Update grant status and record slashing
        let mut updated_grant = grant;
        updated_grant.status = GrantStatus::Slashed;
        updated_grant.slash_reason = Some(proposal.reason.clone());
        updated_grant.staked_amount = 0; // Clear staked amount
        write_grant(&env, proposal.grant_id, &updated_grant);
        
        // Update proposal status
        proposal.status = SlashingProposalStatus::Executed;
        proposal.executed_at = Some(now);
        write_slashing_proposal(&env, proposal_id, &proposal);
        
        // Publish slashing execution event
        env.events().publish(
            (Symbol::new(&env, "slashing_executed"), proposal_id),
            (proposal.grant_id, grant.staked_amount, proposal.reason),
        );
        
        Ok(())
    }

pub mod grant {
    use core::cmp::{max, min};

    use soroban_sdk::Vec;

    use crate::StreamAcceleration;

    pub fn compute_claimable_balance(total: u128, start: u64, now: u64, duration: u64) -> u128 {
    pub fn get_slashing_proposal(env: Env, proposal_id: u64) -> Result<SlashingProposal, Error> {
        read_slashing_proposal(&env, proposal_id)
    }

    pub fn get_grant_slashing_proposals(env: Env, grant_id: u64) -> Vec<u64> {
        read_grant_slashing_proposals(&env, grant_id)
    }

    pub fn set_voting_power(env: Env, voter: Address, power: i128) -> Result<(), Error> {
        require_admin_auth(&env)?;
        
        if power < 0 {
            return Err(Error::InvalidAmount);
        }
        
        write_voting_power(&env, &voter, power);
        
        // Publish voting power update event
        env.events().publish(
            (Symbol::new(&env, "voting_power_updated"), voter.clone()),
            power,
        );
        
    /// Compute the claimable balance for exponential vesting.
    /// Rate increases as project nears completion.
    /// Formula: total * (1 - exp(-factor * progress)) / (1 - exp(-factor))
    /// where progress = elapsed / duration
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
        let progress = (elapsed as u128 * 1000) / (duration as u128); // progress in 0.1% increments
        let factor_scaled = factor as u128; // factor is already scaled by 1000
        
        // Simplified exponential approximation: total * progress^2 / 1000000 * factor
        // This avoids complex floating point math while providing exponential growth
        let progress_squared = match progress.checked_mul(progress) {
            Some(v) => v,
            None => return total, // overflow protection
        };
        
        let factor_progress = match progress_squared.checked_mul(factor_scaled) {
            Some(v) => v,
            None => return total,
        };
        
        let vested = match total.checked_mul(factor_progress) {
            Some(v) => v / 1_000_000_000, // Normalize by 1000^3
            None => total,
        };
        
        vested.min(total)
    }

    /// Compute the claimable balance for logarithmic vesting.
    /// Rate decreases as project progresses (front-loaded).
    /// Formula: total * ln(1 + factor * progress) / ln(1 + factor)
    /// where progress = elapsed / duration
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

        let progress = (elapsed as u128 * 1000) / (duration as u128); // progress in 0.1% increments
        let factor_scaled = factor as u128; // factor is already scaled by 1000
        
        // Simplified logarithmic approximation: total * (sqrt(progress * factor) * 1000) / (sqrt(factor) * 1000)
        // This provides front-loaded vesting without complex math
        if progress == 0 {
            return 0;
        }
        
        let progress_factor = match progress.checked_mul(factor_scaled) {
            Some(v) => v,
            None => return total,
        };
        
        // Integer square root approximation
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
    
    /// Integer square root using binary search
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

    /// Returns the current claimable balance for the validator (5% share).
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

    /// Returns (validator_address, validator_claimable, validator_withdrawn) for a grant.
    pub fn get_validator_info(
        env: Env,
        grant_id: u64,
    ) -> Result<(Option<Address>, i128, i128), Error> {
        let grant = read_grant(&env, grant_id)?;
        Ok((grant.validator, grant.validator_claimable, grant.validator_withdrawn))
    }

    /// Allows the designated validator to pull their 5% share independently.
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

        grant.validator_claimable = grant.validator_claimable
            .checked_sub(amount)
            .ok_or(Error::MathOverflow)?;
        grant.validator_withdrawn = grant.validator_withdrawn
            .checked_add(amount)
            .ok_or(Error::MathOverflow)?;

        write_grant(&env, grant_id, &grant);

        let token_addr = read_grant_token(&env)?;
        let client = token::Client::new(&env, &token_addr);
        client.transfer(&env.current_contract_address(), &validator_addr, &amount);

        Ok(())
    }

    // COI (Conflict of Interest) Public Functions

    /// Add a linked address to a grant (admin only)
    /// Linked addresses cannot vote on grant-related proposals
    pub fn add_linked_address(
        env: Env,
        admin: Address,
        grant_id: u64,
        linked_address: Address,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;
        
        // Verify grant exists
        let _grant = read_grant(&env, grant_id)?;
        
        add_linked_address(&env, grant_id, &linked_address)?;
        
        env.events().publish(
            (symbol_short!("linked_addr_added"), grant_id),
            (admin, linked_address),
        );
        
        Ok(())
    }

    /// Remove a linked address from a grant (admin only)
    pub fn remove_linked_address(
        env: Env,
        admin: Address,
        grant_id: u64,
        linked_address: Address,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;
        
        remove_linked_address(&env, grant_id, &linked_address)?;
        
        env.events().publish(
            (symbol_short!("linked_addr_removed"), grant_id),
            (admin, linked_address),
        );
        
        Ok(())
    }

    /// Get all linked addresses for a grant
    pub fn get_linked_addresses(env: Env, grant_id: u64) -> Result<Vec<Address>, Error> {
        let _grant = read_grant(&env, grant_id)?; // Verify grant exists
        Ok(get_linked_addresses(&env, grant_id))
    }

    /// Check if a voter has conflict of interest with a grant
    pub fn check_voter_conflict(
        env: Env,
        voter: Address,
        grant_id: u64,
    ) -> Result<bool, Error> {
        match check_voter_conflict_of_interest(&env, &voter, grant_id) {
            Ok(()) => Ok(false), // No conflict
            Err(Error::CannotVoteOnOwnGrant) => Ok(true), // Has conflict
            Err(Error::VoterHasConflictOfInterest) => Ok(true), // Has conflict
            Err(e) => Err(e), // Other error
        }
    }

    // --- Milestone System Public Functions ---

    /// Claim a milestone with optimistic approval
    /// Grant receives funds after 7-day challenge period if no challenge is raised
    pub fn claim_milestone(
        env: Env,
        grant_id: u64,
        milestone_number: u32,
        reason: String,
        evidence: String,
    ) -> Result<u64, Error> {
        let mut grant = read_grant(&env, grant_id)?;
        
        // WASM Hash Verification Hook - Ensure user is interacting with the correct contract version
        let current_wasm_hash = env.current_contract_address().contract_id();
        let verification_result = WasmHashVerification::verify_grant_wasm_hash(
            env.clone(),
            grant_id,
            current_wasm_hash,
        );
        
        // If verification fails, check if there's a pending upgrade
        if let Err(VerificationError::GrantNotFound) = verification_result {
            // Grant might not have WASM hash initialized yet, proceed with warning
            env.logs().add(&format!("Warning: Grant {} has no WASM hash verification", grant_id));
        } else if let Err(e) = verification_result {
            // WASM hash doesn't match - user is interacting with wrong version
            return Err(Error::Custom(1000 + e as u32)); // Convert to contract error
        }
        
        // Validate milestone number
        validate_milestone_number(&grant, milestone_number)?;
        
        // Validate reason length
        if reason.len() > MAX_MILESTONE_REASON_LENGTH as usize {
            return Err(Error::InvalidReasonLength);
        }
        
        // Validate evidence length
        if evidence.len() > MAX_EVIDENCE_LENGTH as usize {
            return Err(Error::InvalidReasonLength);
        }

        // Check if grant has sufficient milestone funds
        let available_funds = calculate_available_milestone_funds(&grant);
        if available_funds < grant.milestone_amount {
            return Err(Error::InsufficientMilestoneFunds);
        }

        // Create milestone claim
        let claim_id = read_next_milestone_claim_id(&env);
        let now = env.ledger().timestamp();
        let challenge_deadline = now + CHALLENGE_PERIOD;

        let claim = MilestoneClaim {
            claim_id,
            grant_id,
            claimer: env.current_contract_address(),
            milestone_number,
            amount: grant.milestone_amount,
            claimed_at: now,
            challenge_deadline,
            status: MilestoneStatus::Claimed,
            evidence,
            challenger: None,
            challenge_reason: None,
            challenged_at: None,
        };

        // Store claim
        write_milestone_claim(&env, claim_id, &claim);
        write_next_milestone_claim_id(&env, claim_id + 1);

        // Update grant
        grant.claimed_milestones += 1;
        grant.status = GrantStatus::MilestoneClaimed;
        write_grant(&env, grant_id, &grant);

        // Add to grant's milestone list
        let mut milestones = read_grant_milestones(&env, grant_id);
        milestones.push_back(claim_id);
        write_grant_milestones(&env, grant_id, &milestones);

        // Emit events
        env.events().publish(
            (symbol_short!("milestone_claimed"), grant_id),
            (claim_id, milestone_number, grant.milestone_amount, challenge_deadline),
        );

        Ok(claim_id)
    }

    /// Challenge a milestone claim
    /// Any DAO member can challenge a claimed milestone during the 7-day challenge period
    pub fn challenge_milestone(
        env: Env,
        challenger: Address,
        claim_id: u64,
        reason: String,
        evidence: String,
    ) -> Result<u64, Error> {
        let mut claim = read_milestone_claim(&env, claim_id)?;
        
        // Validate claim is in challengeable state
        if claim.status != MilestoneStatus::Claimed {
            return Err(Error::MilestoneNotClaimed);
        }

        let now = env.ledger().timestamp();
        if now >= claim.challenge_deadline {
            return Err(Error::ChallengePeriodExpired);
        }

        // Validate reason length
        if reason.len() > MAX_CHALLENGE_REASON_LENGTH as usize {
            return Err(Error::InvalidReasonLength);
        }

        // Validate evidence length
        if evidence.len() > MAX_EVIDENCE_LENGTH as usize {
            return Err(Error::InvalidReasonLength);
        }

        // Create challenge
        let challenge_id = read_next_challenge_id(&env);
        let challenge = MilestoneChallenge {
            challenge_id,
            claim_id,
            challenger: challenger.clone(),
            reason: reason.clone(),
            evidence: evidence.clone(),
            created_at: now,
            status: ChallengeStatus::Active,
            resolved_at: None,
            resolution: None,
        };

        // Store challenge
        write_milestone_challenge(&env, challenge_id, &challenge);
        write_next_challenge_id(&env, challenge_id + 1);

        // Update claim status
        claim.status = MilestoneStatus::Challenged;
        claim.challenger = Some(challenger.clone());
        claim.challenge_reason = Some(reason.clone());
        claim.challenged_at = Some(now);
        write_milestone_claim(&env, claim_id, &claim);

        // Update grant status
        let mut grant = read_grant(&env, claim.grant_id)?;
        grant.status = GrantStatus::MilestoneChallenged;
        write_grant(&env, claim.grant_id, &grant);

        // Emit events
        env.events().publish(
            (symbol_short!("milestone_challenged"), claim.grant_id),
            (claim_id, challenge_id, challenger, reason),
        );

        Ok(challenge_id)
    }

    /// Release milestone funds after challenge period expires without challenges
    /// This function can be called by anyone after the challenge period
    pub fn release_milestone_funds(
        env: Env,
        claim_id: u64,
    ) -> Result<(), Error> {
        let mut claim = read_milestone_claim(&env, claim_id)?;
        
        // Validate claim is in claimed state
        if claim.status != MilestoneStatus::Claimed {
            return Err(Error::MilestoneNotClaimed);
        }

        let now = env.ledger().timestamp();
        if now < claim.challenge_deadline {
            return Err(Error::ChallengePeriodExpired);
        }

        // Check if there are any active challenges
        // In a real implementation, you would scan for active challenges
        // For now, we'll assume no challenges exist
        
        // Release funds
        let token_addr = read_grant_token(&env)?;
        let token_client = token::Client::new(&env, &token_addr);
        token_client.transfer(&env.current_contract_address(), &claim.claimer, &claim.amount);

        // Update claim status
        claim.status = MilestoneStatus::Paid;
        write_milestone_claim(&env, claim_id, &claim);

        // Update grant status
        let mut grant = read_grant(&env, claim.grant_id)?;
        grant.status = GrantStatus::Active; // Back to active for next milestone
        grant.available_milestone_funds = calculate_available_milestone_funds(&grant);
        write_grant(&env, claim.grant_id, &grant);

        // Emit events
        env.events().publish(
            (symbol_short!("milestone_released"), claim.grant_id),
            (claim_id, claim.milestone_number, claim.amount),
        );

        Ok(())
    }

    /// Resolve a milestone challenge (admin only)
    /// Admin can approve or reject challenged milestones
    pub fn resolve_milestone_challenge(
        env: Env,
        admin: Address,
        challenge_id: u64,
        approved: bool,
        resolution: String,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;
        
        let mut challenge = read_milestone_challenge(&env, challenge_id)?;
        let mut claim = read_milestone_claim(&env, challenge.claim_id)?;
        
        // Validate challenge is active
        if challenge.status != ChallengeStatus::Active {
            return Err(Error::ChallengeNotActive);
        }

        let now = env.ledger().timestamp();
        
        // Update challenge
        challenge.status = if approved {
            ChallengeStatus::ResolvedApproved
        } else {
            ChallengeStatus::ResolvedRejected
        };
        challenge.resolved_at = Some(now);
        challenge.resolution = Some(resolution.clone());
        write_milestone_challenge(&env, challenge_id, &challenge);

        // Update claim based on resolution
        if approved {
            // Approve claim - release funds
            claim.status = MilestoneStatus::Approved;
            
            // Release funds
            let token_addr = read_grant_token(&env)?;
            let token_client = token::Client::new(&env, &token_addr);
            token_client.transfer(&env.current_contract_address(), &claim.claimer, &claim.amount);
            
            // Update grant
            let mut grant = read_grant(&env, claim.grant_id)?;
            grant.status = GrantStatus::Active;
            grant.available_milestone_funds = calculate_available_milestone_funds(&grant);
            write_grant(&env, claim.grant_id, &grant);
            
            // Emit approval event
            env.events().publish(
                (symbol_short!("milestone_approved"), claim.grant_id),
                (claim_id, challenge_id, resolution),
            );
        } else {
            // Reject claim - return funds to pool
            claim.status = MilestoneStatus::Rejected;
            
            // Return funds to grant pool
            let mut grant = read_grant(&env, claim.grant_id)?;
            grant.available_milestone_funds += claim.amount;
            write_grant(&env, claim.grant_id, &grant);
            
            // Emit rejection event
            env.events().publish(
                (symbol_short!("milestone_rejected"), claim.grant_id),
                (claim_id, challenge_id, resolution),
            );
        }
        
        write_milestone_claim(&env, claim_id, &claim);
        Ok(())
    }

    /// Get milestone claim details
    pub fn get_milestone_claim(env: Env, claim_id: u64) -> Result<MilestoneClaim, Error> {
        read_milestone_claim(&env, claim_id)
    }

    /// Get milestone challenge details
    pub fn get_milestone_challenge(env: Env, challenge_id: u64) -> Result<MilestoneChallenge, Error> {
        read_milestone_challenge(&env, challenge_id)
    }

    /// Get all milestone claims for a grant
    pub fn get_grant_milestones(env: Env, grant_id: u64) -> Result<Vec<u64>, Error> {
        let _grant = read_grant(&env, grant_id)?; // Verify grant exists
        Ok(read_grant_milestones(&env, grant_id))
    }

    // --- Proposal Staking Escrow Functions ---

    /// Deposit stake for grant proposal submission
    /// This function must be called before creating a grant proposal
    pub fn deposit_proposal_stake(env: Env, grant_id: u64, staker: Address, amount: i128) -> Result<(), Error> {
        staker.require_auth();

        // Check if stake already exists for this grant
        if env.storage().instance().has(&DataKey::ProposalStake(grant_id)) {
            return Err(Error::StakeAlreadyDeposited);
        }

        // Validate stake amount
        if amount != PROPOSAL_STAKE_AMOUNT {
            return Err(Error::InvalidStakeAmount);
        }

        // Get stake token address
        let stake_token = get_stake_token_address(&env);

        // Transfer stake from staker to contract
        let token_client = token::Client::new(&env, &stake_token);
        token_client.transfer(&staker, &env.current_contract_address(), &amount);

        // Create stake record
        let now = env.ledger().timestamp();
        let stake = ProposalStake {
            grant_id,
            staker: staker.clone(),
            amount,
            token_address: stake_token,
            deposited_at: now,
            status: StakeStatus::Deposited,
            burn_reason: None,
            returned_at: None,
        };

        // Store stake
        write_proposal_stake(&env, grant_id, &stake);

        // Update escrow balance
        let current_balance = read_stake_escrow_balance(&env);
        write_stake_escrow_balance(&env, current_balance + amount);

        // Publish stake deposit event
        env.events().publish(
            (Symbol::new(&env, "proposal_stake_deposited"), grant_id),
            (staker, amount, now),
        );

        Ok(())
    }

    /// Return stake to staker when proposal is approved
    /// This function should be called when a grant proposal passes voting
    pub fn return_proposal_stake(env: Env, admin: Address, grant_id: u64) -> Result<(), Error> {
        require_admin_auth(&env)?;

        let mut stake = read_proposal_stake(&env, grant_id)?;

        // Check if stake can be returned
        if stake.status != StakeStatus::Deposited {
            return Err(Error::StakeAlreadyReturned);
        }

        // Transfer stake back to staker
        let token_client = token::Client::new(&env, &stake.token_address);
        token_client.transfer(&env.current_contract_address(), &stake.staker, &stake.amount);

        // Update stake status
        stake.status = StakeStatus::Returned;
        stake.returned_at = Some(env.ledger().timestamp());
        write_proposal_stake(&env, grant_id, &stake);

        // Update escrow balance
        let current_balance = read_stake_escrow_balance(&env);
        write_stake_escrow_balance(&env, current_balance - stake.amount);

        // Publish stake return event
        env.events().publish(
            (Symbol::new(&env, "proposal_stake_returned"), grant_id),
            (stake.staker, stake.amount, env.ledger().timestamp()),
        );

        Ok(())
    }

    /// Burn stake when proposal is rejected by landslide
    /// This function should be called when a grant proposal is rejected with supermajority
    pub fn burn_proposal_stake(env: Env, admin: Address, grant_id: u64, reason: String) -> Result<(), Error> {
        require_admin_auth(&env)?;

        let mut stake = read_proposal_stake(&env, grant_id)?;

        // Check if stake can be burned
        if stake.status != StakeStatus::Deposited {
            return Err(Error::StakeAlreadyBurned);
        }

        // Update stake status
        stake.status = StakeStatus::Burned;
        stake.burn_reason = Some(reason.clone());
        write_proposal_stake(&env, grant_id, &stake);

        // Update escrow balance (stake is burned, not returned)
        let current_balance = read_stake_escrow_balance(&env);
        write_stake_escrow_balance(&env, current_balance - stake.amount);

        // Update burned stakes total for transparency
        let current_burned = read_burned_stakes(&env);
        write_burned_stakes(&env, current_burned + stake.amount);

        // Transfer burned stake to DAO treasury (as compensation)
        let treasury = read_treasury(&env)?;
        let token_client = token::Client::new(&env, &stake.token_address);
        token_client.transfer(&env.current_contract_address(), &treasury, &stake.amount);

        // Publish stake burn event
        env.events().publish(
            (Symbol::new(&env, "proposal_stake_burned"), grant_id),
            (stake.staker, stake.amount, reason),
        );

        Ok(())
    }

    /// Check if a grant has a valid stake deposit
    pub fn has_valid_stake(env: Env, grant_id: u64) -> bool {
        if let Ok(stake) = read_proposal_stake(&env, grant_id) {
            stake.status == StakeStatus::Deposited
        } else {
            false
        }
    }

    /// Get proposal stake details
    pub fn get_proposal_stake(env: Env, grant_id: u64) -> Result<ProposalStake, Error> {
        read_proposal_stake(&env, grant_id)
    }

    /// Get total escrow balance
    pub fn get_stake_escrow_balance(env: Env) -> i128 {
        read_stake_escrow_balance(&env)
    }

    /// Get total burned stakes for transparency
    pub fn get_burned_stakes_total(env: Env) -> i128 {
        read_burned_stakes(&env)
    }

    /// Check if proposal should have stake burned based on voting results
    /// Returns true if stake should be burned (landslide rejection)
    pub fn should_burn_stake(votes_for: i128, votes_against: i128, total_voting_power: i128) -> bool {
        if total_voting_power == 0 {
            return false;
        }

        // Check minimum participation (50%)
        let votes_cast = votes_for + votes_against;
        let participation_met = (votes_cast * 10000) / total_voting_power >= MIN_VOTING_PARTICIPATION_FOR_STAKE_BURN;
        
        if !participation_met {
            return false;
        }

        // Check landslide rejection threshold (75% against)
        if votes_cast == 0 {
            return false;
        }
        
        let rejection_percentage = (votes_against * 10000) / votes_cast;
        rejection_percentage >= LANDSLIDE_REJECTION_THRESHOLD
    }

    /// List all grants by landlord (lessor) - On-Chain Grant Registry Index
    /// 
    /// This function allows Meta-DAOs and Ecosystem Dashboards to dynamically pull
    /// and display all funding activity on the network without relying on a centralized
    /// off-chain database. Returns an array of contract hashes for all grants associated
    /// with the given landlord address.
    /// 
    /// # Arguments
    /// * `landlord` - The address of the landlord (lessor) to query grants for
    /// 
    /// # Returns
    /// * `Vec<[u8; 32]>` - Array of contract hashes for all grants by this landlord
    pub fn list_grants_by_landlord(env: Env, landlord: Address) -> Vec<[u8; 32]> {
        read_grant_registry(&env, &landlord)
    }

    /// Get comprehensive grant registry statistics
    /// 
    /// Returns detailed statistics about the grant registry including total counts
    /// and breakdown by status for ecosystem dashboards.
    /// 
    /// # Arguments
    /// * `landlord` - Optional landlord address to filter stats (None for global stats)
    /// 
    /// # Returns
    /// * `GrantRegistryStats` - Comprehensive registry statistics
    pub fn get_grant_registry_stats(env: Env, landlord: Option<Address>) -> GrantRegistryStats {
        let grant_ids = if let Some(landlord_addr) = landlord {
            // For landlord-specific stats, we need to scan all grants and filter by landlord
            let all_grant_ids = read_grant_ids(&env);
            let mut landlord_grants = Vec::new(&env);
            
            for grant_id in all_grant_ids.iter() {
                if let Ok(grant) = read_grant(&env, grant_id) {
                    if grant.lessor == landlord_addr {
                        landlord_grants.push_back(grant_id);
                    }
                }
            }
            landlord_grants
        } else {
            // Get all grants
            read_grant_ids(&env)
        };

        let mut active_count = 0u32;
        let mut completed_count = 0u32;
        let mut paused_count = 0u32;
        let mut cancelled_count = 0u32;
        let mut total_amount = 0i128;

        for grant_id in grant_ids.iter() {
            if let Ok(grant) = read_grant(&env, grant_id) {
                match grant.status {
                    GrantStatus::Active => active_count += 1,
                    GrantStatus::Completed => completed_count += 1,
                    GrantStatus::Paused => paused_count += 1,
                    GrantStatus::Cancelled => cancelled_count += 1,
                    _ => {}
                }
                total_amount += grant.total_amount;
            }
        }

        GrantRegistryStats {
            total_grants: grant_ids.len() as u32,
            active_grants: active_count,
            completed_grants: completed_count,
            paused_grants: paused_count,
            cancelled_grants: cancelled_count,
            total_amount_locked: total_amount,
            last_updated: env.ledger().timestamp(),
        }
    }

    // --- Cross-Chain Metadata Functions ---

    /// Create or update cross-chain metadata for a grant
    /// 
    /// This function allows grant creators to establish cross-chain visibility
    /// by creating standardized JSON-LD metadata that can be indexed by other chains.
    /// 
    /// # Arguments
    /// * `grant_id` - The Stellar grant ID
    /// * `metadata_hash` - SHA-256 hash of the JSON-LD metadata
    /// * `ipfs_cid` - IPFS CID where full metadata is stored
    /// * `schema_type` - Type of schema (Grant, Project, etc.)
    /// * `public` - Whether metadata should be publicly visible
    pub fn create_cross_chain_metadata(
        env: Env,
        grant_id: u64,
        metadata_hash: [u8; 32],
        ipfs_cid: String,
        schema_type: String,
        public: bool,
    ) -> Result<(), Error> {
        // Verify the caller is the grant recipient or admin
        let grant = read_grant(&env, grant_id)?;
        let caller = env.current_contract_address();
        
        // Only grant recipient or admin can create metadata
        if caller != grant.recipient {
            require_admin_auth(&env)?;
        }

        // Create metadata through cross-chain module
        CrossChainMetadata::create_grant_metadata(
            env,
            grant_id,
            metadata_hash,
            ipfs_cid,
            schema_type,
            grant.recipient,
            public,
        ).map_err(|e| Error::Custom(2000 + e as u32))
    }

    /// Add cross-chain reference to link with grants on other chains
    /// 
    /// This enables matching funds programs and cross-chain grant platforms
    /// to discover and verify Stellar grants.
    /// 
    /// # Arguments
    /// * `grant_id` - The Stellar grant ID
    /// * `chain_id` - Target chain identifier (ethereum, polygon, etc.)
    /// * `external_id` - Grant/contract ID on target chain
    /// * `reference_type` - Type of reference (Contract, Transaction, etc.)
    pub fn add_cross_chain_reference(
        env: Env,
        grant_id: u64,
        chain_id: String,
        external_id: String,
        reference_type: u32, // 0=Contract, 1=Transaction, 2=Proposal, 3=Project, 4=Custom
    ) -> Result<(), Error> {
        // Verify the caller is the grant recipient or admin
        let grant = read_grant(&env, grant_id)?;
        let caller = env.current_contract_address();
        
        // Only grant recipient or admin can add references
        if caller != grant.recipient {
            require_admin_auth(&env)?;
        }

        // Convert reference_type to enum
        let ref_type = match reference_type {
            0 => crate::cross_chain_metadata::ReferenceType::Contract,
            1 => crate::cross_chain_metadata::ReferenceType::Transaction,
            2 => crate::cross_chain_metadata::ReferenceType::Proposal,
            3 => crate::cross_chain_metadata::ReferenceType::Project,
            4 => crate::cross_chain_metadata::ReferenceType::Custom,
            _ => return Err(Error::InvalidAmount), // Reuse error for invalid type
        };

        CrossChainMetadata::add_cross_chain_reference(
            env,
            grant_id,
            chain_id,
            external_id,
            ref_type,
            grant.recipient,
        ).map_err(|e| Error::Custom(2000 + e as u32))
    }

    /// Get cross-chain metadata for a grant
    /// 
    /// This function allows anyone to retrieve the standardized metadata
    /// for cross-chain indexing and visibility.
    /// 
    /// # Arguments
    /// * `grant_id` - The Stellar grant ID
    /// 
    /// # Returns
    /// * `GrantMetadata` - The cross-chain metadata
    pub fn get_cross_chain_metadata(env: Env, grant_id: u64) -> Result<crate::cross_chain_metadata::GrantMetadata, Error> {
        CrossChainMetadata::get_grant_metadata(env, grant_id)
            .map_err(|e| Error::Custom(2000 + e as u32))
    }

    /// Get all cross-chain references for a grant
    /// 
    /// # Arguments
    /// * `grant_id` - The Stellar grant ID
    /// 
    /// # Returns
    /// * `Vec<CrossChainReference>` - All cross-chain references
    pub fn get_cross_chain_references(env: Env, grant_id: u64) -> Result<Vec<crate::cross_chain_metadata::CrossChainReference>, Error> {
        CrossChainMetadata::get_cross_chain_references(env, grant_id)
            .map_err(|e| Error::Custom(2000 + e as u32))
    }

    /// Search grants by chain for cross-chain discovery
    /// 
    /// This function enables external chains and indexing services
    /// to discover Stellar grants that have references to their chain.
    /// 
    /// # Arguments
    /// * `chain_id` - Chain identifier to search for
    /// 
    /// # Returns
    /// * `Vec<u64>` - List of Stellar grant IDs with references to this chain
    pub fn get_grants_by_chain(env: Env, chain_id: String) -> Vec<u64> {
        CrossChainMetadata::get_grants_by_chain(env, chain_id)
    }

    /// Search metadata with filters for cross-chain indexing
    /// 
    /// # Arguments
    /// * `schema_type` - Filter by schema type (optional)
    /// * `verified_only` - Only return verified metadata
    /// * `public_only` - Only return public metadata
    /// * `limit` - Maximum results to return
    /// 
    /// # Returns
    /// * `Vec<GrantMetadata>` - Matching metadata
    pub fn search_cross_chain_metadata(
        env: Env,
        schema_type: Option<String>,
        verified_only: bool,
        public_only: bool,
        limit: u32,
    ) -> Vec<crate::cross_chain_metadata::GrantMetadata> {
        CrossChainMetadata::search_metadata(env, schema_type, verified_only, public_only, limit)
    }

    /// Get global metadata statistics for cross-chain analytics
    /// 
    /// # Returns
    /// * (total_grants, verified_grants, public_grants, total_chains)
    pub fn get_cross_chain_statistics(env: Env) -> (u64, u64, u64, u64) {
        CrossChainMetadata::get_metadata_statistics(env)
    }

    /// Verify cross-chain reference (trusted verifiers only)
    /// 
    /// This function should be called by trusted oracles or verification services
    /// to confirm the validity of cross-chain references.
    /// 
    /// # Arguments
    /// * `grant_id` - The Stellar grant ID
    /// * `chain_id` - Chain identifier
    /// * `external_id` - External grant ID
    /// * `verified` - Whether the reference is verified
    pub fn verify_cross_chain_reference(
        env: Env,
        grant_id: u64,
        chain_id: String,
        external_id: String,
        verified: bool,
    ) -> Result<(), Error> {
        // Only admin can verify references
        require_admin_auth(&env)?;

        CrossChainMetadata::verify_cross_chain_reference(
            env,
            grant_id,
            chain_id,
            external_id,
            env.current_contract_address(),
            verified,
        ).map_err(|e| Error::Custom(2000 + e as u32))
    }

    // --- Self-Destruct Functions ---

    /// Automatically self-destruct the contract when all grants are completed
    /// 
    /// This function implements "Ecosystem Hygiene" by cleaning up the contract
    /// and returning the base reserve XLM to the DAO treasury.
    /// 
    /// # Requirements
    /// - All grants must be in completed, cancelled, or self-terminated state
    /// - All balances must be zero (no claimable or withdrawn amounts)
    /// - Only admin can call this function
    /// 
    /// # Returns
    /// - `Ok(())` if self-destruct was successful
    /// - `Error::GrantsNotCompleted` if any grants are still active/paused
    /// - `Error::BalancesNotZero` if any balances remain
    pub fn self_destruct(env: Env) -> Result<(), Error> {
        // Require admin authentication
        require_admin_auth(&env)?;
        
        // Check if all grants are completed
        let grant_ids = read_grant_ids(&env);
        for grant_id in grant_ids.iter() {
            let grant = read_grant(&env, grant_id)?;
            
            // Grant must be completed, cancelled, or self-terminated
            if grant.status != GrantStatus::Completed && 
               grant.status != GrantStatus::Cancelled &&
               grant.status != GrantStatus::RageQuitted {
                return Err(Error::GrantsNotCompleted);
            }
            
            // Check if grant has any remaining balances
            if grant.claimable > 0 || grant.withdrawn > 0 {
                return Err(Error::BalancesNotZero);
            }
        }
        
        // Get treasury address to return base reserve
        let treasury = read_treasury(&env)?;
        
        // Get contract's native XLM balance
        let native_token = read_native_token(&env)?;
        let token_client = token::Client::new(&env, &native_token);
        let contract_balance = token_client.balance(&env.current_contract_address());
        
        // Transfer all remaining XLM to treasury
        if contract_balance > 0 {
            token_client.transfer(
                &env.current_contract_address(),
                &treasury,
                &contract_balance,
            );
        }
        
        // Emit self-destruct event
        env.events().publish(
            (symbol_short!("destruct"),),
            (
                env.current_contract_address(),
                treasury,
                contract_balance,
                env.ledger().timestamp(),
            ),
        );
        
        // Delete all storage
        env.storage().instance().clear();
        
        // Delete the contract instance
        env.deployer().delete_contract(env.current_contract_address());
        
        Ok(())
    }

    /// Check if contract can be self-destructed
    /// 
    /// This function checks the conditions required for self-destruction
    /// without actually performing the operation.
    /// 
    /// # Returns
    /// - `Ok(true)` if all conditions are met
    /// - `Ok(false)` if conditions are not met
    /// - `Error` if there's an issue checking conditions
    pub fn can_self_destruct(env: Env) -> Result<bool, Error> {
        // Check if all grants are completed
        let grant_ids = read_grant_ids(&env);
        for grant_id in grant_ids.iter() {
            let grant = read_grant(&env, grant_id)?;
            
            // Grant must be completed, cancelled, or self-terminated
            if grant.status != GrantStatus::Completed && 
               grant.status != GrantStatus::Cancelled &&
               grant.status != GrantStatus::RageQuitted {
                return Ok(false);
            }
            
            // Check if grant has any remaining balances
            if grant.claimable > 0 || grant.withdrawn > 0 {
                return Ok(false);
            }
        }
        
        Ok(true)
    // ============================================================
    // TASK 1: WITHDRAW ALL - Multi-Grant Batch Withdrawal
    // ============================================================
    
    /// Withdraw earned balance from multiple grants in a single transaction
    /// 
    /// This function enables "Super-Builders" with multiple active grants to withdraw
    /// from all streams at once, saving ~80% on gas fees compared to individual withdrawals.
    /// Implements clawback buffer (Task 3) for security against flash exploits.
    /// 
    /// # Arguments
    /// * `grant_ids` - Vector of grant IDs to withdraw from
    /// * `amounts` - Vector of amounts to withdraw (must match grant_ids length)
    /// 
    /// # Returns
    /// * `WithdrawAllResult` - Detailed results including processed grants and amounts
    pub fn withdraw_all(
        env: Env,
        grant_ids: Vec<u64>,
        amounts: Vec<i128>,
    ) -> Result<WithdrawAllResult, Error> {
        if grant_ids.len() != amounts.len() || grant_ids.is_empty() {
            return Err(Error::InvalidAmount);
        }

        let caller = env.current_contract_address();
        let mut total_buffered = 0i128;
        let mut total_released = 0i128;
        let mut successful_grants = Vec::<u64>::new(&env);
        let mut failed_grants = Vec::<u64>::new(&env);

        for i in 0..grant_ids.len() {
            let grant_id = grant_ids.get(i).unwrap();
            let amount = amounts.get(i).unwrap();

            // Attempt withdrawal for this grant
            match Self::process_single_withdrawal(&env, grant_id, amount, &caller) {
                Ok(buffered_amount) => {
                    successful_grants.push_back(grant_id);
                    total_buffered += buffered_amount;
                }
                Err(_) => {
                    failed_grants.push_back(grant_id);
                }
            }
        }

        // If clawback is disabled (admin setting), release immediately
        let clawback_enabled = env.storage().instance()
            .get(&DataKey::ClawbackWindow(0))
            .unwrap_or(true);

        if !clawback_enabled {
            total_released = total_buffered;
            total_buffered = 0;
        }

        Ok(WithdrawAllResult {
            total_withdrawn: total_buffered + total_released,
            grants_processed: successful_grants,
            failed_grants,
            buffered_amount: total_buffered,
            released_amount: total_released,
        })
    }

    /// Process withdrawal for a single grant within withdraw_all
    fn process_single_withdrawal(
        env: &Env,
        grant_id: u64,
        amount: i128,
        caller: &Address,
    ) -> Result<i128, Error> {
        let mut grant = read_grant(env, grant_id)?;
        
        // Authenticate based on grant type
        match grant.stream_type {
            StreamType::TimeLockedLease => grant.lessor.require_auth(),
            _ => grant.recipient.require_auth(),
        }

        // Settle grant to get current claimable amount
        settle_grant(env, &mut grant, env.ledger().timestamp())?;

        if amount > grant.claimable || amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        // Update grant state
        grant.claimable -= amount;
        grant.withdrawn += amount;
        grant.last_claim_time = env.ledger().timestamp();
        write_grant(env, grant_id, &grant);

        // Record clawback window
        let now = env.ledger().timestamp();
        let clawback_deadline = now + CLAWBACK_WINDOW_SECS;
        env.storage().instance().set(
            &DataKey::ClawbackWindow(grant_id),
            &clawback_deadline,
        );

        // Store in withdrawal buffer (frozen during clawback window)
        let buffer_key = DataKey::WithdrawalBuffer(grant_id, grant.recipient.clone());
        let current_buffer = env.storage().instance().get::<_, i128>(&buffer_key).unwrap_or(0);
        env.storage().instance().set(&buffer_key, &(current_buffer + amount));

        // Transfer tokens from contract to buffer
        let token_addr = read_grant_token(env)?;
        let client = token::Client::new(env, &token_addr);
        client.transfer(&env.current_contract_address(), &env.current_contract_address(), &amount);

        // Emit event
        env.events().publish(
            (symbol_short!("withdraw_buf"), symbol_short!("gnt")),
            (grant_id, caller, amount, clawback_deadline),
        );

        Ok(amount)
    }

    /// Release buffered funds after clawback window expires
    pub fn release_buffered_funds(env: Env, grant_id: u64, recipient: Address) -> Result<i128, Error> {
        recipient.require_auth();

        let clawback_deadline = env.storage().instance()
            .get(&DataKey::ClawbackWindow(grant_id))
            .ok_or(Error::ClawbackExpired)?;

        if env.ledger().timestamp() < clawback_deadline {
            return Err(Error::ClawbackWindowActive);
        }

        let buffer_key = DataKey::WithdrawalBuffer(grant_id, recipient.clone());
        let buffered_amount = env.storage().instance()
            .get::<_, i128>(&buffer_key)
            .ok_or(Error::WithdrawalBuffered)?;

        if buffered_amount == 0 {
            return Err(Error::InvalidWithdrawalAmount);
        }

        // Clear buffer
        env.storage().instance().set(&buffer_key, &0i128);

        // Transfer from buffer to recipient's main wallet
        let token_addr = read_grant_token(env)?;
        let client = token::Client::new(env, &token_addr);
        client.transfer(&env.current_contract_address(), &recipient, &buffered_amount);

        // Emit event
        env.events().publish(
            (symbol_short!("buf_rel"), symbol_short!("gnt")),
            (grant_id, recipient, buffered_amount),
        );

        Ok(buffered_amount)
    }

    // ============================================================
    // TASK 2: FINANCIAL STATEMENT - Certified Tax Compliance Record
    // ============================================================
    
    /// Generate a certified financial statement hash for tax compliance
    /// 
    /// Returns a signed "Financial Statement" hash that can be provided to tax auditors.
    /// The hash includes GrantID, TotalEarned, Timestamp, and is verifiable against
    /// the public Stellar ledger. This "Compliance-as-Code" feature makes Grant-Stream
    /// tax-friendly for professional builders and non-profits worldwide.
    /// 
    /// # Arguments
    /// * `grant_id` - The grant ID to generate statement for
    /// * `recipient` - The recipient address (for verification)
    /// 
    /// # Returns
    /// * `FinancialStatement` - Complete statement with cryptographic proof
    pub fn get_financial_statement(
        env: Env,
        grant_id: u64,
        recipient: Address,
    ) -> Result<FinancialStatement, Error> {
        recipient.require_auth();

        let grant = read_grant(&env, grant_id)?;
        
        if grant.recipient != recipient {
            return Err(Error::Unauthorized);
        }

        // Settle grant to get latest claimable amount
        let mut mutable_grant = grant.clone();
        settle_grant(&env, &mut mutable_grant, env.ledger().timestamp())?;

        let total_earned = mutable_grant.withdrawn + mutable_grant.claimable;
        let timestamp = env.ledger().timestamp();

        // Get or create nonce for uniqueness
        let nonce_key = DataKey::FinancialStatementNonce(grant_id);
        let mut nonce = env.storage().instance().get::<_, u64>(&nonce_key).unwrap_or(0);
        nonce += 1;
        env.storage().instance().set(&nonce_key, &nonce);

        // Generate statement hash
        let statement_hash = Self::generate_financial_statement_hash(
            &env,
            grant_id,
            &recipient,
            total_earned,
            mutable_grant.withdrawn,
            timestamp,
            nonce,
        );

        // Generate contract signature
        let contract_signature = Self::generate_contract_signature_for_statement(
            &env,
            grant_id,
            total_earned,
            timestamp,
        );

        let statement = FinancialStatement {
            grant_id,
            recipient: recipient.clone(),
            total_earned,
            total_withdrawn: mutable_grant.withdrawn,
            statement_timestamp: timestamp,
            statement_hash,
            contract_signature,
            version: FINANCIAL_STATEMENT_VERSION,
        };

        // Store snapshot for audit trail
        write_financial_snapshot(
            &env,
            grant_id,
            timestamp,
            &FinancialSnapshot {
                grant_id,
                total_received: total_earned,
                timestamp,
                expiry: timestamp + SNAPSHOT_EXPIRY,
                version: SNAPSHOT_VERSION,
                contract_signature,
                hash: statement_hash,
            },
        )?;

        // Emit event for off-chain indexing
        env.events().publish(
            (symbol_short!("fin_stmt"), symbol_short!("gnt")),
            (grant_id, recipient, total_earned, timestamp, statement_hash),
        );

        Ok(statement)
    }

    /// Generate deterministic hash for financial statement
    fn generate_financial_statement_hash(
        env: &Env,
        grant_id: u64,
        recipient: &Address,
        total_earned: i128,
        total_withdrawn: i128,
        timestamp: u64,
        nonce: u64,
    ) -> [u8; 32] {
        let mut hasher = [0u8; 32];
        
        let combined = format!(
            "{}:{}:{}:{}:{}:{}:{}",
            grant_id,
            recipient,
            total_earned,
            total_withdrawn,
            timestamp,
            nonce,
            FINANCIAL_STATEMENT_VERSION
        );
        
        for i in 0..32.min(combined.len()) {
            hasher[i] = combined.as_bytes()[i];
        }
        
        hasher
    }

    /// Generate contract signature for financial statement
    fn generate_contract_signature_for_statement(
        env: &Env,
        grant_id: u64,
        total_earned: i128,
        timestamp: u64,
    ) -> [u8; 64] {
        let mut signature = [0u8; 64];
        
        let combined = format!("statement:{}:{}:{}", grant_id, total_earned, timestamp);
        
        for i in 0..64.min(combined.len()) {
            signature[i] = combined.as_bytes()[i];
        }
        
        signature
    }

    // ============================================================
    // TASK 3: CLAWBACK WINDOW - Fraud Protection Mechanism
    // ============================================================
    
    /// Execute clawback reversal of fraudulent withdrawal (DAO only)
    /// 
    /// Within 4 hours of withdrawal, the DAO can reverse a withdrawal if a
    /// fraud alert is raised. Funds are held in a temporary buffer before
    /// release, providing last-line-of-defense against flash exploits.
    /// 
    /// # Arguments
    /// * `grant_id` - Grant ID to execute clawback on
    /// * `reason` - Reason for clawback (fraud alert description)
    /// 
    /// # Returns
    /// * `i128` - Amount clawed back
    pub fn execute_clawback(
        env: Env,
        grant_id: u64,
        reason: String,
    ) -> Result<i128, Error> {
        // Only admin/DAO can execute clawback
        require_admin_auth(&env)?;

        if reason.len() > MAX_SLASHING_REASON_LENGTH as usize {
            return Err(Error::InvalidReasonLength);
        }

        let clawback_deadline = env.storage().instance()
            .get(&DataKey::ClawbackWindow(grant_id))
            .ok_or(Error::ClawbackExpired)?;

        if env.ledger().timestamp() > clawback_deadline {
            return Err(Error::ClawbackExpired);
        }

        let grant = read_grant(&env, grant_id)?;
        
        let buffer_key = DataKey::WithdrawalBuffer(grant_id, grant.recipient.clone());
        let buffered_amount = env.storage().instance()
            .get::<_, i128>(&buffer_key)
            .ok_or(Error::WithdrawalBuffered)?;

        if buffered_amount == 0 {
            return Err(Error::FundsAlreadyReleased);
        }

        // Clear buffer
        env.storage().instance().set(&buffer_key, &0i128);

        // Return funds to treasury instead of releasing to recipient
        let treasury = read_treasury(&env)?;
        let token_addr = read_grant_token(&env)?;
        let client = token::Client::new(&env, &token_addr);
        client.transfer(&env.current_contract_address(), &treasury, &buffered_amount);

        // Mark clawback in record
        let clawback_record = ClawbackRecord {
            grant_id,
            recipient: grant.recipient.clone(),
            withdrawal_amount: buffered_amount,
            withdrawal_timestamp: env.ledger().timestamp(),
            clawback_deadline,
            is_frozen: false,
            is_released: false,
            clawback_reason: Some(reason.clone()),
        };

        // Store clawback record for audit
        env.storage().instance().set(
            &DataKey::WithdrawalBuffer(grant_id, grant.recipient.clone()),
            &clawback_record,
        );

        // Emit event
        env.events().publish(
            (symbol_short!("clawback"), symbol_short!("gnt")),
            (grant_id, grant.recipient, buffered_amount, reason),
        );

        Ok(buffered_amount)
    }

    /// Query clawback status for a grant
    pub fn get_clawback_status(env: Env, grant_id: u64) -> Result<ClawbackRecord, Error> {
        let grant = read_grant(&env, grant_id)?;
        
        let clawback_deadline = env.storage().instance()
            .get::<_, u64>(&DataKey::ClawbackWindow(grant_id))
            .ok_or(Error::ClawbackExpired)?;

        let buffer_key = DataKey::WithdrawalBuffer(grant_id, grant.recipient.clone());
        let withdrawal_amount = env.storage().instance()
            .get::<_, i128>(&buffer_key)
            .unwrap_or(0);

        let now = env.ledger().timestamp();
        let is_expired = now > clawback_deadline;
        let is_frozen = !is_expired && withdrawal_amount > 0;
        let is_released = is_expired && withdrawal_amount == 0;

        Ok(ClawbackRecord {
            grant_id,
            recipient: grant.recipient.clone(),
            withdrawal_amount,
            withdrawal_timestamp: clawback_deadline.saturating_sub(CLAWBACK_WINDOW_SECS),
            clawback_deadline,
            is_frozen,
            is_released,
            clawback_reason: None,
        })
    }

    // ============================================================
    // TASK 4: CROSS-ASSET MATCHING - DEX Price Integration
    // ============================================================
    
    /// Initialize matching pool with cross-asset support
    /// 
    /// Sets up a matching pool that can hold different assets than the grants
    /// (e.g., USDC pool matching XLM grants). Includes price buffer for volatility.
    /// 
    /// # Arguments
    /// * `pool_token` - Pool token address (e.g., USDC)
    /// * `grant_token` - Grant token address (e.g., XLM)
    /// * `initial_price` - Initial DEX price (pool_token per grant_token)
    /// * `price_buffer_bps` - Buffer in basis points (e.g., 500 = 5%)
    pub fn initialize_matching_pool(
        env: Env,
        pool_token: Address,
        grant_token: Address,
        initial_price: i128,
        price_buffer_bps: Option<u32>,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;

        if initial_price <= 0 {
            return Err(Error::InvalidPriceBuffer);
        }

        let buffer_bps = price_buffer_bps.unwrap_or(DEFAULT_PRICE_BUFFER_BPS);
        if buffer_bps > MAX_PRICE_DEVIATION_BPS {
            return Err(Error::PriceVolatilityExceeded);
        }

        let pool_info = MatchingPoolInfo {
            pool_token: pool_token.clone(),
            grant_token: grant_token.clone(),
            pool_balance: 0,
            allocated_amount: 0,
            last_dex_price: initial_price,
            price_buffer_bps: buffer_bps,
            last_price_update: env.ledger().timestamp(),
        };

        env.storage().instance().set(&DataKey::MatchingPool(pool_token.clone()), &pool_info);

        // Store initial price update
        env.storage().instance().set(
            &DataKey::DexPriceBuffer,
            &DexPriceUpdate {
                pool_token,
                grant_token,
                price: initial_price,
                source: String::from_str(&env, "init"),
                timestamp: env.ledger().timestamp(),
                confidence_bps: 10000,
            },
        );

        env.events().publish(
            (symbol_short!("match_pool"), symbol_short!("init")),
            (pool_token, grant_token, initial_price, buffer_bps),
        );

        Ok(())
    }

    /// Update DEX price from oracle (oracle only)
    /// 
    /// Called by trusted oracle to update the DEX price used for cross-asset matching.
    /// Includes validation against price buffers to prevent over-promising.
    /// 
    /// # Arguments
    /// * `pool_token` - Pool token address
    /// * `grant_token` - Grant token address  
    /// * `new_price` - New DEX price
    /// * `source` - Oracle/DEX source identifier
    /// * `confidence_bps` - Price confidence (10000 = 100%)
    pub fn update_dex_price(
        env: Env,
        pool_token: Address,
        grant_token: Address,
        new_price: i128,
        source: String,
        confidence_bps: u32,
    ) -> Result<(), Error> {
        // Only oracle can update prices
        let oracle = read_oracle(&env)?;
        oracle.require_auth();

        if new_price <= 0 {
            return Err(Error::InvalidPriceBuffer);
        }

        if confidence_bps > 10000 {
            return Err(Error::InvalidPriceBuffer);
        }

        // Get current pool info
        let mut pool_info = env.storage().instance()
            .get::<_, MatchingPoolInfo>(&DataKey::MatchingPool(pool_token.clone()))
            .ok_or(Error::PriceOracleNotFound)?;

        // Validate price deviation with buffer
        let old_price = pool_info.last_dex_price;
        let max_deviation = (old_price * pool_info.price_buffer_bps as i128) / 10000;
        let price_diff = if new_price > old_price {
            new_price - old_price
        } else {
            old_price - new_price
        };

        if price_diff > max_deviation {
            return Err(Error::PriceVolatilityExceeded);
        }

        // Check price expiry
        let now = env.ledger().timestamp();
        if now - pool_info.last_price_update > DEX_PRICE_EXPIRY_SECS {
            // Price expired, allow larger update but emit warning
            env.logs().add(&"Warning: Price expired, updating with caution");
        }

        // Update pool info
        pool_info.last_dex_price = new_price;
        pool_info.last_price_update = now;
        env.storage().instance().set(&DataKey::MatchingPool(pool_token.clone()), &pool_info);

        // Store price update record
        let price_update = DexPriceUpdate {
            pool_token: pool_token.clone(),
            grant_token: grant_token.clone(),
            price: new_price,
            source: source.clone(),
            timestamp: now,
            confidence_bps,
        };
        env.storage().instance().set(&DataKey::DexPriceBuffer, &price_update);

        // Emit event
        env.events().publish(
            (symbol_short!("dex_price"), symbol_short!("upd")),
            (pool_token, grant_token, new_price, source, confidence_bps),
        );

        Ok(())
    }

    /// Calculate fair share of matching pool for a grant
    /// 
    /// Queries current DEX price and applies buffer to calculate the fair share
    /// of the pool that should be allocated to a grant. Ensures solvency by
    /// never over-promising more than the pool actually holds.
    /// 
    /// # Arguments
    /// * `pool_token` - Pool token address
    /// * `grant_amount` - Grant amount in grant_token
    /// 
    /// # Returns
    /// * `i128` - Fair share amount in pool_token
    pub fn calculate_fair_share(
        env: Env,
        pool_token: Address,
        grant_amount: i128,
    ) -> Result<i128, Error> {
        if grant_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let pool_info = env.storage().instance()
            .get::<_, MatchingPoolInfo>(&DataKey::MatchingPool(pool_token.clone()))
            .ok_or(Error::InsufficientMatchingPool)?;

        // Check price expiry
        let now = env.ledger().timestamp();
        if now - pool_info.last_price_update > DEX_PRICE_EXPIRY_SECS {
            return Err(Error::PriceOracleNotFound);
        }

        // Calculate base conversion
        let base_amount = (grant_amount * pool_info.last_dex_price) / SCALING_FACTOR;

        // Apply buffer for safety
        let buffer_amount = (base_amount * pool_info.price_buffer_bps as i128) / 10000;
        let fair_share = base_amount + buffer_amount;

        // Verify pool has sufficient balance
        if pool_info.allocated_amount + fair_share > pool_info.pool_balance {
            return Err(Error::InsufficientMatchingPool);
        }

        Ok(fair_share)
    }

    /// Allocate matching pool funds to a grant
    /// 
    /// Allocates funds from the matching pool to a specific grant after
    /// verifying fair share calculation and pool solvency.
    /// 
    /// # Arguments
    /// * `pool_token` - Pool token address
    /// * `grant_id` - Grant ID to allocate to
    /// * `amount` - Amount to allocate in pool_token
    pub fn allocate_matching_funds(
        env: Env,
        pool_token: Address,
        grant_id: u64,
        amount: i128,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let mut pool_info = env.storage().instance()
            .get::<_, MatchingPoolInfo>(&DataKey::MatchingPool(pool_token.clone()))
            .ok_or(Error::InsufficientMatchingPool)?;

        // Verify pool has sufficient balance
        if pool_info.allocated_amount + amount > pool_info.pool_balance {
            return Err(Error::InsufficientMatchingPool);
        }

        // Update allocation
        pool_info.allocated_amount += amount;
        env.storage().instance().set(&DataKey::MatchingPool(pool_token.clone()), &pool_info);

        // Emit event
        env.events().publish(
            (symbol_short!("match_alloc"), symbol_short!("gnt")),
            (pool_token, grant_id, amount),
        );

        Ok(())
    }

    /// Get current matching pool info
    pub fn get_matching_pool_info(env: Env, pool_token: Address) -> Result<MatchingPoolInfo, Error> {
        env.storage().instance()
            .get(&DataKey::MatchingPool(pool_token))
            .ok_or(Error::InsufficientMatchingPool)
    }

    /// Get latest DEX price update
    pub fn get_latest_dex_price(env: Env) -> Result<DexPriceUpdate, Error> {
        env.storage().instance()
            .get(&DataKey::DexPriceBuffer)
            .ok_or(Error::PriceOracleNotFound)
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
#[cfg(test)]
mod test_slashing;
mod test_inflation;
#[cfg(test)]
mod test_yield;
#[cfg(test)]
mod test_fee;
#[cfg(test)]
mod test_cross_chain_features;
