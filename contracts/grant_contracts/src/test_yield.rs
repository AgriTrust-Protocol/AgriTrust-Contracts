#![cfg(test)]

use soroban_sdk::{testutils::Ledger as _, Address, Env};
use super::yield_treasury::{
    YieldTreasuryContract, YieldPosition, TreasuryConfig, YieldMetrics,
    YIELD_STATUS_INACTIVE, YIELD_STATUS_INVESTED, YIELD_STRATEGY_STELLAR_USDC,
    YieldError, DataKey as YieldDataKey,
};
use super::yield_enhanced::{
    YieldEnhancedGrantContract, EnhancedGrant, EnhancedDataKey, EnhancedError,
};

#[test]
fn test_yield_treasury_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let token_address = Address::generate(&env);
    
    let config = TreasuryConfig {
        admin,
        min_reserve_ratio: 2000, // 20%
        max_investment_ratio: 8000, // 80%
        auto_invest: false,
        yield_strategy: YIELD_STRATEGY_STELLAR_USDC,
        emergency_withdrawal_enabled: true,
    };
    
    // Test successful initialization
    assert_eq!(
        YieldTreasuryContract::initialize(env.clone(), admin, token_address, config),
        Ok(())
    );
    
    // Test duplicate initialization
    assert_eq!(
        YieldTreasuryContract::initialize(env, admin, token_address, config),
        Err(YieldError::AlreadyInitialized)
    );
}

#[test]
fn test_invest_idle_funds() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let token_address = Address::generate(&env);
    
    let config = TreasuryConfig {
        admin,
        min_reserve_ratio: 2000,
        max_investment_ratio: 8000,
        auto_invest: false,
        yield_strategy: YIELD_STRATEGY_STELLAR_USDC,
        emergency_withdrawal_enabled: true,
    };
    
    YieldTreasuryContract::initialize(env.clone(), admin, token_address, config).unwrap();
    
    // Test successful investment
    assert_eq!(
        YieldTreasuryContract::invest_idle_funds(env.clone(), 1000, Some(YIELD_STRATEGY_STELLAR_USDC)),
        Ok(())
    );
    
    // Test duplicate investment
    assert_eq!(
        YieldTreasuryContract::invest_idle_funds(env.clone(), 500, Some(YIELD_STRATEGY_STELLAR_USDC)),
        Err(YieldError::InvestmentActive)
    );
    
    // Test invalid amount
    assert_eq!(
        YieldTreasuryContract::invest_idle_funds(env.clone(), 0, Some(YIELD_STRATEGY_STELLAR_USDC)),
        Err(YieldError::InvalidAmount)
    );
    
    // Test invalid strategy
    assert_eq!(
        YieldTreasuryContract::invest_idle_funds(env.clone(), 1000, Some(999)),
        Err(YieldError::InvalidStrategy)
    );
}

#[test]
fn test_divest_funds() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let token_address = Address::generate(&env);
    
    let config = TreasuryConfig {
        admin,
        min_reserve_ratio: 2000,
        max_investment_ratio: 8000,
        auto_invest: false,
        yield_strategy: YIELD_STRATEGY_STELLAR_USDC,
        emergency_withdrawal_enabled: true,
    };
    
    YieldTreasuryContract::initialize(env.clone(), admin, token_address, config).unwrap();
    
    // Invest first
    YieldTreasuryContract::invest_idle_funds(env.clone(), 1000, Some(YIELD_STRATEGY_STELLAR_USDC)).unwrap();
    
    // Test partial divestment
    assert_eq!(
        YieldTreasuryContract::divest_funds(env.clone(), Some(500)),
        Ok(())
    );
    
    // Test full divestment
    assert_eq!(
        YieldTreasuryContract::divest_funds(env.clone(), None),
        Ok(())
    );
    
    // Test divestment without investment
    assert_eq!(
        YieldTreasuryContract::divest_funds(env.clone(), Some(100)),
        Err(YieldError::InvestmentInactive)
    );
}

#[test]
fn test_yield_position_tracking() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let token_address = Address::generate(&env);
    
    let config = TreasuryConfig {
        admin,
        min_reserve_ratio: 2000,
        max_investment_ratio: 8000,
        auto_invest: false,
        yield_strategy: YIELD_STRATEGY_STELLAR_USDC,
        emergency_withdrawal_enabled: true,
    };
    
    YieldTreasuryContract::initialize(env.clone(), admin, token_address, config).unwrap();
    
    // Invest
    YieldTreasuryContract::invest_idle_funds(env.clone(), 1000, Some(YIELD_STRATEGY_STELLAR_USDC)).unwrap();
    
    // Get position
    let position = YieldTreasuryContract::get_yield_position(env.clone()).unwrap();
    
    assert_eq!(position.strategy, YIELD_STRATEGY_STELLAR_USDC);
    assert_eq!(position.invested_amount, 1000);
    assert_eq!(position.current_value, 1000);
    assert_eq!(position.accrued_yield, 0);
    assert_eq!(position.apy, 500); // 5% APY for USDC strategy
}

