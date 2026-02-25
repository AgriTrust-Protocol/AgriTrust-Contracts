#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, 
    token, Token, Vec, Map, TryIntoVal, TryFromVal,
};

// Import from parent modules
use super::optimized::{
    Grant, Error, DataKey, STATUS_ACTIVE, STATUS_PAUSED, STATUS_COMPLETED, STATUS_CANCELLED,
    has_status, set_status, clear_status, read_grant, write_grant, settle_grant,
    require_admin_auth, read_admin,
};
use super::yield_treasury::{
    YieldPosition, TreasuryConfig, YieldMetrics,
    YIELD_STRATEGY_STELLAR_AQUA, YIELD_STRATEGY_STELLAR_USDC, YIELD_STRATEGY_LIQUIDITY_POOL,
    YieldError, DataKey as YieldDataKey,
};

#[contract]
pub struct YieldEnhancedGrantContract;

// Enhanced data keys for combined functionality
#[derive(Clone)]
#[contracttype]
pub enum EnhancedDataKey {
    Admin,
    Grant(u64),
    YieldConfig,
    YieldPosition,
    YieldMetrics,
    ReserveBalance,
    YieldToken,
    TreasuryEnabled,
}

#[contracterror]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum EnhancedError {
    // Grant errors
    NotInitialized = 1,
    AlreadyInitialized = 2,
    NotAuthorized = 3,
    GrantNotFound = 4,
    GrantAlreadyExists = 5,
    InvalidRate = 6,
    InvalidAmount = 7,
    InvalidState = 8,
    MathOverflow = 9,
    InvalidStatusTransition = 10,
    
    // Yield errors
    YieldNotEnabled = 100,
    InsufficientReserve = 101,
    InvestmentActive = 102,
    InvestmentInactive = 103,
    InvalidStrategy = 104,
    YieldCalculationFailed = 105,
    EmergencyMode = 106,
}

// Enhanced grant structure with yield tracking
#[derive(Clone)]
#[contracttype]
pub struct EnhancedGrant {
    pub base_grant: Grant,
    pub yield_enabled: bool,
    pub auto_yield_invest: bool,
    pub min_reserve_percentage: i128, // Minimum percentage to keep available for withdrawal
}

// Helper functions for enhanced grant management
fn read_enhanced_grant(env: &Env, grant_id: u64) -> Result<EnhancedGrant, EnhancedError> {
    env.storage()
        .instance()
        .get(&EnhancedDataKey::Grant(grant_id))
        .ok_or(EnhancedError::GrantNotFound)
}

fn write_enhanced_grant(env: &Env, grant_id: u64, grant: &EnhancedGrant) {
    env.storage().instance().set(&EnhancedDataKey::Grant(grant_id), grant);
}

fn is_treasury_enabled(env: &Env) -> Result<bool, EnhancedError> {
    env.storage()
        .instance()
        .get(&EnhancedDataKey::TreasuryEnabled)
        .unwrap_or(false)
}

fn require_treasury_enabled(env: &Env) -> Result<(), EnhancedError> {
    if !is_treasury_enabled(env)? {
        return Err(EnhancedError::YieldNotEnabled);
    }
    Ok(())
}

fn calculate_available_for_investment(
    env: &Env,
    total_balance: i128,
    total_claimable: i128,
    min_reserve_percentage: i128,
) -> Result<i128, EnhancedError> {
    // Calculate minimum reserve needed for withdrawals
    let min_reserve = total_claimable
        .checked_mul(min_reserve_percentage)
        .ok_or(EnhancedError::MathOverflow)?
        .checked_div(10000)
        .ok_or(EnhancedError::MathOverflow)?;
    
    // Available for investment = total_balance - min_reserve
    let available = total_balance
        .checked_sub(min_reserve)
        .ok_or(EnhancedError::InsufficientReserve)?;
    
    Ok(available)
}

