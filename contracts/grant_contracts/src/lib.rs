#![no_std]

pub mod optimized;
pub mod benchmarks;
pub mod self_terminate;
pub mod multi_token;
pub mod yield_treasury;
pub mod yield_enhanced;

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

// Re-export multi-token implementation
pub use multi_token::{
    GrantContract as MultiTokenContract, TokenBalance, TokenWithdrawal, MultiTokenWithdrawResult,
    MultiTokenGrant, MultiTokenError, create_token_balance, create_token_withdrawal,
};

// Re-export yield treasury implementation
pub use yield_treasury::{
    YieldTreasuryContract, YieldPosition, TreasuryConfig, YieldMetrics,
    YIELD_STATUS_INACTIVE, YIELD_STATUS_INVESTING, YIELD_STATUS_INVESTED, 
    YIELD_STATUS_DIVESTING, YIELD_STATUS_EMERGENCY,
    YIELD_STRATEGY_STELLAR_AQUA, YIELD_STRATEGY_STELLAR_USDC, YIELD_STRATEGY_LIQUIDITY_POOL,
    YieldError,
};

// Re-export yield-enhanced implementation
pub use yield_enhanced::{
    YieldEnhancedGrantContract, EnhancedGrant, EnhancedDataKey, EnhancedError,
};

#[cfg(test)]
pub use test_optimized::*;
#[cfg(test)]
pub use test_self_terminate::*;
#[cfg(test)]
pub use test_multi_token::*;
#[cfg(test)]
pub use test_yield::*;
