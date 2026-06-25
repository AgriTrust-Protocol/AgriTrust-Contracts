//! Circuit Breaker: Governance Activity Monitor
//!
//! SAFEGUARD: Tracks admin activity and enforces mandatory timelocks
//! If an admin attempts to change more than 3 protocol parameters 
//! (fees, thresholds, addresses) in a single ledger, the actions are 
//! automatically queued with a 7-day mandatory timelock, even if the 
//! standard delay is shorter.
//!
//! This prevents rapid protocol changes and provides community oversight
//! for significant parameter modifications.

#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, contracterror, symbol_short, 
    Address, Env, Vec, Map, String,
};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum parameter changes per ledger before circuit breaker triggers
const MAX_PARAM_CHANGES_PER_LEDGER: u32 = 3;
/// Mandatory timelock duration when circuit breaker triggers (7 days in seconds)
const MANDATORY_TIMELOCK_SECS: u64 = 7 * 24 * 60 * 60;
/// Standard timelock duration (can be overridden by mandatory timelock)
const STANDARD_TIMELOCK_SECS: u64 = 24 * 60 * 60; // 24 hours

// ── Data Structures ───────────────────────────────────────────────────────────

/// Types of protocol parameters that are monitored
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum ParameterType {
    Fee,
    Threshold,
    Address,
    Timelock,
    Other,
}

/// Status of a queued parameter change
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum ChangeStatus {
    /// Change is pending normal timelock
    Pending,
    /// Change is pending mandatory 7-day timelock (circuit breaker triggered)
    MandatoryTimelock,
    /// Change has been executed
    Executed,
    /// Change was cancelled
    Cancelled,
}

/// A parameter change that is being tracked
#[derive(Clone, Debug)]
#[contracttype]
pub struct ParameterChange {
    /// Unique ID for this change
    pub change_id: u64,
    /// Admin who initiated the change
    pub admin: Address,
    /// Type of parameter being changed
    pub parameter_type: ParameterType,
    /// Human-readable description of the parameter
    pub parameter_name: String,
    /// Old value (serialized as bytes)
    pub old_value: soroban_sdk::Bytes,
    /// New value (serialized as bytes)
    pub new_value: soroban_sdk::Bytes,
    /// When the change was proposed
    pub proposed_at: u64,
    /// When the change becomes executable
    pub executable_at: u64,
    /// Current status of the change
    pub status: ChangeStatus,
    /// Ledger number when this change was proposed
    pub ledger_number: u32,
    /// Reason for the change (optional)
    pub reason: String,
}

/// Activity tracking for the current ledger
#[derive(Clone, Debug)]
#[contracttype]
pub struct LedgerActivity {
    /// Current ledger number
    pub ledger_number: u32,
    /// Number of parameter changes in this ledger
    pub change_count: u32,
    /// List of change IDs in this ledger
    pub change_ids: Vec<u64>,
    /// When this ledger activity was first recorded
    pub recorded_at: u64,
}

// ── Storage Keys ─────────────────────────────────────────────────────────────

#[derive(Clone)]
#[contracttype]
pub enum MonitorKey {
    /// Next change ID counter
    NextChangeId,
    /// Maps change_id -> ParameterChange
    Change(u64),
    /// List of all change IDs
    ChangeIds,
    /// Current ledger activity tracking
    CurrentLedgerActivity,
    /// Historical ledger activities (ledger_number -> LedgerActivity)
    LedgerHistory(u32),
    /// Admin address for authorization
    Admin,
    /// Whether the monitor is enabled
    Enabled,
    /// Configuration: max changes per ledger
    MaxChangesPerLedger,
    /// Configuration: mandatory timelock duration
    MandatoryTimelockSecs,
    /// Last heartbeat ledger for an admin
    MonitorLastHeartbeat(Address),
    /// Ledger sequence when the admin was established
    AdminSince(Address),
    /// Monotonically increasing activity counter
    ActivityCounter,
}

// ── Errors ───────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum MonitorError {
    NotInitialized = 1,
    NotAuthorized = 2,
    ChangeNotFound = 3,
    TimelockNotExpired = 4,
    AlreadyExecuted = 5,
    CircuitBreakerTriggered = 6,
    MonitorDisabled = 7,
    InvalidParameter = 8,
    MathOverflow = 9,
}

// ── Contract Implementation ───────────────────────────────────────────────────

#[contract]
pub struct GovernanceActivityMonitor;

