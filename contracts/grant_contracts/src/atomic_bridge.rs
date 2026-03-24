#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Vec,
    Symbol, Map, String,
};

use crate::grant_contracts::{GrantContract, GranteeConfig, Error as GrantError};

/// Atomic Bridge Contract
/// 
/// This contract acts as a bridge between Governance and Grant-Stream contracts.
/// It enables atomic execution of grant creation immediately when a proposal passes,
/// eliminating the delay and human intervention required in the current system.
/// 
/// Key Features:
/// - Direct grant creation from governance proposals
/// - Atomic execution (all or nothing)
/// - Multi-asset support
/// - Validation and error handling
/// - Event emission for transparency

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum BridgeStatus {
    Active,
    Paused,
    Emergency,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct BridgeConfig {
    pub governance_contract: Address,
    pub grant_contract: Address,
    pub status: BridgeStatus,
    pub max_grants_per_proposal: u32,
    pub max_total_amount: i128,
    pub authorized_callers: Vec<Address>,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct AtomicGrantPayload {
    pub proposal_id: u64,
    pub grant_configs: Vec<GranteeConfig>,
    pub total_amount: i128,
    pub asset_addresses: Vec<Address>,
    pub execution_timestamp: u64,
}

#[derive(Clone)]
#[contracttype]
pub enum BridgeDataKey {
    Config,
    Payload(u64), // proposal_id -> payload
    ExecutionLog(u64), // proposal_id -> execution result
    AuthorizedCallers,
    NextExecutionId,
}

#[contracterror]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum BridgeError {
    NotInitialized = 1001,
    AlreadyInitialized = 1002,
    NotAuthorized = 1003,
    InvalidConfig = 1004,
    BridgePaused = 1005,
    InvalidPayload = 1006,
    TooManyGrants = 1007,
    AmountTooLarge = 1008,
    ExecutionFailed = 1009,
    ProposalNotFound = 1010,
    AlreadyExecuted = 1011,
    GrantContractError = 1012,
    InvalidAsset = 1013,
    MathOverflow = 1014,
}

#[contract]
pub struct AtomicBridge;

#[contractimpl]
impl AtomicBridge {
    /// Initialize the atomic bridge contract
    pub fn initialize(
        env: Env,
        governance_contract: Address,
        grant_contract: Address,
        max_grants_per_proposal: u32,
        max_total_amount: i128,
        initial_authorized_callers: Vec<Address>,
    ) -> Result<(), BridgeError> {
        if env.storage().instance().has(&BridgeDataKey::Config) {
            return Err(BridgeError::AlreadyInitialized);
        }

        // Validate inputs
        if max_grants_per_proposal == 0 || max_total_amount <= 0 {
            return Err(BridgeError::InvalidConfig);
        }

        let config = BridgeConfig {
            governance_contract: governance_contract.clone(),
            grant_contract: grant_contract.clone(),
            status: BridgeStatus::Active,
            max_grants_per_proposal,
            max_total_amount,
            authorized_callers: initial_authorized_callers.clone(),
        };

        env.storage().instance().set(&BridgeDataKey::Config, &config);
        env.storage().instance().set(&BridgeDataKey::AuthorizedCallers, &initial_authorized_callers);
        env.storage().instance().set(&BridgeDataKey::NextExecutionId, &1u64);

        // Emit initialization event
        env.events().publish(
            (symbol_short!("bridge_init"),),
            (governance_contract, grant_contract, max_grants_per_proposal, max_total_amount),
        );

        Ok(())
    }

    /// Execute atomic grant creation from governance proposal
    /// This is the core function that enables the atomic bridge
    pub fn execute_atomic_grants(
        env: Env,
        caller: Address,
        proposal_id: u64,
        grant_configs: Vec<GranteeConfig>,
        total_amount: i128,
    ) -> Result<Vec<u64>, BridgeError> {
        // Check authorization
        Self::require_authorized_caller(&env, &caller)?;

        // Check bridge status
        let config = Self::get_config(&env)?;
        if config.status != BridgeStatus::Active {
            return Err(BridgeError::BridgePaused);
        }

        // Validate payload
        if grant_configs.len() > config.max_grants_per_proposal as usize {
            return Err(BridgeError::TooManyGrants);
        }

        if total_amount > config.max_total_amount {
            return Err(BridgeError::AmountTooLarge);
        }

        // Check if already executed
        if env.storage().instance().has(&BridgeDataKey::ExecutionLog(proposal_id)) {
            return Err(BridgeError::AlreadyExecuted);
        }

        // Store payload for audit trail
        let payload = AtomicGrantPayload {
            proposal_id,
            grant_configs: grant_configs.clone(),
            total_amount,
            asset_addresses: Self::extract_unique_assets(&grant_configs),
            execution_timestamp: env.ledger().timestamp(),
        };
        env.storage().instance().set(&BridgeDataKey::Payload(proposal_id), &payload);

        // Execute atomic grant creation
        let mut successful_grants = Vec::new(&env);
        let mut failed_count = 0u32;

        // Get next grant ID
        let next_grant_id = Self::get_next_grant_id(&env)?;

        // Call grant contract to create grants atomically
        match Self::call_grant_contract_batch_init(&env, &grant_configs, next_grant_id) {
            Ok(result) => {
                successful_grants = result.successful_grants;
                failed_count = result.failed_grants.len() as u32;

                // Log execution result
                let execution_log = ExecutionResult {
                    proposal_id,
                    success: true,
                    grants_created: successful_grants.len() as u32,
                    failed_grants: failed_count,
                    total_amount,
                    executed_at: env.ledger().timestamp(),
                    error_message: String::from_str(&env, ""),
                };

                env.storage().instance().set(&BridgeDataKey::ExecutionLog(proposal_id), &execution_log);

                // Emit success event
                env.events().publish(
                    (symbol_short!("atomic_exec"), proposal_id),
                    (successful_grants.len(), failed_count, total_amount),
                );

                Ok(successful_grants)
            }
            Err(e) => {
                // Log failure
                let execution_log = ExecutionResult {
                    proposal_id,
                    success: false,
                    grants_created: 0,
                    failed_grants: grant_configs.len() as u32,
                    total_amount,
                    executed_at: env.ledger().timestamp(),
                    error_message: String::from_str(&env, &format!("Grant contract error: {:?}", e)),
                };

                env.storage().instance().set(&BridgeDataKey::ExecutionLog(proposal_id), &execution_log);

                // Emit failure event
                env.events().publish(
                    (symbol_short!("atomic_fail"), proposal_id),
                    (grant_configs.len(), total_amount),
                );

                Err(BridgeError::ExecutionFailed)
            }
        }
    }

    /// Emergency pause function
    pub fn pause_bridge(env: Env, caller: Address) -> Result<(), BridgeError> {
        Self::require_authorized_caller(&env, &caller)?;
        
        let mut config = Self::get_config(&env)?;
        if config.status == BridgeStatus::Emergency {
            return Err(BridgeError::BridgePaused);
        }

        config.status = BridgeStatus::Paused;
        env.storage().instance().set(&BridgeDataKey::Config, &config);

        env.events().publish((symbol_short!("bridge_pause"),), caller);
        Ok(())
    }

    /// Resume bridge operations
    pub fn resume_bridge(env: Env, caller: Address) -> Result<(), BridgeError> {
        Self::require_authorized_caller(&env, &caller)?;
        
        let mut config = Self::get_config(&env)?;
        if config.status != BridgeStatus::Paused {
            return Err(BridgeError::InvalidConfig);
        }

        config.status = BridgeStatus::Active;
        env.storage().instance().set(&BridgeDataKey::Config, &config);

        env.events().publish((symbol_short!("bridge_resume"),), caller);
        Ok(())
    }

    /// Add authorized caller
    pub fn add_authorized_caller(env: Env, caller: Address, new_caller: Address) -> Result<(), BridgeError> {
        Self::require_authorized_caller(&env, &caller)?;
        
        let mut authorized = Self::get_authorized_callers(&env)?;
        
        // Check if already authorized
        for existing in authorized.iter() {
            if existing == new_caller {
                return Err(BridgeError::NotAuthorized); // Already exists
            }
        }

        authorized.push_back(new_caller.clone());
        env.storage().instance().set(&BridgeDataKey::AuthorizedCallers, &authorized);

        // Update config
        let mut config = Self::get_config(&env)?;
        config.authorized_callers = authorized.clone();
        env.storage().instance().set(&BridgeDataKey::Config, &config);

        env.events().publish((symbol_short!("caller_added"),), (caller, new_caller));
        Ok(())
    }

    /// Remove authorized caller
    pub fn remove_authorized_caller(env: Env, caller: Address, target_caller: Address) -> Result<(), BridgeError> {
        Self::require_authorized_caller(&env, &caller)?;
        
        let mut authorized = Self::get_authorized_callers(&env)?;
        let mut found = false;

        // Remove the target caller
        let mut new_authorized = Vec::new(&env);
        for existing in authorized.iter() {
            if existing == target_caller {
                found = true;
            } else {
                new_authorized.push_back(existing);
            }
        }

        if !found {
            return Err(BridgeError::NotAuthorized);
        }

        env.storage().instance().set(&BridgeDataKey::AuthorizedCallers, &new_authorized);

        // Update config
        let mut config = Self::get_config(&env)?;
        config.authorized_callers = new_authorized.clone();
        env.storage().instance().set(&BridgeDataKey::Config, &config);

        env.events().publish((symbol_short!("caller_removed"),), (caller, target_caller));
        Ok(())
    }

    // View functions

    /// Get bridge configuration
    pub fn get_config(env: Env) -> Result<BridgeConfig, BridgeError> {
        env.storage()
            .instance()
            .get(&BridgeDataKey::Config)
            .ok_or(BridgeError::NotInitialized)
    }

    /// Get execution log for a proposal
    pub fn get_execution_log(env: Env, proposal_id: u64) -> Result<ExecutionResult, BridgeError> {
        env.storage()
            .instance()
            .get(&BridgeDataKey::ExecutionLog(proposal_id))
            .ok_or(BridgeError::ProposalNotFound)
    }

    /// Get payload for a proposal
    pub fn get_payload(env: Env, proposal_id: u64) -> Result<AtomicGrantPayload, BridgeError> {
        env.storage()
            .instance()
            .get(&BridgeDataKey::Payload(proposal_id))
            .ok_or(BridgeError::ProposalNotFound)
    }

    /// Get all authorized callers
    pub fn get_authorized_callers(env: Env) -> Result<Vec<Address>, BridgeError> {
        env.storage()
            .instance()
            .get(&BridgeDataKey::AuthorizedCallers)
            .ok_or(BridgeError::NotInitialized)
    }

    // Private helper functions

    fn require_authorized_caller(env: &Env, caller: &Address) -> Result<(), BridgeError> {
        caller.require_auth();
        
        let authorized = Self::get_authorized_callers(env)?;
        
        for auth_caller in authorized.iter() {
            if auth_caller == *caller {
                return Ok(());
            }
        }
        
        Err(BridgeError::NotAuthorized)
    }

    fn extract_unique_assets(grant_configs: &Vec<GranteeConfig>) -> Vec<Address> {
        let mut unique_assets = Vec::new(&grant_configs.env);
        let mut seen = Vec::new(&grant_configs.env);

        for config in grant_configs.iter() {
            let mut already_seen = false;
            for asset in seen.iter() {
                if *asset == config.asset {
                    already_seen = true;
                    break;
                }
            }
            
            if !already_seen {
                unique_assets.push_back(config.asset.clone());
                seen.push_back(config.asset.clone());
            }
        }

        unique_assets
    }

    fn get_next_grant_id(env: &Env) -> Result<u64, BridgeError> {
        // This would typically query the grant contract for the next available ID
        // For now, we'll use a simple counter
        let next_id = env
            .storage()
            .instance()
            .get(&BridgeDataKey::NextExecutionId)
            .unwrap_or(1u64);
        
        env.storage().instance().set(&BridgeDataKey::NextExecutionId, &(next_id + 1000));
        Ok(next_id)
    }

    fn call_grant_contract_batch_init(
        env: &Env,
        grant_configs: &Vec<GranteeConfig>,
        starting_grant_id: u64,
    ) -> Result<BatchInitResult, BridgeError> {
        let config = Self::get_config(env)?;
        
        // Create grant contract client
        let grant_contract_client = GrantContractClient::new(env, &config.grant_contract);
        
        // Call batch_init on grant contract
        match grant_contract_client.batch_init(&grant_configs.clone(), &starting_grant_id) {
            Ok(result) => Ok(result),
            Err(_) => Err(BridgeError::GrantContractError),
        }
    }
}

// Supporting types

#[derive(Clone, Debug)]
#[contracttype]
pub struct ExecutionResult {
    pub proposal_id: u64,
    pub success: bool,
    pub grants_created: u32,
    pub failed_grants: u32,
    pub total_amount: i128,
    pub executed_at: u64,
    pub error_message: String,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchInitResult {
    pub successful_grants: Vec<u64>,
    pub failed_grants: Vec<u64>,
    pub total_deposited: i128,
    pub grants_created: u32,
}

// Grant contract client (stub - would be generated)
pub struct GrantContractClient {
    env: Env,
    contract_id: Address,
}

impl GrantContractClient {
    pub fn new(env: &Env, contract_id: &Address) -> Self {
        Self {
            env: env.clone(),
            contract_id: contract_id.clone(),
        }
    }

    pub fn batch_init(
        &self,
        grantee_configs: &Vec<GranteeConfig>,
        starting_grant_id: &u64,
    ) -> Result<BatchInitResult, GrantError> {
        // This would be a proper contract call in the actual implementation
        // For now, returning a stub result
        Ok(BatchInitResult {
            successful_grants: Vec::new(&self.env),
            failed_grants: Vec::new(&self.env),
            total_deposited: 0,
            grants_created: 0,
        })
    }
}
