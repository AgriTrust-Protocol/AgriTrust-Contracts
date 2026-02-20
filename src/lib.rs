#![no_std]
<<<<<<< HEAD
use soroban_sdk::{contract, contracttype, contractimpl, Address, Env, token};

#[contracttype]
#[derive(Clone)]
pub struct Grant {
    pub admin: Address,
    pub grantee: Address,
    pub flow_rate: i128,
    pub balance: i128,
    pub last_claim_time: u64,
    pub is_paused: bool,
    pub token: Address,
}

#[contracttype]
pub enum DataKey {
    Grant(u64),
    Count,
}

#[contract]
pub struct GrantContract;

#[contractimpl]
impl GrantContract {
    pub fn create_grant(env: Env, admin: Address, grantee: Address, deposit: i128, flow_rate: i128, token: Address) -> u64 {
        admin.require_auth();
        let mut count: u64 = env.storage().instance().get(&DataKey::Count).unwrap_or(0);
        count += 1;
        let client = token::Client::new(&env, &token);
        client.transfer(&admin, &env.current_contract_address(), &deposit);
        let grant = Grant { admin, grantee, flow_rate, balance: deposit, last_claim_time: env.ledger().timestamp(), is_paused: false, token };
        env.storage().instance().set(&DataKey::Grant(count), &grant);
        env.storage().instance().set(&DataKey::Count, &count);
        count
    }

    pub fn withdraw(env: Env, grant_id: u64) {
        let mut grant: Grant = env.storage().instance().get(&DataKey::Grant(grant_id)).unwrap();
        grant.grantee.require_auth();
        if grant.is_paused { panic!("Grant PAUSED by admin"); }
        let current_time = env.ledger().timestamp();
        let seconds_passed = current_time - grant.last_claim_time;
        let amount_due = grant.flow_rate * seconds_passed as i128;
        let payout = if grant.balance >= amount_due { amount_due } else { grant.balance };
        if payout > 0 {
            let client = token::Client::new(&env, &grant.token);
            client.transfer(&env.current_contract_address(), &grant.grantee, &payout);
            grant.balance -= payout;
            grant.last_claim_time = current_time;
            env.storage().instance().set(&DataKey::Grant(grant_id), &grant);
        }
    }

    pub fn set_pause(env: Env, grant_id: u64, pause_state: bool) {
        let mut grant: Grant = env.storage().instance().get(&DataKey::Grant(grant_id)).unwrap();
        grant.admin.require_auth();
        grant.is_paused = pause_state;
        env.storage().instance().set(&DataKey::Grant(grant_id), &grant);
    }
}
=======
use soroban_sdk::{
    contract, contractimpl, vec, Env, String, Vec, Map, Symbol, Address, 
    token, IntoVal, TryFromVal, try_from_val, ConversionError
};

#[contract]
pub struct VestingContract;

// Storage keys for efficient access
const VAULT_COUNT: Symbol = Symbol::new(&"VAULT_COUNT");
const VAULT_DATA: Symbol = Symbol::new(&"VAULT_DATA");
const USER_VAULTS: Symbol = Symbol::new(&"USER_VAULTS");

// Vault structure with lazy initialization
#[contracttype]
pub struct Vault {
    pub owner: Address,
    pub total_amount: i128,
    pub released_amount: i128,
    pub start_time: u64,
    pub end_time: u64,
    pub is_initialized: bool, // Lazy initialization flag
}

#[contracttype]
pub struct BatchCreateData {
    pub recipients: Vec<Address>,
    pub amounts: Vec<i128>,
    pub start_times: Vec<u64>,
    pub end_times: Vec<u64>,
}

#[contractimpl]
impl VestingContract {
    // Full initialization - writes all metadata immediately
    pub fn create_vault_full(env: Env, owner: Address, amount: i128, start_time: u64, end_time: u64) -> u64 {
        // Get next vault ID
        let mut vault_count: u64 = env.storage().instance().get(&VAULT_COUNT).unwrap_or(0);
        vault_count += 1;
        
        // Create vault with full initialization
        let vault = Vault {
            owner: owner.clone(),
            total_amount: amount,
            released_amount: 0,
            start_time,
            end_time,
            is_initialized: true, // Mark as fully initialized
        };
        
        // Store vault data immediately (expensive gas usage)
        env.storage().instance().set(&VAULT_DATA, &vault_count, &vault);
        
        // Update user vaults list
        let mut user_vaults: Vec<u64> = env.storage().instance()
            .get(&USER_VAULTS, &owner)
            .unwrap_or(Vec::new(&env));
        user_vaults.push_back(vault_count);
        env.storage().instance().set(&USER_VAULTS, &owner, &user_vaults);
        
        // Update vault count
        env.storage().instance().set(&VAULT_COUNT, &vault_count);
        
        vault_count
    }
    
    // Lazy initialization - writes minimal data initially
    pub fn create_vault_lazy(env: Env, owner: Address, amount: i128, start_time: u64, end_time: u64) -> u64 {
        // Get next vault ID
        let mut vault_count: u64 = env.storage().instance().get(&VAULT_COUNT).unwrap_or(0);
        vault_count += 1;
        
        // Create vault with lazy initialization (minimal storage)
        let vault = Vault {
            owner: owner.clone(),
            total_amount: amount,
            released_amount: 0,
            start_time,
            end_time,
            is_initialized: false, // Mark as lazy initialized
        };
        
        // Store only essential data initially (cheaper gas)
        env.storage().instance().set(&VAULT_DATA, &vault_count, &vault);
        
        // Update vault count
        env.storage().instance().set(&VAULT_COUNT, &vault_count);
        
        // Don't update user vaults list yet (lazy)
        
        vault_count
    }
    