fn ensure_grantee_liquidity(
    env: &Env,
    investment_amount: i128,
) -> Result<(), EnhancedError> {
    // Get all active grants and ensure sufficient liquidity
    // This is a simplified check - in production, you'd want to be more sophisticated
    let yield_token = read_yield_token_internal(env)?;
    let contract_balance = yield_token.balance(&env.current_contract_address());
    
    // Calculate total claimable across all grants
    let total_claimable = calculate_total_claimable(env)?;
    
    // Ensure we have enough liquidity for potential withdrawals
    let min_liquidity_needed = total_claimable
        .checked_mul(2000) // 20% buffer
        .ok_or(EnhancedError::MathOverflow)?
        .checked_div(10000)
        .ok_or(EnhancedError::MathOverflow)?;
    
    let remaining_balance = contract_balance
        .checked_sub(investment_amount)
        .ok_or(EnhancedError::InsufficientReserve)?;
    
    if remaining_balance < min_liquidity_needed {
        return Err(EnhancedError::InsufficientReserve);
    }
    
    Ok(())
}

fn calculate_total_claimable(env: &Env) -> Result<i128, EnhancedError> {
    // This is a simplified implementation
    // In production, you'd want to iterate through all grants
    let total_claimable = env
        .storage()
        .instance()
        .bump(&EnhancedDataKey::Grant(0)) // This is a placeholder
        .unwrap_or(0i128);
    
    Ok(total_claimable)
}

fn read_yield_token_internal(env: &Env) -> Result<Token, EnhancedError> {
    let token_address = env
        .storage()
        .instance()
        .get(&EnhancedDataKey::YieldToken)
        .ok_or(EnhancedError::YieldNotEnabled)?;
    Ok(token::Client::new(env, &token_address))
}

#[contractimpl]
impl YieldEnhancedGrantContract {
    /// Initialize the enhanced grant contract with yield functionality
    pub fn initialize(
        env: Env,
        admin: Address,
        yield_token_address: Address,
        enable_treasury: bool,
    ) -> Result<(), EnhancedError> {
        if env.storage().instance().has(&EnhancedDataKey::Admin) {
            return Err(EnhancedError::AlreadyInitialized);
        }
        
        admin.require_auth();
        
        // Set admin
        env.storage().instance().set(&EnhancedDataKey::Admin, &admin);
        
        // Set yield token if treasury is enabled
        if enable_treasury {
            env.storage().instance().set(&EnhancedDataKey::YieldToken, &yield_token_address);
            env.storage().instance().set(&EnhancedDataKey::TreasuryEnabled, &true);
            
            // Initialize basic treasury config
            let config = TreasuryConfig {
                admin,
                min_reserve_ratio: 2000, // 20% minimum reserve
                max_investment_ratio: 8000, // 80% maximum investment
                auto_invest: false,
                yield_strategy: YIELD_STRATEGY_STELLAR_USDC,
                emergency_withdrawal_enabled: true,
            };
            env.storage().instance().set(&EnhancedDataKey::YieldConfig, &config);
            
            // Initialize metrics
            let metrics = YieldMetrics {
                total_invested: 0,
                total_yield_earned: 0,
                current_apy: 0,
                last_yield_calculation: env.ledger().timestamp(),
                investment_count: 0,
            };
            env.storage().instance().set(&EnhancedDataKey::YieldMetrics, &metrics);
            
            // Initialize reserve balance
            env.storage().instance().set(&EnhancedDataKey::ReserveBalance, &0i128);
        }
        
        env.events().publish(
            (symbol_short!("enhanced_init"),),
            (admin, enable_treasury),
        );
        
        Ok(())
    }
    
