//! Admin Module - Governance Security Components
//!
//! This module provides security-focused admin functionality including:
//! - Dead Man's Switch: Automated admin recovery after inactivity
//! - Governance Activity Monitor: Circuit breaker for rapid parameter changes
//!
//! These components work together to ensure protocol security and proper
//! governance oversight while maintaining operational flexibility.

pub mod dead_mans_switch;
pub mod governance_activity_monitor;

// Re-export main types for easier integration
pub use dead_mans_switch::DeadMansSwitchContract;
pub use governance_activity_monitor::GovernanceActivityMonitor;
pub use governance_activity_monitor::{ParameterType, ChangeStatus, MonitorError};

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, symbol_short};

#[contracttype]
pub enum AdminKey {
    ActiveAdmin,
    PendingAdmin,
    MonitorContract,
}

#[contract]
pub struct AdminContract;

#[contractimpl]
impl AdminContract {
    pub fn initialize(env: Env, initial_admin: Address, monitor_contract: Address) {
        env.storage().instance().set(&AdminKey::ActiveAdmin, &initial_admin);
        env.storage().instance().set(&AdminKey::MonitorContract, &monitor_contract);
    }

    pub fn transfer_ownership(env: Env, new_admin: Address) {
        let active_admin: Address = env.storage().instance().get(&AdminKey::ActiveAdmin).unwrap();
        active_admin.require_auth();
        env.storage().instance().set(&AdminKey::PendingAdmin, &new_admin);
    }

    pub fn accept_ownership(env: Env) {
        let pending_admin: Address = env.storage().instance().get(&AdminKey::PendingAdmin).unwrap();
        pending_admin.require_auth();
        
        let old_admin: Address = env.storage().instance().get(&AdminKey::ActiveAdmin).unwrap();
        
        env.storage().instance().set(&AdminKey::ActiveAdmin, &pending_admin);
        env.storage().instance().remove(&AdminKey::PendingAdmin);

        env.events().publish(
            (symbol_short!("own_trans"),), 
            (old_admin.clone(), pending_admin.clone())
        );

        let current_ledger = env.ledger().sequence();
        env.events().publish(
            (symbol_short!("act_hdff"),),
            (old_admin, pending_admin.clone(), current_ledger)
        );

        // Cross-contract call to record_activity
        if let Some(monitor_contract) = env.storage().instance().get::<_, Address>(&AdminKey::MonitorContract) {
            let monitor_client = governance_activity_monitor::GovernanceActivityMonitorClient::new(&env, &monitor_contract);
            monitor_client.record_activity(&pending_admin);
        }
    }
}