#[contractimpl]
impl GovernanceActivityMonitor {
    /// Initialize the monitor with an admin address
    pub fn initialize(env: Env, admin: Address) -> Result<(), MonitorError> {
        if env.storage().instance().has(&MonitorKey::Admin) {
            return Err(MonitorError::NotInitialized);
        }

        admin.require_auth();
        
        env.storage().instance().set(&MonitorKey::Admin, &admin);
        env.storage().instance().set(&MonitorKey::Enabled, &true);
        env.storage().instance().set(&MonitorKey::NextChangeId, &1u64);
        env.storage().instance().set(&MonitorKey::ChangeIds, &Vec::<u64>::new(&env));
        env.storage().instance().set(&MonitorKey::MaxChangesPerLedger, &MAX_PARAM_CHANGES_PER_LEDGER);
        env.storage().instance().set(&MonitorKey::MandatoryTimelockSecs, &MANDATORY_TIMELOCK_SECS);

        env.events().publish(
            (symbol_short!("monitor_init"),),
            (admin, env.ledger().timestamp()),
        );

        Ok(())
    }

    /// Record a parameter change attempt
    /// This should be called by any admin function that changes protocol parameters
    pub fn record_parameter_change(
        env: Env,
        admin: Address,
        parameter_type: ParameterType,
        parameter_name: String,
        old_value: soroban_sdk::Bytes,
        new_value: soroban_sdk::Bytes,
        reason: String,
    ) -> Result<u64, MonitorError> {
        // Check if monitor is enabled
        if !Self::is_enabled(&env) {
            return Ok(0); // Monitor disabled, allow change without tracking
        }

        admin.require_auth();
        Self::assert_admin(&env, &admin)?;

        let now = env.ledger().timestamp();
        let current_ledger = env.ledger().sequence();
        
        // Get or create current ledger activity
        let mut activity = Self::get_or_create_ledger_activity(&env, current_ledger)?;
        
        // Increment change count
        activity.change_count = activity.change_count.checked_add(1).ok_or(MonitorError::MathOverflow)?;
        
        // Check if circuit breaker should be triggered
        let max_changes = Self::get_max_changes_per_ledger(&env)?;
        let triggers_breaker = activity.change_count > max_changes;
        
        // Calculate timelock duration
        let mandatory_timelock = Self::get_mandatory_timelock(&env)?;
        let timelock_duration = if triggers_breaker {
            mandatory_timelock
        } else {
            STANDARD_TIMELOCK_SECS
        };
        
        let executable_at = now.checked_add(timelock_duration).ok_or(MonitorError::MathOverflow)?;
        
        // Create parameter change record
        let change_id = Self::next_change_id(&env)?;
        let change = ParameterChange {
            change_id,
            admin: admin.clone(),
            parameter_type: parameter_type.clone(),
            parameter_name: parameter_name.clone(),
            old_value: old_value.clone(),
            new_value: new_value.clone(),
            proposed_at: now,
            executable_at,
            status: if triggers_breaker { 
                ChangeStatus::MandatoryTimelock 
            } else { 
                ChangeStatus::Pending 
            },
            ledger_number: current_ledger,
            reason: reason.clone(),
        };
        
        // Store the change
        env.storage().instance().set(&MonitorKey::Change(change_id), &change);
        
        // Update change IDs list
        let mut change_ids = Self::get_change_ids(&env)?;
        change_ids.push_back(change_id);
        env.storage().instance().set(&MonitorKey::ChangeIds, &change_ids);
        
        // Update ledger activity
        activity.change_ids.push_back(change_id);
        activity.recorded_at = now;
        env.storage().instance().set(&MonitorKey::CurrentLedgerActivity, &activity);
        
        // Emit event
        let event_symbol = if triggers_breaker {
            symbol_short!("breaker_trig")
        } else {
            symbol_short!("param_change")
        };
        
        env.events().publish(
            (event_symbol, change_id),
            (admin, parameter_name, triggers_breaker, executable_at),
        );
        
        // If circuit breaker triggered, emit additional warning event
        if triggers_breaker {
            env.events().publish(
                (symbol_short!("breaker_warn"),),
                (activity.change_count, max_changes, mandatory_timelock),
            );
        }
        
        Ok(change_id)
    }