    // Initialize vault metadata when needed (on-demand)
    pub fn initialize_vault_metadata(env: Env, vault_id: u64) -> bool {
        let vault: Vault = env.storage().instance()
            .get(&VAULT_DATA, &vault_id)
            .unwrap_or_else(|| {
                // Return empty vault if not found
                Vault {
                    owner: Address::from_contract_id(&env.current_contract_address()),
                    total_amount: 0,
                    released_amount: 0,
                    start_time: 0,
                    end_time: 0,
                    is_initialized: false,
                }
            });
        
        // Only initialize if not already initialized
        if !vault.is_initialized {
            let mut updated_vault = vault.clone();
            updated_vault.is_initialized = true;
            
            // Store updated vault with full metadata
            env.storage().instance().set(&VAULT_DATA, &vault_id, &updated_vault);
            
            // Update user vaults list (deferred)
            let mut user_vaults: Vec<u64> = env.storage().instance()
                .get(&USER_VAULTS, &updated_vault.owner)
                .unwrap_or(Vec::new(&env));
            user_vaults.push_back(vault_id);
            env.storage().instance().set(&USER_VAULTS, &updated_vault.owner, &user_vaults);
            
            true
        } else {
            false // Already initialized
        }
    }
    
    // Batch create vaults with lazy initialization
    pub fn batch_create_vaults_lazy(env: Env, batch_data: BatchCreateData) -> Vec<u64> {
        let mut vault_ids = Vec::new(&env);
        let initial_count: u64 = env.storage().instance().get(&VAULT_COUNT).unwrap_or(0);
        
        for i in 0..batch_data.recipients.len() {
            let vault_id = initial_count + i as u64 + 1;
            
            // Create vault with lazy initialization
            let vault = Vault {
                owner: batch_data.recipients.get(i).unwrap(),
                total_amount: batch_data.amounts.get(i).unwrap(),
                released_amount: 0,
                start_time: batch_data.start_times.get(i).unwrap(),
                end_time: batch_data.end_times.get(i).unwrap(),
                is_initialized: false, // Lazy initialization
            };
            
            // Store vault data (minimal writes)
            env.storage().instance().set(&VAULT_DATA, &vault_id, &vault);
            vault_ids.push_back(vault_id);
        }
        
        // Update vault count once (cheaper than individual updates)
        let final_count = initial_count + batch_data.recipients.len() as u64;
        env.storage().instance().set(&VAULT_COUNT, &final_count);
        
        vault_ids
    }
    
    // Batch create vaults with full initialization
    pub fn batch_create_vaults_full(env: Env, batch_data: BatchCreateData) -> Vec<u64> {
        let mut vault_ids = Vec::new(&env);
        let initial_count: u64 = env.storage().instance().get(&VAULT_COUNT).unwrap_or(0);
        
        for i in 0..batch_data.recipients.len() {
            let vault_id = initial_count + i as u64 + 1;
            
            // Create vault with full initialization
            let vault = Vault {
                owner: batch_data.recipients.get(i).unwrap(),
                total_amount: batch_data.amounts.get(i).unwrap(),
                released_amount: 0,
                start_time: batch_data.start_times.get(i).unwrap(),
                end_time: batch_data.end_times.get(i).unwrap(),
                is_initialized: true, // Full initialization
            };
            
            // Store vault data (expensive writes)
            env.storage().instance().set(&VAULT_DATA, &vault_id, &vault);
            
            // Update user vaults list for each vault (expensive)
            let mut user_vaults: Vec<u64> = env.storage().instance()
                .get(&USER_VAULTS, &vault.owner)
                .unwrap_or(Vec::new(&env));
            user_vaults.push_back(vault_id);
            env.storage().instance().set(&USER_VAULTS, &vault.owner, &user_vaults);
            
            vault_ids.push_back(vault_id);
        }
        
        // Update vault count once
        let final_count = initial_count + batch_data.recipients.len() as u64;
        env.storage().instance().set(&VAULT_COUNT, &final_count);
        
        vault_ids
    }
    
    // Get vault info (initializes if needed)
    pub fn get_vault(env: Env, vault_id: u64) -> Vault {
        let vault: Vault = env.storage().instance()
            .get(&VAULT_DATA, &vault_id)
            .unwrap_or_else(|| {
                Vault {
                    owner: Address::from_contract_id(&env.current_contract_address()),
                    total_amount: 0,
                    released_amount: 0,
                    start_time: 0,
                    end_time: 0,
                    is_initialized: false,
                }
            });
        
        // Auto-initialize if lazy
        if !vault.is_initialized {
            Self::initialize_vault_metadata(env, vault_id);
            // Get updated vault
            env.storage().instance().get(&VAULT_DATA, &vault_id).unwrap()
        } else {
            vault
        }
    }
    
    // Get user vaults (initializes all if needed)
    pub fn get_user_vaults(env: Env, user: Address) -> Vec<u64> {
        let vault_ids: Vec<u64> = env.storage().instance()
            .get(&USER_VAULTS, &user)
            .unwrap_or(Vec::new(&env));
        
        // Initialize all lazy vaults for this user
        for vault_id in vault_ids.iter() {
            let vault: Vault = env.storage().instance()
                .get(&VAULT_DATA, vault_id)
                .unwrap_or_else(|| {
                    Vault {
                        owner: user.clone(),
                        total_amount: 0,
                        released_amount: 0,
                        start_time: 0,
                        end_time: 0,
                        is_initialized: false,
                    }
                });
            
            if !vault.is_initialized {
                Self::initialize_vault_metadata(env, *vault_id);
            }
        }
        
        vault_ids
    }
}

mod test;
>>>>>>> ce7eb66fb1d747f6801e3ee7512cf41d0eff7c70
