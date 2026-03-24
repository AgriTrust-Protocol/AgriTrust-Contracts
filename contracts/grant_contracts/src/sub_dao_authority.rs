#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Vec,
    Symbol, Map, String,
};

/// Delegated Clawback Authority for Sub-DAOs
/// 
/// This module implements a hierarchical permission system where:
/// 1. Main DAO can grant specific powers to Sub-DAOs
/// 2. Sub-DAOs can pause/clawback grants within their jurisdiction
/// 3. Main DAO retains veto power over Sub-DAO actions
/// 4. Comprehensive audit trail and event emission

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum PermissionLevel {
    None,           // No permissions
    Pause,          // Can pause/resume grants
    Clawback,       // Can pause/resume and cancel grants (clawback)
    Full,           // All permissions including rate changes
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum SubDaoStatus {
    Active,
    Suspended,
    Revoked,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct SubDaoPermission {
    pub sub_dao_address: Address,
    pub department: String,           // e.g., "Marketing", "Engineering"
    pub permission_level: PermissionLevel,
    pub status: SubDaoStatus,
    pub granted_at: u64,
    pub granted_by: Address,          // Main DAO admin who granted permissions
    pub expires_at: Option<u64>,      // Optional expiration time
    pub max_grant_amount: i128,       // Maximum total grant amount they can manage
    pub managed_grants: Vec<u64>,     // List of grant IDs they manage
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct VetoRecord {
    pub veto_id: u64,
    pub sub_dao_address: Address,
    pub action_type: String,          // "pause", "resume", "cancel", "rate_change"
    pub grant_id: u64,
    pub veto_reason: String,
    pub vetoed_by: Address,           // Main DAO admin
    pub vetoed_at: u64,
    pub original_action_timestamp: u64,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct ActionLog {
    pub action_id: u64,
    pub sub_dao_address: Address,
    pub action_type: String,
    pub grant_id: u64,
    pub action_data: String,         // Additional action-specific data
    pub executed_at: u64,
    pub vetoed: bool,
    pub veto_id: Option<u64>,
}

#[derive(Clone)]
#[contracttype]
pub enum SubDaoDataKey {
    SubDaoPermission(Address),        // sub_dao_address -> permission
    ManagedGrants(Address),          // sub_dao_address -> Vec<grant_id>
    VetoRecord(u64),                 // veto_id -> VetoRecord
    ActionLog(u64),                  // action_id -> ActionLog
    NextVetoId,
    NextActionId,
    MainDaoAdmin,                    // Main DAO admin address
    DepartmentIndex(String),         // department -> Vec<sub_dao_address>
    GrantToSubDao(u64),              // grant_id -> sub_dao_address (who manages it)
}

#[contracterror]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum SubDaoError {
    NotInitialized = 2001,
    AlreadyInitialized = 2002,
    NotAuthorized = 2003,
    SubDaoNotFound = 2004,
    InsufficientPermissions = 2005,
    SubDaoSuspended = 2006,
    GrantNotManaged = 2007,
    ExceededMaxAmount = 2008,
    VetoNotFound = 2009,
    ActionNotFound = 2010,
    InvalidDepartment = 2011,
    PermissionExpired = 2012,
    MainDaoVeto = 2013,
    MathOverflow = 2014,
    InvalidState = 2015,
}

#[contract]
pub struct SubDaoAuthority;

#[contractimpl]
impl SubDaoAuthority {
    /// Initialize the Sub-DAO authority system
    pub fn initialize(env: Env, main_dao_admin: Address) -> Result<(), SubDaoError> {
        if env.storage().instance().has(&SubDaoDataKey::MainDaoAdmin) {
            return Err(SubDaoError::AlreadyInitialized);
        }

        env.storage().instance().set(&SubDaoDataKey::MainDaoAdmin, &main_dao_admin);
        env.storage().instance().set(&SubDaoDataKey::NextVetoId, &1u64);
        env.storage().instance().set(&SubDaoDataKey::NextActionId, &1u64);

        // Emit initialization event
        env.events().publish(
            (symbol_short!("subdao_init"),),
            main_dao_admin,
        );

        Ok(())
    }

    /// Grant permissions to a Sub-DAO
    pub fn grant_sub_dao_permissions(
        env: Env,
        main_dao_admin: Address,
        sub_dao_address: Address,
        department: String,
        permission_level: PermissionLevel,
        max_grant_amount: i128,
        expires_at: Option<u64>,
    ) -> Result<(), SubDaoError> {
        Self::require_main_dao_auth(&env, &main_dao_admin)?;

        if max_grant_amount <= 0 {
            return Err(SubDaoError::InvalidState);
        }

        let now = env.ledger().timestamp();
        
        // Check if expiration is in the past
        if let Some(expiry) = expires_at {
            if expiry <= now {
                return Err(SubDaoError::PermissionExpired);
            }
        }

        let permission = SubDaoPermission {
            sub_dao_address: sub_dao_address.clone(),
            department: department.clone(),
            permission_level,
            status: SubDaoStatus::Active,
            granted_at: now,
            granted_by: main_dao_admin.clone(),
            expires_at,
            max_grant_amount,
            managed_grants: Vec::new(&env),
        };

        // Store permission
        env.storage().instance().set(&SubDaoDataKey::SubDaoPermission(sub_dao_address.clone()), &permission);
        
        // Initialize managed grants list
        env.storage().instance().set(&SubDaoDataKey::ManagedGrants(sub_dao_address.clone()), &Vec::new(&env));
        
        // Update department index
        let mut department_index = Self::get_department_index(&env, &department)?;
        department_index.push_back(sub_dao_address.clone());
        env.storage().instance().set(&SubDaoDataKey::DepartmentIndex(department), &department_index);

        // Emit event
        env.events().publish(
            (symbol_short!("permission_granted"),),
            (sub_dao_address, department, permission_level as u32, max_grant_amount),
        );

        Ok(())
    }

    /// Assign a grant to a Sub-DAO for management
    pub fn assign_grant_to_sub_dao(
        env: Env,
        main_dao_admin: Address,
        sub_dao_address: Address,
        grant_id: u64,
    ) -> Result<(), SubDaoError> {
        Self::require_main_dao_auth(&env, &main_dao_admin)?;

        // Check if Sub-DAO exists and is active
        let mut permission = Self::get_sub_dao_permission(&env, &sub_dao_address)?;
        if permission.status != SubDaoStatus::Active {
            return Err(SubDaoError::SubDaoSuspended);
        }

        // Check if grant is already assigned
        if let Some(current_manager) = env.storage().instance().get::<_, Address>(&SubDaoDataKey::GrantToSubDao(grant_id)) {
            if current_manager == sub_dao_address {
                return Err(SubDaoError::InvalidState); // Already assigned to this Sub-DAO
            }
        }

        // Add to Sub-DAO's managed grants
        let mut managed_grants = Self::get_managed_grants(&env, &sub_dao_address)?;
        managed_grants.push_back(grant_id);
        env.storage().instance().set(&SubDaoDataKey::ManagedGrants(sub_dao_address.clone()), &managed_grants);
        
        // Update permission record
        permission.managed_grants = managed_grants;
        env.storage().instance().set(&SubDaoDataKey::SubDaoPermission(sub_dao_address.clone()), &permission);
        
        // Set grant assignment
        env.storage().instance().set(&SubDaoDataKey::GrantToSubDao(grant_id), &sub_dao_address);

        // Emit event
        env.events().publish(
            (symbol_short!("grant_assigned"),),
            (sub_dao_address, grant_id),
        );

        Ok(())
    }

    /// Delegated pause function for Sub-DAOs
    pub fn delegated_pause_grant(
        env: Env,
        sub_dao_address: Address,
        grant_id: u64,
        reason: String,
    ) -> Result<u64, SubDaoError> {
        Self::validate_sub_dao_action(&env, &sub_dao_address, grant_id, PermissionLevel::Pause)?;

        let action_id = Self::log_action(
            &env,
            sub_dao_address.clone(),
            "pause".into_val(&env),
            grant_id,
            reason.clone(),
        )?;

        // Emit event
        env.events().publish(
            (symbol_short!("delegated_pause"),),
            (sub_dao_address, grant_id, action_id, reason),
        );

        Ok(action_id)
    }

    /// Delegated resume function for Sub-DAOs
    pub fn delegated_resume_grant(
        env: Env,
        sub_dao_address: Address,
        grant_id: u64,
        reason: String,
    ) -> Result<u64, SubDaoError> {
        Self::validate_sub_dao_action(&env, &sub_dao_address, grant_id, PermissionLevel::Pause)?;

        let action_id = Self::log_action(
            &env,
            sub_dao_address.clone(),
            "resume".into_val(&env),
            grant_id,
            reason.clone(),
        )?;

        // Emit event
        env.events().publish(
            (symbol_short!("delegated_resume"),),
            (sub_dao_address, grant_id, action_id, reason),
        );

        Ok(action_id)
    }

    /// Delegated clawback (cancel) function for Sub-DAOs
    pub fn delegated_clawback_grant(
        env: Env,
        sub_dao_address: Address,
        grant_id: u64,
        reason: String,
    ) -> Result<u64, SubDaoError> {
        Self::validate_sub_dao_action(&env, &sub_dao_address, grant_id, PermissionLevel::Clawback)?;

        let action_id = Self::log_action(
            &env,
            sub_dao_address.clone(),
            "cancel".into_val(&env),
            grant_id,
            reason.clone(),
        )?;

        // Emit event
        env.events().publish(
            (symbol_short!("delegated_clawback"),),
            (sub_dao_address, grant_id, action_id, reason),
        );

        Ok(action_id)
    }

    /// Main DAO veto power - override Sub-DAO action
    pub fn veto_sub_dao_action(
        env: Env,
        main_dao_admin: Address,
        action_id: u64,
        veto_reason: String,
    ) -> Result<u64, SubDaoError> {
        Self::require_main_dao_auth(&env, &main_dao_admin)?;

        let mut action_log = Self::get_action_log(&env, action_id)?;
        if action_log.vetoed {
            return Err(SubDaoError::InvalidState); // Already vetoed
        }

        let veto_id = Self::get_next_veto_id(&env)?;
        let now = env.ledger().timestamp();

        // Create veto record
        let veto_record = VetoRecord {
            veto_id,
            sub_dao_address: action_log.sub_dao_address.clone(),
            action_type: action_log.action_type.clone(),
            grant_id: action_log.grant_id,
            veto_reason: veto_reason.clone(),
            vetoed_by: main_dao_admin.clone(),
            vetoed_at: now,
            original_action_timestamp: action_log.executed_at,
        };

        // Store veto record
        env.storage().instance().set(&SubDaoDataKey::VetoRecord(veto_id), &veto_record);
        Self::set_next_veto_id(&env, veto_id + 1);

        // Update action log
        action_log.vetoed = true;
        action_log.veto_id = Some(veto_id);
        env.storage().instance().set(&SubDaoDataKey::ActionLog(action_id), &action_log);

        // Emit veto event
        env.events().publish(
            (symbol_short!("action_vetoed"),),
            (action_log.sub_dao_address, action_id, veto_id, veto_reason),
        );

        Ok(veto_id)
    }

    /// Revoke Sub-DAO permissions
    pub fn revoke_sub_dao_permissions(
        env: Env,
        main_dao_admin: Address,
        sub_dao_address: Address,
        reason: String,
    ) -> Result<(), SubDaoError> {
        Self::require_main_dao_auth(&env, &main_dao_admin)?;

        let mut permission = Self::get_sub_dao_permission(&env, &sub_dao_address)?;
        permission.status = SubDaoStatus::Revoked;
        env.storage().instance().set(&SubDaoDataKey::SubDaoPermission(sub_dao_address.clone()), &permission);

        // Remove from department index
        let mut department_index = Self::get_department_index(&env, &permission.department)?;
        let mut new_index = Vec::new(&env);
        for addr in department_index.iter() {
            if addr != sub_dao_address {
                new_index.push_back(addr);
            }
        }
        env.storage().instance().set(&SubDaoDataKey::DepartmentIndex(permission.department), &new_index);

        // Emit event
        env.events().publish(
            (symbol_short!("permission_revoked"),),
            (sub_dao_address, reason),
        );

        Ok(())
    }

    /// Suspend Sub-DAO (temporary)
    pub fn suspend_sub_dao(
        env: Env,
        main_dao_admin: Address,
        sub_dao_address: Address,
        reason: String,
    ) -> Result<(), SubDaoError> {
        Self::require_main_dao_auth(&env, &main_dao_admin)?;

        let mut permission = Self::get_sub_dao_permission(&env, &sub_dao_address)?;
        if permission.status == SubDaoStatus::Revoked {
            return Err(SubDaoError::InvalidState);
        }

        permission.status = SubDaoStatus::Suspended;
        env.storage().instance().set(&SubDaoDataKey::SubDaoPermission(sub_dao_address.clone()), &permission);

        // Emit event
        env.events().publish(
            (symbol_short!("subdao_suspended"),),
            (sub_dao_address, reason),
        );

        Ok(())
    }

    /// Unsuspend Sub-DAO
    pub fn unsuspend_sub_dao(
        env: Env,
        main_dao_admin: Address,
        sub_dao_address: Address,
    ) -> Result<(), SubDaoError> {
        Self::require_main_dao_auth(&env, &main_dao_admin)?;

        let mut permission = Self::get_sub_dao_permission(&env, &sub_dao_address)?;
        if permission.status != SubDaoStatus::Suspended {
            return Err(SubDaoError::InvalidState);
        }

        permission.status = SubDaoStatus::Active;
        env.storage().instance().set(&SubDaoDataKey::SubDaoPermission(sub_dao_address.clone()), &permission);

        // Emit event
        env.events().publish(
            (symbol_short!("subdao_unsuspended"),),
            sub_dao_address,
        );

        Ok(())
    }

    // View functions

    /// Get Sub-DAO permission details
    pub fn get_sub_dao_permission(env: Env, sub_dao_address: Address) -> Result<SubDaoPermission, SubDaoError> {
        Self::get_sub_dao_permission(&env, &sub_dao_address)
    }

    /// Get grants managed by a Sub-DAO
    pub fn get_managed_grants(env: Env, sub_dao_address: Address) -> Result<Vec<u64>, SubDaoError> {
        Ok(Self::get_managed_grants(&env, &sub_dao_address)?)
    }

    /// Get action log details
    pub fn get_action_log(env: Env, action_id: u64) -> Result<ActionLog, SubDaoError> {
        Self::get_action_log(&env, action_id)
    }

    /// Get veto record details
    pub fn get_veto_record(env: Env, veto_id: u64) -> Result<VetoRecord, SubDaoError> {
        env.storage()
            .instance()
            .get(&SubDaoDataKey::VetoRecord(veto_id))
            .ok_or(SubDaoError::VetoNotFound)
    }

    /// Get all Sub-DAOs in a department
    pub fn get_department_sub_daos(env: Env, department: String) -> Result<Vec<Address>, SubDaoError> {
        Ok(Self::get_department_index(&env, &department)?)
    }

    /// Get who manages a specific grant
    pub fn get_grant_manager(env: Env, grant_id: u64) -> Result<Option<Address>, SubDaoError> {
        Ok(env.storage().instance().get(&SubDaoDataKey::GrantToSubDao(grant_id)))
    }

    // Private helper functions

    fn require_main_dao_auth(env: &Env, caller: &Address) -> Result<(), SubDaoError> {
        let main_dao_admin = Self::get_main_dao_admin(env)?;
        if *caller != main_dao_admin {
            return Err(SubDaoError::NotAuthorized);
        }
        caller.require_auth();
        Ok(())
    }

    fn get_main_dao_admin(env: &Env) -> Result<Address, SubDaoError> {
        env.storage()
            .instance()
            .get(&SubDaoDataKey::MainDaoAdmin)
            .ok_or(SubDaoError::NotInitialized)
    }

    fn get_sub_dao_permission(env: &Env, sub_dao_address: &Address) -> Result<SubDaoPermission, SubDaoError> {
        env.storage()
            .instance()
            .get(&SubDaoDataKey::SubDaoPermission(sub_dao_address.clone()))
            .ok_or(SubDaoError::SubDaoNotFound)
    }

    fn get_managed_grants(env: &Env, sub_dao_address: &Address) -> Result<Vec<u64>, SubDaoError> {
        env.storage()
            .instance()
            .get(&SubDaoDataKey::ManagedGrants(sub_dao_address.clone()))
            .unwrap_or_else(|| Vec::new(env))
    }

    fn get_department_index(env: &Env, department: &String) -> Result<Vec<Address>, SubDaoError> {
        env.storage()
            .instance()
            .get(&SubDaoDataKey::DepartmentIndex(department.clone()))
            .unwrap_or_else(|| Vec::new(env))
    }

    fn get_action_log(env: &Env, action_id: u64) -> Result<ActionLog, SubDaoError> {
        env.storage()
            .instance()
            .get(&SubDaoDataKey::ActionLog(action_id))
            .ok_or(SubDaoError::ActionNotFound)
    }

    fn get_next_veto_id(env: &Env) -> Result<u64, SubDaoError> {
        env.storage()
            .instance()
            .get(&SubDaoDataKey::NextVetoId)
            .unwrap_or(1)
    }

    fn set_next_veto_id(env: &Env, next_id: u64) {
        env.storage().instance().set(&SubDaoDataKey::NextVetoId, &next_id);
    }

    fn get_next_action_id(env: &Env) -> Result<u64, SubDaoError> {
        env.storage()
            .instance()
            .get(&SubDaoDataKey::NextActionId)
            .unwrap_or(1)
    }

    fn set_next_action_id(env: &Env, next_id: u64) {
        env.storage().instance().set(&SubDaoDataKey::NextActionId, &next_id);
    }

    fn validate_sub_dao_action(
        env: &Env,
        sub_dao_address: &Address,
        grant_id: u64,
        required_permission: PermissionLevel,
    ) -> Result<(), SubDaoError> {
        sub_dao_address.require_auth();

        // Check Sub-DAO permission
        let permission = Self::get_sub_dao_permission(env, sub_dao_address)?;
        
        if permission.status != SubDaoStatus::Active {
            return Err(SubDaoError::SubDaoSuspended);
        }

        // Check permission level
        if permission.permission_level as u32 < required_permission as u32 {
            return Err(SubDaoError::InsufficientPermissions);
        }

        // Check if permission has expired
        if let Some(expiry) = permission.expires_at {
            if env.ledger().timestamp() >= expiry {
                return Err(SubDaoError::PermissionExpired);
            }
        }

        // Check if Sub-DAO manages this grant
        let managed_grants = Self::get_managed_grants(env, sub_dao_address)?;
        let mut manages_grant = false;
        for managed_id in managed_grants.iter() {
            if *managed_id == grant_id {
                manages_grant = true;
                break;
            }
        }
        
        if !manages_grant {
            return Err(SubDaoError::GrantNotManaged);
        }

        Ok(())
    }

    fn log_action(
        env: &Env,
        sub_dao_address: Address,
        action_type: String,
        grant_id: u64,
        action_data: String,
    ) -> Result<u64, SubDaoError> {
        let action_id = Self::get_next_action_id(env)?;
        let now = env.ledger().timestamp();

        let action_log = ActionLog {
            action_id,
            sub_dao_address: sub_dao_address.clone(),
            action_type: action_type.clone(),
            grant_id,
            action_data: action_data.clone(),
            executed_at: now,
            vetoed: false,
            veto_id: None,
        };

        env.storage().instance().set(&SubDaoDataKey::ActionLog(action_id), &action_log);
        Self::set_next_action_id(env, action_id + 1);

        Ok(action_id)
    }
}
