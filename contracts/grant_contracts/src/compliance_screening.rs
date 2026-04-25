use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, Map, Symbol, Vec, String,
};

// --- Compliance Screening Constants ---
pub const SCREENING_CACHE_DURATION: u64 = 300; // 5 minutes cache
pub const MAX_REASON_LENGTH: u32 = 500;

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct SanctionsRegistry {
    pub registry_address: Address,
    pub last_updated: u64,
    pub version: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ScreeningResult {
    pub address: Address,
    pub is_sanctioned: bool,
    pub reason: Option<String>,
    pub timestamp: u64,
    pub registry_version: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ExemptionList {
    pub addresses: Vec<Address>,
    pub last_updated: u64,
    pub admin: Address,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[contracterror]
pub enum ComplianceError {
    SanctionedAddress = 1,
    RegistryError = 2,
    InvalidAddress = 3,
    Unauthorized = 4,
    CacheExpired = 5,
    ScreeningFailed = 6,
}

/// Sanctions Registry Interface
pub trait SanctionsRegistryContract {
    fn is_sanctioned(env: &Env, address: Address) -> Result<bool, ComplianceError>;
    fn get_sanction_reason(env: &Env, address: Address) -> Result<Option<String>, ComplianceError>;
    fn get_registry_version(env: &Env) -> Result<u32, ComplianceError>;
}

/// Compliance Screening Hook for SEP-12
pub struct ComplianceHook;

#[contractimpl]
impl ComplianceHook {
    /// Initialize compliance screening system
    pub fn initialize_compliance(
        env: Env,
        admin: Address,
        registry_address: Address,
    ) -> Result<(), ComplianceError> {
        admin.require_auth();
        
        let registry = SanctionsRegistry {
            registry_address,
            last_updated: env.ledger().timestamp(),
            version: 1,
        };
        
        let registry_key = Symbol::new(&env, "sanctions_registry");
        env.storage().instance().set(&registry_key, registry);
        
        // Initialize empty exemption list
        let exemption_list = ExemptionList {
            addresses: Vec::new(&env),
            last_updated: env.ledger().timestamp(),
            admin,
        };
        
        let exemption_key = Symbol::new(&env, "exemption_list");
        env.storage().instance().set(&exemption_key, exemption_list);
        
        Ok(())
    }
    
    /// Check if grantee is eligible for stream initialization
    pub fn check_grantee_eligibility(env: Env, grantee: Address) -> Result<(), ComplianceError> {
        // Check exemptions first
        if Self::is_exempted(&env, grantee.clone())? {
            return Ok(());
        }
        
        // Perform sanctions screening
        let screening_result = Self::screen_address(&env, grantee)?;
        
        if screening_result.is_sanctioned {
            Err(ComplianceError::SanctionedAddress)
        } else {
            Ok(())
        }
    }
    
    /// Screen an address against sanctions registry
    pub fn screen_address(env: &Env, address: Address) -> Result<ScreeningResult, ComplianceError> {
        // Check cache first
        let cache_key = Symbol::new(env, &format!("screening_cache_{}", address));
        if let Some(cached_result) = env.storage().instance().get::<Symbol, ScreeningResult>(&cache_key) {
            // Check if cache is still valid
            if env.ledger().timestamp().saturating_sub(cached_result.timestamp) < SCREENING_CACHE_DURATION {
                return Ok(cached_result);
            }
        }
        
        // Query sanctions registry
        let registry = Self::get_sanctions_registry(env)?;
        let registry_contract = SanctionsRegistryClient::new(env, &registry.registry_address);
        
        let is_sanctioned = registry_contract.is_sanctioned(&address)?;
        let reason = registry_contract.get_sanction_reason(&address)?;
        let registry_version = registry_contract.get_registry_version()?;
        
        let result = ScreeningResult {
            address: address.clone(),
            is_sanctioned,
            reason,
            timestamp: env.ledger().timestamp(),
            registry_version,
        };
        
        // Cache the result
        env.storage().instance().set(&cache_key, result.clone());
        
        Ok(result)
    }
    
    /// Add address to exemption list (admin only)
    pub fn add_exemption(env: Env, admin: Address, address: Address) -> Result<(), ComplianceError> {
        admin.require_auth();
        
        let exemption_key = Symbol::new(&env, "exemption_list");
        let mut exemption_list = Self::get_exemption_list(&env)?;
        
        // Verify caller is admin
        if exemption_list.admin != admin {
            return Err(ComplianceError::Unauthorized);
        }
        
        // Check if already exempted
        for addr in exemption_list.addresses.iter() {
            if addr == address {
                return Ok(()); // Already exempted
            }
        }
        
        exemption_list.addresses.push_back(address);
        exemption_list.last_updated = env.ledger().timestamp();
        
        env.storage().instance().set(&exemption_key, exemption_list);
        
        Ok(())
    }
    
    /// Remove address from exemption list (admin only)
    pub fn remove_exemption(env: Env, admin: Address, address: Address) -> Result<(), ComplianceError> {
        admin.require_auth();
        
        let exemption_key = Symbol::new(&env, "exemption_list");
        let mut exemption_list = Self::get_exemption_list(&env)?;
        
        // Verify caller is admin
        if exemption_list.admin != admin {
            return Err(ComplianceError::Unauthorized);
        }
        
        // Find and remove address
        let mut found = false;
        let mut new_addresses = Vec::new(&env);
        
        for addr in exemption_list.addresses.iter() {
            if addr != address {
                new_addresses.push_back(addr);
            } else {
                found = true;
            }
        }
        
        if !found {
            return Ok(()); // Address not in exemption list
        }
        
        exemption_list.addresses = new_addresses;
        exemption_list.last_updated = env.ledger().timestamp();
        
        env.storage().instance().set(&exemption_key, exemption_list);
        
        Ok(())
    }
    
    /// Update sanctions registry address (admin only)
    pub fn update_registry(env: Env, admin: Address, new_registry: Address) -> Result<(), ComplianceError> {
        admin.require_auth();
        
        let registry_key = Symbol::new(&env, "sanctions_registry");
        let mut registry = Self::get_sanctions_registry(&env)?;
        
        // Verify current admin
        let exemption_list = Self::get_exemption_list(&env)?;
        if exemption_list.admin != admin {
            return Err(ComplianceError::Unauthorized);
        }
        
        registry.registry_address = new_registry;
        registry.last_updated = env.ledger().timestamp();
        registry.version += 1;
        
        env.storage().instance().set(&registry_key, registry);
        
        // Clear all screening caches since registry changed
        Self::clear_screening_cache(&env)?;
        
        Ok(())
    }
    
    /// Verify identity with SEP-12 (optional additional check)
    pub fn verify_identity_sep12(env: &Env, address: Address) -> Result<bool, ComplianceError> {
        // In a real implementation, this would call SEP-12 identity verification contract
        // For now, we'll return true if the address passes sanctions screening
        let screening_result = Self::screen_address(env, address)?;
        
        Ok(!screening_result.is_sanctioned)
    }
    
    /// Get screening statistics (admin only)
    pub fn get_screening_stats(env: Env, admin: Address) -> Result<ScreeningStats, ComplianceError> {
        admin.require_auth();
        
        let exemption_list = Self::get_exemption_list(&env)?;
        if exemption_list.admin != admin {
            return Err(ComplianceError::Unauthorized);
        }
        
        let stats_key = Symbol::new(&env, "screening_stats");
        
        if let Some(stats) = env.storage().instance().get::<Symbol, ScreeningStats>(&stats_key) {
            Ok(stats)
        } else {
            Ok(ScreeningStats {
                total_screened: 0,
                sanctioned_blocked: 0,
                exemptions_granted: exemption_list.addresses.len(),
                last_updated: env.ledger().timestamp(),
            })
        }
    }
    
    /// Clear screening cache
    fn clear_screening_cache(env: &Env) -> Result<(), ComplianceError> {
        let cache_prefix = "screening_cache_";
        
        // In a real implementation, this would iterate through all cache keys
        // For now, we'll store a cache version to invalidate all entries
        let cache_version_key = Symbol::new(env, "cache_version");
        let current_version = env.storage().instance().get(&cache_version_key).unwrap_or(0u32);
        env.storage().instance().set(&cache_version_key, current_version + 1);
        
        Ok(())
    }
    
    /// Check if address is exempted
    fn is_exempted(env: &Env, address: Address) -> Result<bool, ComplianceError> {
        let exemption_list = Self::get_exemption_list(env)?;
        
        for addr in exemption_list.addresses.iter() {
            if addr == address {
                return Ok(true);
            }
        }
        
        Ok(false)
    }
    
    /// Get sanctions registry configuration
    fn get_sanctions_registry(env: &Env) -> Result<SanctionsRegistry, ComplianceError> {
        let registry_key = Symbol::new(env, "sanctions_registry");
        env.storage().instance()
            .get(&registry_key)
            .ok_or(ComplianceError::RegistryError)
    }
    
    /// Get exemption list
    fn get_exemption_list(env: &Env) -> Result<ExemptionList, ComplianceError> {
        let exemption_key = Symbol::new(env, "exemption_list");
        env.storage().instance()
            .get(&exemption_key)
            .ok_or(ComplianceError::RegistryError)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ScreeningStats {
    pub total_screened: u64,
    pub sanctioned_blocked: u64,
    pub exemptions_granted: u32,
    pub last_updated: u64,
}

/// Mock Sanctions Registry Client for testing
pub struct SanctionsRegistryClient<'a> {
    env: &'a Env,
    contract_id: &'a Address,
}

impl<'a> SanctionsRegistryClient<'a> {
    pub fn new(env: &'a Env, contract_id: &'a Address) -> Self {
        Self { env, contract_id }
    }
    
    pub fn is_sanctioned(&self, address: &Address) -> Result<bool, ComplianceError> {
        // In a real implementation, this would make a cross-contract call
        // For now, we'll simulate with storage
        let sanctioned_key = Symbol::new(self.env, &format!("sanctioned_{}", address));
        Ok(self.env.storage().instance().get(&sanctioned_key).unwrap_or(false))
    }
    
    pub fn get_sanction_reason(&self, address: &Address) -> Result<Option<String>, ComplianceError> {
        // In a real implementation, this would make a cross-contract call
        let reason_key = Symbol::new(self.env, &format!("sanction_reason_{}", address));
        Ok(self.env.storage().instance().get(&reason_key))
    }
    
    pub fn get_registry_version(&self) -> Result<u32, ComplianceError> {
        // In a real implementation, this would make a cross-contract call
        let version_key = Symbol::new(self.env, "registry_version");
        Ok(self.env.storage().instance().get(&version_key).unwrap_or(1))
    }
    
    /// Add sanctioned address (for testing/admin purposes)
    pub fn add_sanctioned_address(&self, admin: &Address, address: Address, reason: String) -> Result<(), ComplianceError> {
        admin.require_auth();
        
        let sanctioned_key = Symbol::new(self.env, &format!("sanctioned_{}", address));
        let reason_key = Symbol::new(self.env, &format!("sanction_reason_{}", address));
        
        self.env.storage().instance().set(&sanctioned_key, true);
        self.env.storage().instance().set(&reason_key, reason);
        
        Ok(())
    }
    
    /// Remove sanctioned address (for testing/admin purposes)
    pub fn remove_sanctioned_address(&self, admin: &Address, address: Address) -> Result<(), ComplianceError> {
        admin.require_auth();
        
        let sanctioned_key = Symbol::new(self.env, &format!("sanctioned_{}", address));
        let reason_key = Symbol::new(self.env, &format!("sanction_reason_{}", address));
        
        self.env.storage().instance().remove(&sanctioned_key);
        self.env.storage().instance().remove(&reason_key);
        
        Ok(())
    }
}