    /// Execute a queued parameter change after timelock expires
    pub fn execute_parameter_change(
        env: Env,
        admin: Address,
        change_id: u64,
    ) -> Result<(), MonitorError> {
        admin.require_auth();
        Self::assert_admin(&env, &admin)?;

        let mut change = Self::get_change(&env, change_id)?;
        
        if change.status == ChangeStatus::Executed {
            return Err(MonitorError::AlreadyExecuted);
        }

        let now = env.ledger().timestamp();
        
        if now < change.executable_at {
            return Err(MonitorError::TimelockNotExpired);
        }

        // Mark as executed
        change.status = ChangeStatus::Executed;
        env.storage().instance().set(&MonitorKey::Change(change_id), &change);

        env.events().publish(
            (symbol_short!("param_exec"), change_id),
            (admin, change.parameter_name, now),
        );

        Ok(())
    }

    /// Cancel a pending parameter change
    pub fn cancel_parameter_change(
        env: Env,
        admin: Address,
        change_id: u64,
    ) -> Result<(), MonitorError> {
        admin.require_auth();
        Self::assert_admin(&env, &admin)?;

        let mut change = Self::get_change(&env, change_id)?;
        
        if change.status == ChangeStatus::Executed {
            return Err(MonitorError::AlreadyExecuted);
        }

        change.status = ChangeStatus::Cancelled;
        env.storage().instance().set(&MonitorKey::Change(change_id), &change);

        env.events().publish(
            (symbol_short!("param_cancel"), change_id),
            (admin, change.parameter_name),
        );

        Ok(())
    }

    /// Enable/disable the monitor (admin only)
    pub fn set_enabled(env: Env, admin: Address, enabled: bool) -> Result<(), MonitorError> {
        admin.require_auth();
        Self::assert_admin(&env, &admin)?;

        env.storage().instance().set(&MonitorKey::Enabled, &enabled);

        env.events().publish(
            (symbol_short!("monitor_toggle"),),
            (admin, enabled),
        );

        Ok(())
    }

    /// Update configuration (admin only)
    pub fn update_config(
        env: Env,
        admin: Address,
        max_changes_per_ledger: Option<u32>,
        mandatory_timelock_secs: Option<u64>,
    ) -> Result<(), MonitorError> {
        admin.require_auth();
        Self::assert_admin(&env, &admin)?;

        if let Some(max_changes) = max_changes_per_ledger {
            env.storage().instance().set(&MonitorKey::MaxChangesPerLedger, &max_changes);
        }

        if let Some(timelock) = mandatory_timelock_secs {
            env.storage().instance().set(&MonitorKey::MandatoryTimelockSecs, &timelock);
        }

        env.events().publish(
            (symbol_short!("config_update"),),
            (admin, max_changes_per_ledger, mandatory_timelock_secs),
        );

        Ok(())
    }

    // ── View Functions ─────────────────────────────────────────────────────────

    /// Check if the admin is active or inactive
    pub fn check_activity(env: Env, admin: Address) {
        let now = env.ledger().sequence();
        let admin_since: u32 = env.storage().instance().get(&MonitorKey::AdminSince(admin.clone())).unwrap_or(0);
        let last_heartbeat: u32 = env.storage().instance().get(&MonitorKey::MonitorLastHeartbeat(admin.clone())).unwrap_or(0);

        // Grace period for new admins (1440 ledgers = ~2h)
        if now <= admin_since + 1440 {
            return;
        }

        // Check if inactive based on heartbeat
        if now > last_heartbeat + 17280 {
            env.events().publish((symbol_short!("warn_inac"),), admin.clone());
        }
    }

    /// Bootstrap the new admin's heartbeat
    pub fn record_activity(env: Env, admin: Address) {
        admin.require_auth();
        let current_ledger = env.ledger().sequence();
        
        env.storage().instance().set(&MonitorKey::MonitorLastHeartbeat(admin.clone()), &current_ledger);
        
        if !env.storage().instance().has(&MonitorKey::AdminSince(admin.clone())) {
            env.storage().instance().set(&MonitorKey::AdminSince(admin.clone()), &current_ledger);
        }

        let mut counter: u64 = env.storage().instance().get(&MonitorKey::ActivityCounter).unwrap_or(0);
        counter += 1;
        env.storage().instance().set(&MonitorKey::ActivityCounter, &counter);
    }

    /// Get a parameter change by ID
    pub fn get_parameter_change(env: Env, change_id: u64) -> Result<ParameterChange, MonitorError> {
        Self::get_change(&env, change_id)
    }