    /// Create an enhanced grant with yield options
    pub fn create_enhanced_grant(
        env: Env,
        grant_id: u64,
        recipient: Address,
        total_amount: i128,
        flow_rate: i128,
        initial_status_mask: u32,
        yield_enabled: bool,
        auto_yield_invest: bool,
        min_reserve_percentage: i128, // In basis points (10000 = 100%)
    ) -> Result<(), EnhancedError> {
        require_admin_auth(&env)?;
        
        if total_amount <= 0 {
            return Err(EnhancedError::InvalidAmount);
        }
        
        if flow_rate < 0 {
            return Err(EnhancedError::InvalidRate);
        }
        
        if min_reserve_percentage < 0 || min_reserve_percentage > 10000 {
            return Err(EnhancedError::InvalidAmount);
        }
        
        let key = EnhancedDataKey::Grant(grant_id);
        if env.storage().instance().has(&key) {
            return Err(EnhancedError::GrantAlreadyExists);
        }
        
        let now = env.ledger().timestamp();
        let base_grant = Grant {
            recipient,
            total_amount,
            withdrawn: 0,
            claimable: 0,
            flow_rate,
            last_update_ts: now,
            rate_updated_at: now,
            status_mask: initial_status_mask,
        };
        
        let enhanced_grant = EnhancedGrant {
            base_grant,
            yield_enabled,
            auto_yield_invest,
            min_reserve_percentage,
        };
        
        env.storage().instance().set(&key, &enhanced_grant);
        
        env.events().publish(
            (symbol_short!("enhanced_grant_created"),),
            (grant_id, recipient, yield_enabled),
        );
        
        Ok(())
    }
    
    /// Enhanced withdraw that considers yield investments
    pub fn enhanced_withdraw(
        env: Env,
        grant_id: u64,
        amount: i128,
    ) -> Result<(), EnhancedError> {
        if amount <= 0 {
            return Err(EnhancedError::InvalidAmount);
        }
        
        let mut enhanced_grant = read_enhanced_grant(&env, grant_id)?;
        
        // Can only withdraw from active grants
        if !has_status(enhanced_grant.base_grant.status_mask, STATUS_ACTIVE) {
            return Err(EnhancedError::InvalidState);
        }
        
        enhanced_grant.base_grant.recipient.require_auth();
        
        // Settle the grant first
        settle_grant(&mut enhanced_grant.base_grant, env.ledger().timestamp())?;
        
        if amount > enhanced_grant.base_grant.claimable {
            return Err(EnhancedError::InvalidAmount);
        }
        
        // If treasury is enabled and auto-invest is on, check if we need to divest
        if is_treasury_enabled(&env)? && enhanced_grant.yield_enabled && enhanced_grant.auto_yield_invest {
            let yield_token = read_yield_token_internal(&env)?;
            let contract_balance = yield_token.balance(&env.current_contract_address());
            
            // Check if we need to divest to cover withdrawal
            let available_liquidity = contract_balance
                .checked_sub(enhanced_grant.base_grant.claimable)
                .ok_or(EnhancedError::InsufficientReserve)?;
            
            if available_liquidity < amount {
                // Calculate how much to divest
                let needed = amount - available_liquidity;
                Self::emergency_divest_for_withdrawal(&env, needed)?;
            }
        }
        
        // Process withdrawal
        enhanced_grant.base_grant.claimable = enhanced_grant
            .base_grant
            .claimable
            .checked_sub(amount)
            .ok_or(EnhancedError::MathOverflow)?;
        
        enhanced_grant.base_grant.withdrawn = enhanced_grant
            .base_grant
            .withdrawn
            .checked_add(amount)
            .ok_or(EnhancedError::MathOverflow)?;
        
        let accounted = enhanced_grant
            .base_grant
            .withdrawn
            .checked_add(enhanced_grant.base_grant.claimable)
            .ok_or(EnhancedError::MathOverflow)?;
        
        if accounted == enhanced_grant.base_grant.total_amount {
            enhanced_grant.base_grant.status_mask = set_status(enhanced_grant.base_grant.status_mask, STATUS_COMPLETED);
            enhanced_grant.base_grant.status_mask = clear_status(enhanced_grant.base_grant.status_mask, STATUS_ACTIVE);
        }
        
        write_enhanced_grant(&env, grant_id, &enhanced_grant);
        
        // Transfer tokens to recipient
        let yield_token = read_yield_token_internal(&env)?;
        yield_token.transfer(&env.current_contract_address(), &enhanced_grant.base_grant.recipient, &amount);
        
        env.events().publish(
            (symbol_short!("enhanced_withdraw"),),
            (grant_id, amount, enhanced_grant.base_grant.recipient),
        );
        
        Ok(())
    }
    
