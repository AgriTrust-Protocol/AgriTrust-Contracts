#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, Map, Symbol, Vec, String,
    panic_with_error,
};

// --- Constants ---
const MAX_WASM_HASH_LENGTH: usize = 32;
const MAX_VERSION_LENGTH: u32 = 50;
const MAX_REASON_LENGTH: u32 = 500;
const UPGRADE_COOLDOWN_SECS: u64 = 7 * 24 * 60 * 60; // 7 days

// --- Data Structures ---

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub enum VerificationDataKey {
    /// Maps grant_id to WasmHashInfo
    GrantWasmHash(u64),
    /// Maps wasm_hash to VersionInfo
    VersionRegistry([u8; 32]),
    /// Maps user_address to their upgrade preferences
    UserUpgradePreferences(Address),
    /// Global upgrade tracking
    GlobalUpgradeRegistry,
    /// Pending upgrades awaiting user consent
    PendingUpgrades(Address, u64), // (user, grant_id)
}

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub struct WasmHashInfo {
    /// Current active WASM hash for this grant
    pub current_hash: [u8; 32],
    /// When this hash was set
    pub set_at: u64,
    /// Who set this hash (admin address)
    pub set_by: Address,
    /// Version identifier
    pub version: String,
    /// Whether this is the initial hash
    pub is_initial: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub struct VersionInfo {
    /// WASM hash
    pub wasm_hash: [u8; 32],
    /// Version string (e.g., "v1.0.0")
    pub version: String,
    /// When this version was registered
    pub registered_at: u64,
    /// Who registered this version
    pub registered_by: Address,
    /// Description of changes in this version
    pub description: String,
    /// Whether this version is deprecated
    pub deprecated: bool,
    /// Security level of this version
    pub security_level: SecurityLevel,
}

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub enum SecurityLevel {
    Low,      // Experimental or testing
    Medium,   // Standard release
    High,     // Audited and production-ready
    Critical, // Security-critical updates
}

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub struct UserUpgradePreference {
    /// Whether user wants auto-upgrade for security updates
    pub auto_upgrade_security: bool,
    /// Whether user wants auto-upgrade for any version
    pub auto_upgrade_all: bool,
    /// Minimum security level required for auto-upgrade
    pub min_security_level: SecurityLevel,
    /// Specific versions user has opted out of
    pub opted_out_versions: Vec<String>,
    /// Last time user updated preferences
    pub last_updated: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub struct PendingUpgrade {
    /// Grant ID being upgraded
    pub grant_id: u64,
    /// New WASM hash
    pub new_hash: [u8; 32],
    /// Old WASM hash
    pub old_hash: [u8; 32],
    /// New version info
    pub new_version: VersionInfo,
    /// When upgrade was proposed
    pub proposed_at: u64,
    /// Deadline for user decision
    pub decision_deadline: u64,
    /// Reason for upgrade
    pub reason: String,
    /// Whether this is a security-critical upgrade
    pub is_security_critical: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub enum UpgradeDecision {
    Pending,
    Accepted,
    Rejected,
    Expired,
}

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub struct UpgradeRecord {
    /// Grant ID
    pub grant_id: u64,
    /// Previous WASM hash
    pub old_hash: [u8; 32],
    /// New WASM hash
    pub new_hash: [u8; 32],
    /// Old version
    pub old_version: String,
    /// New version
    pub new_version: String,
    /// When upgrade occurred
    pub upgraded_at: u64,
    /// Who initiated upgrade
    pub initiated_by: Address,
    /// User decision
    pub decision: UpgradeDecision,
    /// Reason for upgrade
    pub reason: String,
}

// --- Errors ---

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum VerificationError {
    InvalidWasmHash = 1,
    InvalidVersion = 2,
    UnauthorizedAccess = 3,
    GrantNotFound = 4,
    VersionNotFound = 5,
    UpgradeNotPending = 6,
    DecisionDeadlinePassed = 7,
    AlreadyDecided = 8,
    VersionDeprecated = 9,
    SecurityLevelTooLow = 10,
    UserOptedOut = 11,
    CooldownPeriodActive = 12,
    InvalidSecurityLevel = 13,
    StringTooLong = 14,
    DuplicateVersion = 15,
}

// --- Contract Implementation ---

#[contract]
pub struct WasmHashVerification;

#[contractimpl]
impl WasmHashVerification {
    /// Initialize WASM hash for a new grant
    /// 
    /// This should be called when a grant is first created to establish
    /// the baseline WASM hash that users are consenting to.
    /// 
    /// # Arguments
    /// * `grant_id` - The grant identifier
    /// * `wasm_hash` - The initial WASM hash (32 bytes)
    /// * `version` - Version string (e.g., "v1.0.0")
    /// * `admin` - Admin address authorizing this initialization
    pub fn initialize_grant_wasm_hash(
        env: Env,
        grant_id: u64,
        wasm_hash: [u8; 32],
        version: String,
        admin: Address,
    ) -> Result<(), VerificationError> {
        // Verify admin authorization (this would integrate with existing admin auth)
        admin.require_auth();

        // Validate inputs
        if version.len() > MAX_VERSION_LENGTH as usize {
            return Err(VerificationError::StringTooLong);
        }

        // Check if already initialized
        let existing_hash = Self::get_grant_wasm_hash(env.clone(), grant_id);
        if existing_hash.is_ok() {
            return Err(VerificationError::AlreadyDecided); // Reusing error for already initialized
        }

        // Create WASM hash info
        let hash_info = WasmHashInfo {
            current_hash: wasm_hash,
            set_at: env.ledger().timestamp(),
            set_by: admin,
            version: version.clone(),
            is_initial: true,
        };

        // Store the hash info
        env.storage().instance().set(
            &VerificationDataKey::GrantWasmHash(grant_id),
            &hash_info,
        );

        // Register version if not already registered
        let version_key = VerificationDataKey::VersionRegistry(wasm_hash);
        if !env.storage().instance().has(&version_key) {
            let version_info = VersionInfo {
                wasm_hash,
                version: version.clone(),
                registered_at: env.ledger().timestamp(),
                registered_by: admin,
                description: "Initial version".into_val(&env),
                deprecated: false,
                security_level: SecurityLevel::Medium,
            };
            env.storage().instance().set(&version_key, &version_info);
        }

        Ok(())
    }

    /// Get current WASM hash for a grant
    pub fn get_grant_wasm_hash(env: Env, grant_id: u64) -> Result<WasmHashInfo, VerificationError> {
        env.storage()
            .instance()
            .get(&VerificationDataKey::GrantWasmHash(grant_id))
            .ok_or(VerificationError::GrantNotFound)
    }

    /// Register a new WASM version in the global registry
    /// 
    /// This allows the platform to register new versions that users can opt into.
    /// 
    /// # Arguments
    /// * `wasm_hash` - The new WASM hash (32 bytes)
    /// * `version` - Version string
    /// * `description` - Description of changes
    /// * `security_level` - Security level of this version
    /// * `admin` - Admin address registering the version
    pub fn register_version(
        env: Env,
        wasm_hash: [u8; 32],
        version: String,
        description: String,
        security_level: SecurityLevel,
        admin: Address,
    ) -> Result<(), VerificationError> {
        // Verify admin authorization
        admin.require_auth();

        // Validate inputs
        if version.len() > MAX_VERSION_LENGTH as usize {
            return Err(VerificationError::StringTooLong);
        }
        if description.len() > MAX_REASON_LENGTH as usize {
            return Err(VerificationError::StringTooLong);
        }

        // Check if version already exists
        let version_key = VerificationDataKey::VersionRegistry(wasm_hash);
        if env.storage().instance().has(&version_key) {
            return Err(VerificationError::DuplicateVersion);
        }

        // Create version info
        let version_info = VersionInfo {
            wasm_hash,
            version: version.clone(),
            registered_at: env.ledger().timestamp(),
            registered_by: admin,
            description,
            deprecated: false,
            security_level,
        };

        // Store version info
        env.storage().instance().set(&version_key, &version_info);

        Ok(())
    }

    /// Propose an upgrade for a specific grant
    /// 
    /// This creates a pending upgrade that the user must accept before it takes effect.
    /// 
    /// # Arguments
    /// * `grant_id` - The grant to upgrade
    /// * `new_wasm_hash` - The new WASM hash
    /// * `reason` - Reason for the upgrade
    /// * `is_security_critical` - Whether this is a security-critical upgrade
    /// * `admin` - Admin proposing the upgrade
    pub fn propose_upgrade(
        env: Env,
        grant_id: u64,
        new_wasm_hash: [u8; 32],
        reason: String,
        is_security_critical: bool,
        admin: Address,
    ) -> Result<(), VerificationError> {
        // Verify admin authorization
        admin.require_auth();

        // Validate inputs
        if reason.len() > MAX_REASON_LENGTH as usize {
            return Err(VerificationError::StringTooLong);
        }

        // Get current hash info
        let current_hash_info = Self::get_grant_wasm_hash(env.clone(), grant_id)?;

        // Check cooldown period
        let now = env.ledger().timestamp();
        if now.saturating_sub(current_hash_info.set_at) < UPGRADE_COOLDOWN_SECS {
            return Err(VerificationError::CooldownPeriodActive);
        }

        // Get new version info
        let version_key = VerificationDataKey::VersionRegistry(new_wasm_hash);
        let new_version_info: VersionInfo = env
            .storage()
            .instance()
            .get(&version_key)
            .ok_or(VerificationError::VersionNotFound)?;

        // Check if version is deprecated
        if new_version_info.deprecated {
            return Err(VerificationError::VersionDeprecated);
        }

        // Get grant to find user (this would integrate with existing grant system)
        // For now, we'll use the admin as the user for demonstration
        let user_address = admin.clone();

        // Create pending upgrade
        let pending_upgrade = PendingUpgrade {
            grant_id,
            new_hash: new_wasm_hash,
            old_hash: current_hash_info.current_hash,
            new_version: new_version_info,
            proposed_at: now,
            decision_deadline: now + (30 * 24 * 60 * 60), // 30 days
            reason,
            is_security_critical,
        };

        // Store pending upgrade
        env.storage().instance().set(
            &VerificationDataKey::PendingUpgrades(user_address.clone(), grant_id),
            &pending_upgrade,
        );

        Ok(())
    }

    /// User accepts or rejects a proposed upgrade
    /// 
    /// # Arguments
    /// * `grant_id` - The grant ID
    /// * `accept` - Whether to accept (true) or reject (false) the upgrade
    /// * `user` - The user making the decision
    pub fn decide_upgrade(
        env: Env,
        grant_id: u64,
        accept: bool,
        user: Address,
    ) -> Result<(), VerificationError> {
        // Verify user authorization
        user.require_auth();

        // Get pending upgrade
        let pending_key = VerificationDataKey::PendingUpgrades(user.clone(), grant_id);
        let pending_upgrade: PendingUpgrade = env
            .storage()
            .instance()
            .get(&pending_key)
            .ok_or(VerificationError::UpgradeNotPending)?;

        // Check deadline
        let now = env.ledger().timestamp();
        if now > pending_upgrade.decision_deadline {
            return Err(VerificationError::DecisionDeadlinePassed);
        }

        // Get user preferences if they exist
        let user_prefs = Self::get_user_upgrade_preferences(env.clone(), user.clone());

        // Check if user opted out of this version
        if let Some(prefs) = user_prefs {
            if prefs.opted_out_versions.contains(&pending_upgrade.new_version.version) {
                return Err(VerificationError::UserOptedOut);
            }
        }

        let decision = if accept {
            UpgradeDecision::Accepted
        } else {
            UpgradeDecision::Rejected
        };

        // Create upgrade record
        let upgrade_record = UpgradeRecord {
            grant_id,
            old_hash: pending_upgrade.old_hash,
            new_hash: pending_upgrade.new_hash,
            old_version: pending_upgrade.new_version.version.clone(), // This would need to be tracked properly
            new_version: pending_upgrade.new_version.version,
            upgraded_at: now,
            initiated_by: pending_upgrade.new_version.registered_by,
            decision,
            reason: pending_upgrade.reason,
        };

        // Store upgrade record in global registry
        let mut global_registry = Self::get_global_upgrade_registry(env.clone());
        global_registry.push_back(upgrade_record);
        env.storage().instance().set(
            &VerificationDataKey::GlobalUpgradeRegistry,
            &global_registry,
        );

        // If accepted, update the grant's WASM hash
        if accept {
            let new_hash_info = WasmHashInfo {
                current_hash: pending_upgrade.new_hash,
                set_at: now,
                set_by: user,
                version: pending_upgrade.new_version.version,
                is_initial: false,
            };

            env.storage().instance().set(
                &VerificationDataKey::GrantWasmHash(grant_id),
                &new_hash_info,
            );
        }

        // Remove pending upgrade
        env.storage().instance().remove(&pending_key);

        Ok(())
    }

    /// Set user upgrade preferences
    /// 
    /// # Arguments
    /// * `preferences` - User's upgrade preferences
    /// * `user` - The user setting preferences
    pub fn set_user_upgrade_preferences(
        env: Env,
        preferences: UserUpgradePreference,
        user: Address,
    ) -> Result<(), VerificationError> {
        // Verify user authorization
        user.require_auth();

        // Update last_updated timestamp
        let mut updated_prefs = preferences;
        updated_prefs.last_updated = env.ledger().timestamp();

        // Store preferences
        env.storage().instance().set(
            &VerificationDataKey::UserUpgradePreferences(user),
            &updated_prefs,
        );

        Ok(())
    }

    /// Get user upgrade preferences
    pub fn get_user_upgrade_preferences(
        env: Env,
        user: Address,
    ) -> Option<UserUpgradePreference> {
        env.storage()
            .instance()
            .get(&VerificationDataKey::UserUpgradePreferences(user))
    }

    /// Get version info for a specific WASM hash
    pub fn get_version_info(
        env: Env,
        wasm_hash: [u8; 32],
    ) -> Result<VersionInfo, VerificationError> {
        env.storage()
            .instance()
            .get(&VerificationDataKey::VersionRegistry(wasm_hash))
            .ok_or(VerificationError::VersionNotFound)
    }

    /// Get global upgrade registry
    pub fn get_global_upgrade_registry(env: Env) -> Vec<UpgradeRecord> {
        env.storage()
            .instance()
            .get(&VerificationDataKey::GlobalUpgradeRegistry)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Get pending upgrades for a user
    pub fn get_pending_upgrades(
        env: Env,
        user: Address,
    ) -> Vec<PendingUpgrade> {
        let mut pending = Vec::new(&env);
        
        // This is a simplified implementation
        // In practice, you'd want to iterate through possible grant IDs
        // or maintain an index of pending upgrades per user
        
        pending
    }

    /// Verify that a grant is using the expected WASM hash
    /// 
    /// This is the core verification function that should be called
    /// before executing any grant logic to ensure the user is
    /// interacting with the version they consented to.
    /// 
    /// # Arguments
    /// * `grant_id` - The grant ID to verify
    /// * `expected_hash` - The WASM hash that should be active
    pub fn verify_grant_wasm_hash(
        env: Env,
        grant_id: u64,
        expected_hash: [u8; 32],
    ) -> Result<bool, VerificationError> {
        let hash_info = Self::get_grant_wasm_hash(env, grant_id)?;
        Ok(hash_info.current_hash == expected_hash)
    }

    /// Auto-upgrade grants for security-critical updates
    /// 
    /// This function can be called to automatically upgrade grants
    /// that have opted into security updates.
    /// 
    /// # Arguments
    /// * `new_wasm_hash` - The new security-critical WASM hash
    /// * `admin` - Admin triggering the auto-upgrade
    pub fn auto_upgrade_security_critical(
        env: Env,
        new_wasm_hash: [u8; 32],
        admin: Address,
    ) -> Result<u64, VerificationError> { // Returns number of upgraded grants
        // Verify admin authorization
        admin.require_auth();

        // Get version info
        let version_info = Self::get_version_info(env.clone(), new_wasm_hash)?;
        
        // Only allow auto-upgrade for critical security updates
        if version_info.security_level != SecurityLevel::Critical {
            return Err(VerificationError::SecurityLevelTooLow);
        }

        let mut upgraded_count = 0u64;
        
        // This is a simplified implementation
        // In practice, you'd iterate through all grants and check user preferences
        // For each grant with auto_upgrade_security = true, perform the upgrade
        
        Ok(upgraded_count)
    }
}