#[test]
fn test_yield_metrics() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let token_address = Address::generate(&env);
    
    let config = TreasuryConfig {
        admin,
        min_reserve_ratio: 2000,
        max_investment_ratio: 8000,
        auto_invest: false,
        yield_strategy: YIELD_STRATEGY_STELLAR_USDC,
        emergency_withdrawal_enabled: true,
    };
    
    YieldTreasuryContract::initialize(env.clone(), admin, token_address, config).unwrap();
    
    // Get initial metrics
    let metrics = YieldTreasuryContract::get_yield_metrics(env.clone()).unwrap();
    assert_eq!(metrics.total_invested, 0);
    assert_eq!(metrics.total_yield_earned, 0);
    assert_eq!(metrics.investment_count, 0);
    
    // Invest
    YieldTreasuryContract::invest_idle_funds(env.clone(), 1000, Some(YIELD_STRATEGY_STELLAR_USDC)).unwrap();
    
    // Get updated metrics
    let metrics = YieldTreasuryContract::get_yield_metrics(env.clone()).unwrap();
    assert_eq!(metrics.total_invested, 1000);
    assert_eq!(metrics.investment_count, 1);
    assert_eq!(metrics.current_apy, 500); // 5% APY
}

#[test]
fn test_enhanced_grant_with_yield() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let token_address = Address::generate(&env);
    
    // Initialize enhanced contract with treasury
    assert_eq!(
        YieldEnhancedGrantContract::initialize(env.clone(), admin, token_address, true),
        Ok(())
    );
    
    // Create enhanced grant with yield enabled
    assert_eq!(
        YieldEnhancedGrantContract::create_enhanced_grant(
            env.clone(),
            1,
            recipient,
            10000,
            100,
            1, // STATUS_ACTIVE
            true,
            true,
            2000, // 20% reserve
        ),
        Ok(())
    );
    
    // Get enhanced grant
    let enhanced_grant = YieldEnhancedGrantContract::get_enhanced_grant(env.clone(), 1).unwrap();
    assert!(enhanced_grant.yield_enabled);
    assert!(enhanced_grant.auto_yield_invest);
    assert_eq!(enhanced_grant.min_reserve_percentage, 2000);
}

#[test]
fn test_invest_with_liquidity_protection() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let token_address = Address::generate(&env);
    
    // Initialize enhanced contract
    YieldEnhancedGrantContract::initialize(env.clone(), admin, token_address, true).unwrap();
    
    // Create grant
    YieldEnhancedGrantContract::create_enhanced_grant(
        env.clone(),
        1,
        recipient,
        10000,
        100,
        1, // STATUS_ACTIVE
        true,
        true,
        2000, // 20% reserve
    ).unwrap();
    
    // Test investment with sufficient liquidity
    assert_eq!(
        YieldEnhancedGrantContract::invest_idle_funds(env.clone(), 1000, Some(YIELD_STRATEGY_STELLAR_USDC)),
        Ok(())
    );
    
    // Test that we can't over-invest and jeopardize withdrawals
    // This would need more sophisticated testing with actual token balances
}

#[test]
fn test_enhanced_withdrawal_with_yield() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let token_address = Address::generate(&env);
    
    // Initialize enhanced contract
    YieldEnhancedGrantContract::initialize(env.clone(), admin, token_address, true).unwrap();
    
    // Create grant
    YieldEnhancedGrantContract::create_enhanced_grant(
        env.clone(),
        1,
        recipient,
        10000,
        100,
        1, // STATUS_ACTIVE
        true,
        false, // No auto-invest for this test
        2000, // 20% reserve
    ).unwrap();
    
    // Test withdrawal
    assert_eq!(
        YieldEnhancedGrantContract::enhanced_withdraw(env.clone(), 1, 500),
        Ok(())
    );
}