    /// Invest idle funds from available grant reserves
    pub fn invest_idle_funds(
        env: Env,
        amount: i128,
        strategy: Option<u32>,
    ) -> Result<(), EnhancedError> {
        require_admin_auth(&env)?;
        require_treasury_enabled(&env)?;
        
        if amount <= 0 {
            return Err(EnhancedError::InvalidAmount);
        }
        
        // Ensure grantee liquidity is preserved
        ensure_grantee_liquidity(&env, amount)?;
        
        // Check if there's an active investment
        if env.storage().instance().has(&EnhancedDataKey::YieldPosition) {
            return Err(EnhancedError::InvestmentActive);
        }
        
        let yield_token = read_yield_token_internal(&env)?;
        let contract_balance = yield_token.balance(&env.current_contract_address());
        
        if amount > contract_balance {
            return Err(EnhancedError::InsufficientReserve);
        }
        
        // Determine strategy
        let config = env
            .storage()
            .instance()
            .get::<_, TreasuryConfig>(&EnhancedDataKey::YieldConfig)
            .ok_or(EnhancedError::YieldNotEnabled)?;
        
        let investment_strategy = strategy.unwrap_or(config.yield_strategy);
        
        // Validate strategy
        match investment_strategy {
            YIELD_STRATEGY_STELLAR_AQUA
            | YIELD_STRATEGY_STELLAR_USDC
            | YIELD_STRATEGY_LIQUIDITY_POOL => {},
            _ => return Err(EnhancedError::InvalidStrategy),
        }
        
        // Create yield position
        let now = env.ledger().timestamp();
        let apy = Self::get_strategy_apy(&env, investment_strategy)?;
        
        let position = YieldPosition {
            strategy: investment_strategy,
            invested_amount: amount,
            current_value: amount,
            accrued_yield: 0,
            invested_at: now,
            last_yield_update: now,
            apy,
        };
        
        env.storage().instance().set(&EnhancedDataKey::YieldPosition, &position);
        
        // Update reserve balance
        let current_reserve = env
            .storage()
            .instance()
            .get::<_, i128>(&EnhancedDataKey::ReserveBalance)
            .unwrap_or(0);
        let new_reserve = current_reserve
            .checked_sub(amount)
            .ok_or(EnhancedError::InsufficientReserve)?;
        env.storage().instance().set(&EnhancedDataKey::ReserveBalance, &new_reserve);
        
        // Update metrics
        let mut metrics = env
            .storage()
            .instance()
            .get::<_, YieldMetrics>(&EnhancedDataKey::YieldMetrics)
            .ok_or(EnhancedError::YieldNotEnabled)?;
        metrics.total_invested = metrics
            .total_invested
            .checked_add(amount)
            .ok_or(EnhancedError::MathOverflow)?;
        metrics.investment_count += 1;
        metrics.current_apy = apy;
        metrics.last_yield_calculation = now;
        env.storage().instance().set(&EnhancedDataKey::YieldMetrics, &metrics);
        
        env.events().publish(
            (symbol_short!("yield_invest"),),
            (amount, investment_strategy, apy),
        );
        
        Ok(())
    }
    
