//! Admin Module - Governance Security Components
//!
//! This module provides security-focused admin functionality including:
//! - Dead Man's Switch: Automated admin recovery after inactivity
//! - Governance Activity Monitor: Circuit breaker for rapid parameter changes
//! - Auth Depth Tracking: Prevents SorobanAuthContextOverflow in nested call chains
//!
//! These components work together to ensure protocol security and proper
//! governance oversight while maintaining operational flexibility.

#![no_std]

pub mod dead_mans_switch;
pub mod depth_tracker;
pub mod governance_activity_monitor;

// Re-export main types for easier integration
pub use dead_mans_switch::DeadMansSwitchModule;
pub use depth_tracker::{
    flush_depth, get_current_depth, get_max_depth, initialize_depth_tracking,
    is_approaching_limit, is_depth_tracking_enabled, pop_depth, push_depth,
    set_max_depth, DepthError, DepthKey, DEFAULT_MAX_ADMIN_CHAIN_DEPTH,
};
pub use governance_activity_monitor::GovernanceActivityMonitorModule;
pub use governance_activity_monitor::{ParameterType, ChangeStatus, MonitorError};

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, symbol_short};

#[contracttype]
pub enum AdminKey {
    ActiveAdmin,
    PendingAdmin,
}

#[contract]
pub struct AdminContract;

#[contractimpl]
impl AdminContract {
    pub fn initialize(env: Env, initial_admin: Address) {
        env.storage().instance().set(&AdminKey::ActiveAdmin, &initial_admin);
        depth_tracker::initialize_depth_tracking(&env);
    }

    pub fn transfer_ownership(env: Env, new_admin: Address) {
        let active_admin: Address = env.storage().instance().get(&AdminKey::ActiveAdmin).unwrap();

        depth_tracker::push_depth(&env).unwrap_or(0);
        active_admin.require_auth();
        depth_tracker::pop_depth(&env).unwrap_or(0);

        env.storage().instance().set(&AdminKey::PendingAdmin, &new_admin);
    }

    pub fn accept_ownership(env: Env) {
        let pending_admin: Address = env.storage().instance().get(&AdminKey::PendingAdmin).unwrap();

        depth_tracker::push_depth(&env).unwrap_or(0);
        pending_admin.require_auth();
        depth_tracker::pop_depth(&env).unwrap_or(0);

        let old_admin: Address = env.storage().instance().get(&AdminKey::ActiveAdmin).unwrap();

        env.storage().instance().set(&AdminKey::ActiveAdmin, &pending_admin);
        env.storage().instance().remove(&AdminKey::PendingAdmin);

        env.events().publish(
            (symbol_short!("own_trans"),),
            (old_admin.clone(), pending_admin.clone()),
        );

        let current_ledger = env.ledger().sequence();
        env.events().publish(
            (symbol_short!("act_hdff"),),
            (old_admin, pending_admin.clone(), current_ledger),
        );

        // Direct module call with depth check
        if depth_tracker::is_approaching_limit(&env) {
            depth_tracker::flush_depth(&env);
        }

        governance_activity_monitor::GovernanceActivityMonitorModule::record_activity(&env, pending_admin);

        depth_tracker::flush_depth(&env);
    }

    /// Get current auth depth (view function for monitoring)
    pub fn get_auth_depth(env: Env) -> u32 {
        depth_tracker::get_current_depth(&env)
    }

    /// Check if depth tracking is enabled
    pub fn is_auth_depth_tracking_enabled(env: Env) -> bool {
        depth_tracker::is_depth_tracking_enabled(&env)
    }

    /// Expose depth tracking configuration
    pub fn get_max_auth_chain_depth(env: Env) -> u32 {
        depth_tracker::get_max_depth(&env)
    }
}

#[cfg(test)]
mod depth_tracker_test;
