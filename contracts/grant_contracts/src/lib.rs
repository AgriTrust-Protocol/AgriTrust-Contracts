#![no_std]

pub mod optimized;
// pub mod benchmarks;
// pub mod self_terminate;
// pub mod multi_token;
// pub mod yield_treasury;
// pub mod yield_enhanced;

// #[cfg(test)]
// mod test_snapshot_events;

// Re-export optimized implementation as the main contract
pub use optimized::{
    GrantContract, Grant, Error, DataKey,
    STATUS_ACTIVE, STATUS_PAUSED, STATUS_COMPLETED, STATUS_CANCELLED,
    STATUS_REVOCABLE, STATUS_MILESTONE_BASED, STATUS_AUTO_RENEW, STATUS_EMERGENCY_PAUSE,
    has_status, set_status, clear_status, toggle_status,
};