    /// Divest funds from yield-bearing strategy
    pub fn divest_funds(
        env: Env,
        amount: Option<i128>,
    ) -> Result<(), EnhancedError> {
        require_admin_auth(&env)?;
        require_treasury_enabled(&env)?;
        
        let mut position = env
            .storage()
            .instance()
            .get::<_, YieldPosition>(&EnhancedDataKey::YieldPosition)
            .ok_or(EnhancedError::InvestmentInactive)?;
        
        // Update position with accrued yield
        let now = env.ledger().timestamp();
        let time_elapsed = now - position.last_yield_update;
        if time_elapsed > 0 {
            let seconds_in_year = 365u64 * 24u64 * 60u64 * 60u64;
            let time_ratio = i128::from(time_elapsed);
            let year_ratio = i128::from(seconds_in_year);
            
            let new_yield = position
                .invested_amount
                .checked_mul(position.apy)
                .ok_or(EnhancedError::MathOverflow)?
                .checked_mul(time_ratio)
                .ok_or(EnhancedError::MathOverflow)?
                .checked_div(10000)
                .ok_or(EnhancedError::MathOverflow)?
                .checked_div(year_ratio)
                .ok_or(EnhancedError::MathOverflow)?;
            
            position.accrued_yield = position
                .accrued_yield
                .checked_add(new_yield)
                .ok_or(EnhancedError::MathOverflow)?;
            
            position.current_value = position
                .invested_amount
                .checked_add(position.accrued_yield)
                .ok_or(EnhancedError::MathOverflow)?;
            
            position.last_yield_update = now;
        }
        
        // Determine divestment amount
        let divest_amount = match amount {
            Some(amt) => {
                if amt <= 0 {
                    return Err(EnhancedError::InvalidAmount);
                }
                if amt > position.current_value {
                    return Err(EnhancedError::InsufficientInvestment);
                }
                amt
            },
            None => position.current_value,
        };
        
        // Calculate remaining position
        let remaining_value = position
            .current_value
            .checked_sub(divest_amount)
            .ok_or(EnhancedError::MathOverflow)?;
        
        // Calculate proportional investment and yield
        let investment_ratio = if position.current_value > 0 {
            position.invested_amount
                .checked_mul(10000)
                .ok_or(EnhancedError::MathOverflow)?
                .checked_div(position.current_value)
                .ok_or(EnhancedError::MathOverflow)?
        } else {
            10000
        };
        
        let investment_return = divest_amount
            .checked_mul(investment_ratio)
            .ok_or(EnhancedError::MathOverflow)?
            .checked_div(10000)
            .ok_or(EnhancedError::MathOverflow)?;
        
        let yield_return = divest_amount
            .checked_sub(investment_return)
            .ok_or(EnhancedError::MathOverflow)?;
        
        // Update position
        position.invested_amount = position
            .invested_amount
            .checked_sub(investment_return)
            .ok_or(EnhancedError::MathOverflow)?;
        
        position.accrued_yield = position
            .accrued_yield
            .checked_sub(yield_return)
            .ok_or(EnhancedError::MathOverflow)?;
        
        position.current_value = remaining_value;
        
        // If fully divested, remove position
        if remaining_value == 0 {
            env.storage().instance().remove(&EnhancedDataKey::YieldPosition);
        } else {
            env.storage().instance().set(&EnhancedDataKey::YieldPosition, &position);
        }
        
        // Update reserve balance
        let current_reserve = env
            .storage()
            .instance()
            .get::<_, i128>(&EnhancedDataKey::ReserveBalance)
            .unwrap_or(0);
        let new_reserve = current_reserve
            .checked_add(divest_amount)
            .ok_or(EnhancedError::MathOverflow)?;
        env.storage().instance().set(&EnhancedDataKey::ReserveBalance, &new_reserve);
        
        // Update metrics
        let mut metrics = env
            .storage()
            .instance()
            .get::<_, YieldMetrics>(&EnhancedDataKey::YieldMetrics)
            .ok_or(EnhancedError::YieldNotEnabled)?;
        metrics.total_yield_earned = metrics
            .total_yield_earned
            .checked_add(yield_return)
            .ok_or(EnhancedError::MathOverflow)?;
        
        if remaining_value == 0 {
            metrics.current_apy = 0;
        }
        metrics.last_yield_calculation = now;
        env.storage().instance().set(&EnhancedDataKey::YieldMetrics, &metrics);
        
        env.events().publish(
            (symbol_short!("yield_divest"),),
            (divest_amount, investment_return, yield_return),
        );
        
        Ok(())
    }
    