    /// Get all parameter changes
    pub fn get_all_changes(env: Env) -> Result<Vec<ParameterChange>, MonitorError> {
        let change_ids = Self::get_change_ids(&env)?;
        let mut changes = Vec::new(&env);
        
        for id in change_ids.iter() {
            if let Ok(change) = Self::get_change(&env, id) {
                changes.push_back(change);
            }
        }
        
        Ok(changes)
    }

    /// Get current ledger activity
    pub fn get_current_ledger_activity(env: Env) -> Result<LedgerActivity, MonitorError> {
        let current_ledger = env.ledger().sequence();
        Self::get_ledger_activity(&env, current_ledger)
    }

    /// Get pending changes for an admin
    pub fn get_pending_changes(env: Env, admin: Address) -> Result<Vec<ParameterChange>, MonitorError> {
        let change_ids = Self::get_change_ids(&env)?;
        let mut pending = Vec::new(&env);
        
        for id in change_ids.iter() {
            if let Ok(change) = Self::get_change(&env, id) {
                if change.admin == admin && 
                   (change.status == ChangeStatus::Pending || change.status == ChangeStatus::MandatoryTimelock) {
                    pending.push_back(change);
                }
            }
        }
        
        Ok(pending)
    }

    /// Check if monitor is enabled
    pub fn is_enabled(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&MonitorKey::Enabled)
            .unwrap_or(false)
    }

    /// Get current admin
    pub fn get_admin(env: &Env) -> Result<Address, MonitorError> {
        env.storage()
            .instance()
            .get(&MonitorKey::Admin)
            .ok_or(MonitorError::NotInitialized)
    }

    // ── Internal Helpers ───────────────────────────────────────────────────────

    fn next_change_id(env: &Env) -> Result<u64, MonitorError> {
        let id: u64 = env
            .storage()
            .instance()
            .get(&MonitorKey::NextChangeId)
            .unwrap_or(1);
        env.storage()
            .instance()
            .set(&MonitorKey::NextChangeId, &(id + 1));
        Ok(id)
    }

    fn get_change(env: &Env, change_id: u64) -> Result<ParameterChange, MonitorError> {
        env.storage()
            .instance()
            .get(&MonitorKey::Change(change_id))
            .ok_or(MonitorError::ChangeNotFound)
    }

    fn get_change_ids(env: &Env) -> Result<Vec<u64>, MonitorError> {
        env.storage()
            .instance()
            .get(&MonitorKey::ChangeIds)
            .ok_or(MonitorError::NotInitialized)
    }

    fn get_or_create_ledger_activity(env: &Env, ledger_number: u32) -> Result<LedgerActivity, MonitorError> {
        // Try to get current activity
        if let Some(activity) = env.storage().instance().get(&MonitorKey::CurrentLedgerActivity) {
            if activity.ledger_number == ledger_number {
                return Ok(activity);
            }
        }

        // Create new activity for this ledger
        let activity = LedgerActivity {
            ledger_number,
            change_count: 0,
            change_ids: Vec::new(env),
            recorded_at: env.ledger().timestamp(),
        };
        
        Ok(activity)
    }

    fn get_ledger_activity(env: &Env, ledger_number: u32) -> Result<LedgerActivity, MonitorError> {
        if let Some(activity) = env.storage().instance().get(&MonitorKey::CurrentLedgerActivity) {
            if activity.ledger_number == ledger_number {
                return Ok(activity);
            }
        }

        // Check historical data
        env.storage()
            .instance()
            .get(&MonitorKey::LedgerHistory(ledger_number))
            .ok_or(MonitorError::ChangeNotFound)
    }

    fn get_max_changes_per_ledger(env: &Env) -> Result<u32, MonitorError> {
        env.storage()
            .instance()
            .get(&MonitorKey::MaxChangesPerLedger)
            .ok_or(MonitorError::NotInitialized)
    }

    fn get_mandatory_timelock(env: &Env) -> Result<u64, MonitorError> {
        env.storage()
            .instance()
            .get(&MonitorKey::MandatoryTimelockSecs)
            .ok_or(MonitorError::NotInitialized)
    }

    fn assert_admin(env: &Env, caller: &Address) -> Result<(), MonitorError> {
        let admin = Self::get_admin(env)?;
        if *caller != admin {
            return Err(MonitorError::NotAuthorized);
        }
        Ok(())
    }
}