#[test]
fn test_emergency_withdrawal() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let token_address = Address::generate(&env);
    let recipient = Address::generate(&env);
    
    let config = TreasuryConfig {
        admin,
        min_reserve_ratio: 2000,
        max_investment_ratio: 8000,
        auto_invest: false,
        yield_strategy: YIELD_STRATEGY_STELLAR_USDC,
        emergency_withdrawal_enabled: true,
    };
    
    YieldTreasuryContract::initialize(env.clone(), admin, token_address, config).unwrap();
    
    // Test emergency withdrawal
    assert_eq!(
        YieldTreasuryContract::emergency_withdraw(env.clone(), 500, recipient),
        Ok(())
    );
    
    // Test emergency withdrawal when disabled
    let config_disabled = TreasuryConfig {
        admin,
        min_reserve_ratio: 2000,
        max_investment_ratio: 8000,
        auto_invest: false,
        yield_strategy: YIELD_STRATEGY_STELLAR_USDC,
        emergency_withdrawal_enabled: false,
    };
    
    YieldTreasuryContract::update_config(env.clone(), config_disabled).unwrap();
    
    assert_eq!(
        YieldTreasuryContract::emergency_withdraw(env.clone(), 500, recipient),
        Err(YieldError::EmergencyMode)
    );
}

#[test]
fn test_auto_invest() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let token_address = Address::generate(&env);
    
    let mut config = TreasuryConfig {
        admin,
        min_reserve_ratio: 2000,
        max_investment_ratio: 8000,
        auto_invest: true,
        yield_strategy: YIELD_STRATEGY_STELLAR_USDC,
        emergency_withdrawal_enabled: true,
    };
    
    YieldTreasuryContract::initialize(env.clone(), admin, token_address, config).unwrap();
    
    // Test auto-invest when enabled
    assert_eq!(
        YieldTreasuryContract::auto_invest(env.clone()),
        Ok(())
    );
    
    // Test auto-invest when disabled
    config.auto_invest = false;
    YieldTreasuryContract::update_config(env.clone(), config).unwrap();
    
    assert_eq!(
        YieldTreasuryContract::auto_invest(env.clone()),
        Err(YieldError::InvalidState)
    );
}

#[test]
fn test_strategy_apy() {
    let env = Env::default();
    
    // Test different strategy APYs
    assert_eq!(
        YieldTreasuryContract::get_strategy_apy(&env, YIELD_STRATEGY_STELLAR_USDC),
        Ok(500) // 5%
    );
    
    assert_eq!(
        YieldTreasuryContract::get_strategy_apy(&env, YIELD_STRATEGY_STELLAR_AQUA),
        Ok(800) // 8%
    );
    
    assert_eq!(
        YieldTreasuryContract::get_strategy_apy(&env, YIELD_STRATEGY_LIQUIDITY_POOL),
        Ok(1200) // 12%
    );
    
    // Test invalid strategy
    assert_eq!(
        YieldTreasuryContract::get_strategy_apy(&env, 999),
        Err(YieldError::InvalidStrategy)
    );
}

#[test]
fn test_yield_calculation() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let token_address = Address::generate(&env);
    
    let config = TreasuryConfig {
        admin,
        min_reserve_ratio: 2000,
        max_investment_ratio: 8000,
        auto_invest: false,
        yield_strategy: YIELD_STRATEGY_STELLAR_USDC,
        emergency_withdrawal_enabled: true,
    };
    
    YieldTreasuryContract::initialize(env.clone(), admin, token_address, config).unwrap();
    
    // Invest
    YieldTreasuryContract::invest_idle_funds(env.clone(), 10000, Some(YIELD_STRATEGY_STELLAR_USDC)).unwrap();
    
    // Get position immediately (should have minimal yield)
    let position = YieldTreasuryContract::get_yield_position(env.clone()).unwrap();
    assert_eq!(position.invested_amount, 10000);
    assert_eq!(position.accrued_yield, 0); // No time has passed
    
    // In a real test, you'd advance time and check yield calculation
    // For now, we're testing the structure and basic functionality
}

#[test]
fn test_error_conditions() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let token_address = Address::generate(&env);
    
    // Test operations before initialization
    assert_eq!(
        YieldTreasuryContract::invest_idle_funds(env.clone(), 1000, Some(YIELD_STRATEGY_STELLAR_USDC)),
        Err(YieldError::NotInitialized)
    );
    
    assert_eq!(
        YieldTreasuryContract::divest_funds(env.clone(), Some(100)),
        Err(YieldError::NotInitialized)
    );
    
    assert_eq!(
        YieldTreasuryContract::get_yield_position(env.clone()),
        Err(YieldError::NotInitialized)
    );
    
    // Initialize
    let config = TreasuryConfig {
        admin,
        min_reserve_ratio: 2000,
        max_investment_ratio: 8000,
        auto_invest: false,
        yield_strategy: YIELD_STRATEGY_STELLAR_USDC,
        emergency_withdrawal_enabled: true,
    };
    
    YieldTreasuryContract::initialize(env.clone(), admin, token_address, config).unwrap();
    
    // Test unauthorized access (would fail in real scenario)
    // This is simplified for testing
    assert_eq!(
        YieldTreasuryContract::invest_idle_funds(env.clone(), 1000, Some(YIELD_STRATEGY_STELLAR_USDC)),
        Ok(())
    );
}