    /// Emergency divest for withdrawal (internal function)
    fn emergency_divest_for_withdrawal(env: &Env, needed_amount: i128) -> Result<(), EnhancedError> {
        let mut position = env
            .storage()
            .instance()
            .get::<_, YieldPosition>(&EnhancedDataKey::YieldPosition)
            .ok_or(EnhancedError::InvestmentInactive)?;
        
        // Calculate current value with yield
        let now = env.ledger().timestamp();
        let time_elapsed = now - position.last_yield_update;
        let current_value = if time_elapsed > 0 {
            let seconds_in_year = 365u64 * 24u64 * 60u64 * 60u64;
            let time_ratio = i128::from(time_elapsed);
            let year_ratio = i128::from(seconds_in_year);
            
            let new_yield = position
                .invested_amount
                .checked_mul(position.apy)
                .ok_or(EnhancedError::MathOverflow)?
                .checked_mul(time_ratio)
                .ok_or(EnhancedError::MathOverflow)?
                .checked_div(10000)
                .ok_or(EnhancedError::MathOverflow)?
                .checked_div(year_ratio)
                .ok_or(EnhancedError::MathOverflow)?;
            
            position.invested_amount + position.accrued_yield + new_yield
        } else {
            position.current_value
        };
        
        // Divest the needed amount (or all if needed > current_value)
        let divest_amount = if needed_amount > current_value {
            current_value
        } else {
            needed_amount
        };
        
        // Remove position if fully divested
        if divest_amount >= current_value {
            env.storage().instance().remove(&EnhancedDataKey::YieldPosition);
        } else {
            // Update remaining position
            let remaining_ratio = (current_value - divest_amount)
                .checked_mul(10000)
                .ok_or(EnhancedError::MathOverflow)?
                .checked_div(current_value)
                .ok_or(EnhancedError::MathOverflow)?;
            
            position.invested_amount = position
                .invested_amount
                .checked_mul(remaining_ratio)
                .ok_or(EnhancedError::MathOverflow)?
                .checked_div(10000)
                .ok_or(EnhancedError::MathOverflow)?;
            
            position.accrued_yield = position
                .accrued_yield
                .checked_mul(remaining_ratio)
                .ok_or(EnhancedError::MathOverflow)?
                .checked_div(10000)
                .ok_or(EnhancedError::MathOverflow)?;
            
            position.current_value = current_value - divest_amount;
            env.storage().instance().set(&EnhancedDataKey::YieldPosition, &position);
        }
        
        // Update reserve balance
        let current_reserve = env
            .storage()
            .instance()
            .get::<_, i128>(&EnhancedDataKey::ReserveBalance)
            .unwrap_or(0);
        let new_reserve = current_reserve
            .checked_add(divest_amount)
            .ok_or(EnhancedError::MathOverflow)?;
        env.storage().instance().set(&EnhancedDataKey::ReserveBalance, &new_reserve);
        
        env.events().publish(
            (symbol_short!("emergency_divest"),),
            (divest_amount, needed_amount),
        );
        
        Ok(())
    }
    
    /// Get enhanced grant information
    pub fn get_enhanced_grant(env: Env, grant_id: u64) -> Result<EnhancedGrant, EnhancedError> {
        let mut enhanced_grant = read_enhanced_grant(&env, grant_id)?;
        settle_grant(&mut enhanced_grant.base_grant, env.ledger().timestamp())?;
        Ok(enhanced_grant)
    }
    
    /// Get yield position
    pub fn get_yield_position(env: Env) -> Result<YieldPosition, EnhancedError> {
        require_treasury_enabled(&env)?;
        
        let mut position = env
            .storage()
            .instance()
            .get::<_, YieldPosition>(&EnhancedDataKey::YieldPosition)
            .ok_or(EnhancedError::InvestmentInactive)?;
        
        // Update with accrued yield
        let now = env.ledger().timestamp();
        let time_elapsed = now - position.last_yield_update;
        if time_elapsed > 0 {
            let seconds_in_year = 365u64 * 24u64 * 60u64 * 60u64;
            let time_ratio = i128::from(time_elapsed);
            let year_ratio = i128::from(seconds_in_year);
            
            let new_yield = position
                .invested_amount
                .checked_mul(position.apy)
                .ok_or(EnhancedError::MathOverflow)?
                .checked_mul(time_ratio)
                .ok_or(EnhancedError::MathOverflow)?
                .checked_div(10000)
                .ok_or(EnhancedError::MathOverflow)?
                .checked_div(year_ratio)
                .ok_or(EnhancedError::MathOverflow)?;
            
            position.accrued_yield = position
                .accrued_yield
                .checked_add(new_yield)
                .ok_or(EnhancedError::MathOverflow)?;
            
            position.current_value = position
                .invested_amount
                .checked_add(position.accrued_yield)
                .ok_or(EnhancedError::MathOverflow)?;
            
            position.last_yield_update = now;
        }
        
        Ok(position)
    }
    
