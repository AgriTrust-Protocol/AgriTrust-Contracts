use crate::depth_tracker;
use soroban_sdk::{contracttype, Address, Env};

const INACTIVITY_PERIOD: u64 = 180 * 24 * 60 * 60; // 180 days in seconds

// ── Storage Keys ─────────────────────────────────────────────────────────────

#[contracttype]
pub enum SwitchKey {
    PrimaryAdmin,       // Address
    RecoveryVault,      // Address
    LastActivityAt,     // u64 timestamp — reset on every admin action
    RecoveryExecuted,   // bool
}

// ── Module ──────────────────────────────────────────────────────────────────
/// Dead Man's Switch functionality (regular module, not a separate contract).
/// All auth checks are performed by the calling AdminContract.

pub struct DeadMansSwitchModule;

impl DeadMansSwitchModule {

    /// Initialize switch storage. Caller (AdminContract) must auth the admin.
    pub fn initialize(env: &Env, primary_admin: Address, recovery_vault: Address) {
        env.storage().instance().set(&SwitchKey::PrimaryAdmin, &primary_admin);
        env.storage().instance().set(&SwitchKey::RecoveryVault, &recovery_vault);
        env.storage().instance().set(&SwitchKey::LastActivityAt, &env.ledger().timestamp());
        env.storage().instance().set(&SwitchKey::RecoveryExecuted, &false);
    }

    /// Check if the recovery period has elapsed.
    pub fn is_recovery_due(env: &Env) -> bool {
        let last_activity: u64 = env.storage().instance().get(&SwitchKey::LastActivityAt).unwrap_or(0);
        let current_time = env.ledger().timestamp();
        current_time.saturating_sub(last_activity) >= INACTIVITY_PERIOD
    }

    /// Reset the activity timer (called by AdminContract after admin actions).
    pub fn reset_last_activity(env: &Env) {
        env.storage().instance().set(&SwitchKey::LastActivityAt, &env.ledger().timestamp());
    }

    /// Execute recovery — resets admin to recovery vault.
    /// Caller must verify is_recovery_due() first and auth the caller.
    pub fn execute_recovery(env: &Env) -> Address {
        let recovery_vault: Address = env.storage().instance().get(&SwitchKey::RecoveryVault).unwrap();
        env.storage().instance().set(&SwitchKey::PrimaryAdmin, &recovery_vault);
        env.storage().instance().set(&SwitchKey::RecoveryExecuted, &true);
        recovery_vault
    }

    pub fn get_primary_admin(env: &Env) -> Option<Address> {
        env.storage()
            .instance()
            .get(&SwitchKey::PrimaryAdmin)
    }

    pub fn set_recovery_vault(env: &Env, new_vault: Address) {
        env.storage().instance().set(&SwitchKey::RecoveryVault, &new_vault);
    }
}
