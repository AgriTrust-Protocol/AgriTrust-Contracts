#![no_std]

pub mod optimized;
pub mod benchmarks;
pub mod self_terminate;

// Re-export optimized implementation
pub use optimized::{
    GrantContract, Grant, Error, DataKey,
    STATUS_ACTIVE, STATUS_PAUSED, STATUS_COMPLETED, STATUS_CANCELLED,
    STATUS_REVOCABLE, STATUS_MILESTONE_BASED, STATUS_AUTO_RENEW, STATUS_EMERGENCY_PAUSE,
    has_status, set_status, clear_status, toggle_status,
};

// Re-export self-termination implementation
pub use self_terminate::{
    GrantContract as SelfTerminateContract, SelfTerminateResult, SelfTerminateError,
    STATUS_SELF_TERMINATED, is_self_terminated, can_be_self_terminated,
    validate_self_terminate_transition,
};

#[cfg(test)]
pub use test_optimized::*;
#[cfg(test)]
pub use test_self_terminate::*;