    /// Get treasury metrics
    pub fn get_treasury_metrics(env: Env) -> Result<YieldMetrics, EnhancedError> {
        require_treasury_enabled(&env)?;
        env
            .storage()
            .instance()
            .get(&EnhancedDataKey::YieldMetrics)
            .ok_or(EnhancedError::YieldNotEnabled)
    }
    
    /// Check if treasury is enabled
    pub fn is_treasury_enabled(env: Env) -> bool {
        is_treasury_enabled(&env).unwrap_or(false)
    }
    
    /// Get total available balance (reserve + invested)
    pub fn get_total_available_balance(env: Env) -> Result<i128, EnhancedError> {
        if !is_treasury_enabled(&env)? {
            return Ok(0);
        }
        
        let yield_token = read_yield_token_internal(&env)?;
        Ok(yield_token.balance(&env.current_contract_address()))
    }
    
    /// Get APY for a specific strategy
    fn get_strategy_apy(env: &Env, strategy: u32) -> Result<i128, EnhancedError> {
        match strategy {
            YIELD_STRATEGY_STELLAR_AQUA => Ok(800), // 8% APY
            YIELD_STRATEGY_STELLAR_USDC => Ok(500), // 5% APY
            YIELD_STRATEGY_LIQUIDITY_POOL => Ok(1200), // 12% APY
            _ => Err(EnhancedError::InvalidStrategy),
        }
    }
    
    /// Auto-invest idle funds (can be called by anyone)
    pub fn auto_invest_idle_funds(env: Env) -> Result<(), EnhancedError> {
        require_treasury_enabled(&env)?;
        
        let config = env
            .storage()
            .instance()
            .get::<_, TreasuryConfig>(&EnhancedDataKey::YieldConfig)
            .ok_or(EnhancedError::YieldNotEnabled)?;
        
        if !config.auto_invest {
            return Err(EnhancedError::InvalidState);
        }
        
        // Check if there's already an active investment
        if env.storage().instance().has(&EnhancedDataKey::YieldPosition) {
            return Err(EnhancedError::InvestmentActive);
        }
        
        let yield_token = read_yield_token_internal(&env)?;
        let contract_balance = yield_token.balance(&env.current_contract_address());
        let reserve_balance = env
            .storage()
            .instance()
            .get::<_, i128>(&EnhancedDataKey::ReserveBalance)
            .unwrap_or(0);
        
        // Calculate available idle funds
        let idle_funds = contract_balance
            .checked_sub(reserve_balance)
            .ok_or(EnhancedError::InsufficientReserve)?;
        
        // Calculate maximum investment based on ratio
        let max_investment = contract_balance
            .checked_mul(config.max_investment_ratio)
            .ok_or(EnhancedError::MathOverflow)?
            .checked_div(10000)
            .ok_or(EnhancedError::MathOverflow)?;
        
        let investment_amount = if idle_funds > max_investment {
            max_investment
        } else {
            idle_funds
        };
        
        if investment_amount > 0 {
            Self::invest_idle_funds(env, investment_amount, Some(config.yield_strategy))?;
        }
        
        Ok(())
    }
    
    /// Check if investment is active
    pub fn is_investment_active(env: Env) -> Result<bool, EnhancedError> {
        require_treasury_enabled(&env)?;
        Ok(env.storage().instance().has(&EnhancedDataKey::YieldPosition))
    }
}
