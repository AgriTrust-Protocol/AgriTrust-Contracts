//! Auth Context Depth Tracker
//!
//! Tracks Soroban auth context depth to prevent `SorobanAuthContextOutOfBounds`
//! panics in deeply-nested cross-contract admin call chains.
//!
//! Soroban's auth context has an implicit recursion limit (~32 frames).
//! Each `require_auth` call pushes a new frame. The admin chain:
//!   governance_activity_monitor -> admin::dispatch -> dead_mans_switch -> treasury::speed_bump
//! consumes 4+ frames per compound operation, hitting the ceiling under load.

use soroban_sdk::{contracttype, symbol_short, Env};

/// Storage key for the depth counter.
#[contracttype]
pub enum DepthKey {
    /// Current auth context depth counter.
    AdminAuthDepth,
    /// Maximum allowed depth before deferring to two-phase pattern.
    MaxAdminChainDepth,
    /// Whether depth tracking is enabled.
    DepthTrackingEnabled,
}

/// Default maximum admin chain depth (conservative: leaves headroom below 32).
pub const DEFAULT_MAX_ADMIN_CHAIN_DEPTH: u32 = 24;

/// Depth tracking errors.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u32)]
pub enum DepthError {
    DepthExceeded = 1,
    TrackingDisabled = 2,
    CounterCorrupted = 3,
}

/// Initialize depth tracking. Must be called once during contract init.
pub fn initialize_depth_tracking(env: &Env) {
    env.storage()
        .instance()
        .set(&DepthKey::AdminAuthDepth, &0u32);
    env.storage().instance().set(
        &DepthKey::MaxAdminChainDepth,
        &DEFAULT_MAX_ADMIN_CHAIN_DEPTH,
    );
    env.storage()
        .instance()
        .set(&DepthKey::DepthTrackingEnabled, &true);
}

/// Check if depth tracking is enabled.
pub fn is_depth_tracking_enabled(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DepthKey::DepthTrackingEnabled)
        .unwrap_or(false)
}

/// Get the current depth counter.
pub fn get_current_depth(env: &Env) -> u32 {
    if !is_depth_tracking_enabled(env) {
        return 0;
    }
    env.storage()
        .instance()
        .get(&DepthKey::AdminAuthDepth)
        .unwrap_or(0)
}

/// Get the maximum allowed depth.
pub fn get_max_depth(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DepthKey::MaxAdminChainDepth)
        .unwrap_or(DEFAULT_MAX_ADMIN_CHAIN_DEPTH)
}

/// Increment the depth counter before a `require_auth` call.
/// Returns the new depth value.
pub fn push_depth(env: &Env) -> Result<u32, DepthError> {
    if !is_depth_tracking_enabled(env) {
        return Ok(0);
    }

    let current: u32 = env
        .storage()
        .instance()
        .get(&DepthKey::AdminAuthDepth)
        .ok_or(DepthError::CounterCorrupted)?;

    let max = get_max_depth(env);

    if current >= max {
        env.events()
            .publish((symbol_short!("dep_excd"),), (current, max));
        return Err(DepthError::DepthExceeded);
    }

    let new_depth = current + 1;
    env.storage()
        .instance()
        .set(&DepthKey::AdminAuthDepth, &new_depth);
    Ok(new_depth)
}

/// Decrement the depth counter after a `require_auth` call completes.
pub fn pop_depth(env: &Env) -> Result<u32, DepthError> {
    if !is_depth_tracking_enabled(env) {
        return Ok(0);
    }

    let current: u32 = env
        .storage()
        .instance()
        .get(&DepthKey::AdminAuthDepth)
        .ok_or(DepthError::CounterCorrupted)?;

    if current == 0 {
        return Ok(0);
    }

    let new_depth = current - 1;
    env.storage()
        .instance()
        .set(&DepthKey::AdminAuthDepth, &new_depth);
    Ok(new_depth)
}

/// Flush/reset the depth counter. Call at the end of a batch of operations
/// or when ledger sequence changes (natural break point).
pub fn flush_depth(env: &Env) {
    env.storage()
        .instance()
        .set(&DepthKey::AdminAuthDepth, &0u32);
    env.events()
        .publish((symbol_short!("dep_flsh"),), env.ledger().sequence());
}

/// Check if we're approaching the depth limit (within 4 frames).
pub fn is_approaching_limit(env: &Env) -> bool {
    let current = get_current_depth(env);
    let max = get_max_depth(env);
    current >= max.saturating_sub(4)
}

/// Update the maximum depth limit (admin only, via AdminContract).
pub fn set_max_depth(env: &Env, new_max: u32) {
    env.storage()
        .instance()
        .set(&DepthKey::MaxAdminChainDepth, &new_max);
    env.events()
        .publish((symbol_short!("mx_dep"),), new_max);
}
