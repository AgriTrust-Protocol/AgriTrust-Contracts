use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, Map, Symbol, Vec, i128,
};

// --- Clawback-Resilient Accounting Constants ---
pub const SHARES_SCALING_FACTOR: i128 = 1_000_000; // 6 decimal places for share precision

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ShareBasedBalance {
    pub total_shares: i128,
    pub user_shares: Map<Address, i128>,
    pub last_known_balance: i128,
    pub balance_timestamp: u64,
    pub token_address: Address,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct BalanceAdjustmentEvent {
    pub old_balance: i128,
    pub new_balance: i128,
    pub timestamp: u64,
    pub reason: String,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[contracterror]
pub enum BalanceError {
    InsufficientShares = 1,
    MathOverflow = 2,
    InvalidAmount = 3,
    ZeroShares = 4,
    NotInitialized = 5,
}

/// Clawback-Resilient Balance Tracker
pub struct ClawbackResilientTracker;

#[contractimpl]
impl ClawbackResilientTracker {
    /// Initialize share-based balance tracking for a token
    pub fn initialize_balance_tracking(
        env: Env,
        admin: Address,
        token_address: Address,
        initial_balance: i128,
    ) -> Result<(), BalanceError> {
        admin.require_auth();
        
        if initial_balance <= 0 {
            return Err(BalanceError::InvalidAmount);
        }
        
        let balance_key = Symbol::new(&env, &format!("balance_{}", token_address));
        
        let share_balance = ShareBasedBalance {
            total_shares: SHARES_SCALING_FACTOR, // Start with 1 share = 1 token
            user_shares: Map::new(&env),
            last_known_balance: initial_balance,
            balance_timestamp: env.ledger().timestamp(),
            token_address: token_address.clone(),
        };
        
        env.storage().instance().set(&balance_key, share_balance);
        
        Ok(())
    }
    
    /// Add shares for a user (when they deposit tokens)
    pub fn add_user_shares(
        env: Env,
        admin: Address,
        token_address: Address,
        user: Address,
        amount: i128,
    ) -> Result<(), BalanceError> {
        admin.require_auth();
        
        if amount <= 0 {
            return Err(BalanceError::InvalidAmount);
        }
        
        let balance_key = Symbol::new(&env, &format!("balance_{}", token_address));
        let mut share_balance = Self::get_share_balance(&env, token_address)?;
        
        // Calculate shares to add based on current pool state
        let current_physical_balance = Self::get_current_physical_balance(&env, token_address)?;
        let shares_to_add = Self::calculate_shares_from_amount(
            amount,
            current_physical_balance,
            share_balance.total_shares,
        )?;
        
        // Update user shares
        let current_user_shares = share_balance.user_shares.get(user).unwrap_or(0);
        let new_user_shares = current_user_shares
            .checked_add(shares_to_add)
            .ok_or(BalanceError::MathOverflow)?;
        
        share_balance.user_shares.set(user, new_user_shares);
        share_balance.total_shares = share_balance.total_shares
            .checked_add(shares_to_add)
            .ok_or(BalanceError::MathOverflow)?;
        
        // Update balance tracking
        share_balance.last_known_balance = current_physical_balance;
        share_balance.balance_timestamp = env.ledger().timestamp();
        
        env.storage().instance().set(&balance_key, share_balance);
        
        Ok(())
    }
    
    /// Remove shares for a user (when they withdraw tokens)
    pub fn remove_user_shares(
        env: Env,
        token_address: Address,
        user: Address,
        shares: i128,
    ) -> Result<i128, BalanceError> {
        if shares <= 0 {
            return Err(BalanceError::InvalidAmount);
        }
        
        let balance_key = Symbol::new(&env, &format!("balance_{}", token_address));
        let mut share_balance = Self::get_share_balance(&env, token_address)?;
        
        // Check user has enough shares
        let current_user_shares = share_balance.user_shares.get(user).unwrap_or(0);
        if current_user_shares < shares {
            return Err(BalanceError::InsufficientShares);
        }
        
        // Calculate token amount based on current pool state
        let current_physical_balance = Self::get_current_physical_balance(&env, token_address)?;
        let token_amount = Self::calculate_amount_from_shares(
            shares,
            current_physical_balance,
            share_balance.total_shares,
        )?;
        
        // Update user shares
        let new_user_shares = current_user_shares
            .checked_sub(shares)
            .ok_or(BalanceError::InsufficientShares)?;
        
        if new_user_shares == 0 {
            share_balance.user_shares.remove(user);
        } else {
            share_balance.user_shares.set(user, new_user_shares);
        }
        
        share_balance.total_shares = share_balance.total_shares
            .checked_sub(shares)
            .ok_or(BalanceError::MathOverflow)?;
        
        // Update balance tracking
        share_balance.last_known_balance = current_physical_balance;
        share_balance.balance_timestamp = env.ledger().timestamp();
        
        env.storage().instance().set(&balance_key, share_balance);
        
        Ok(token_amount)
    }
    
    /// Get user's token balance based on shares
    pub fn get_user_balance(env: Env, token_address: Address, user: Address) -> Result<i128, BalanceError> {
        let share_balance = Self::get_share_balance(&env, token_address)?;
        let user_shares = share_balance.user_shares.get(user).unwrap_or(0);
        
        if user_shares == 0 || share_balance.total_shares == 0 {
            return Ok(0);
        }
        
        let current_physical_balance = Self::get_current_physical_balance(&env, token_address)?;
        
        // Calculate user's proportional balance
        let user_balance = (user_shares * current_physical_balance) / share_balance.total_shares;
        
        Ok(user_balance)
    }
    
    /// Handle external balance changes (clawbacks, deposits, etc.)
    pub fn handle_balance_change(env: Env, token_address: Address) -> Result<(), BalanceError> {
        let balance_key = Symbol::new(&env, &format!("balance_{}", token_address));
        let mut share_balance = Self::get_share_balance(&env, token_address)?;
        
        let current_balance = Self::get_current_physical_balance(&env, token_address)?;
        let last_balance = share_balance.last_known_balance;
        
        if current_balance != last_balance {
            // Balance changed due to external factors (clawback, deposit, etc.)
            let adjustment = BalanceAdjustmentEvent {
                old_balance: last_balance,
                new_balance: current_balance,
                timestamp: env.ledger().timestamp(),
                reason: if current_balance < last_balance {
                    String::from_str(&env, "External clawback detected")
                } else {
                    String::from_str(&env, "External deposit detected")
                },
            };
            
            // Update balance tracking
            share_balance.last_known_balance = current_balance;
            share_balance.balance_timestamp = env.ledger().timestamp();
            
            // Store adjustment event for transparency
            let events_key = Symbol::new(&env, &format!("balance_events_{}", token_address));
            let mut events = Self::get_balance_events(&env, token_address)?;
            events.push_back(adjustment);
            env.storage().instance().set(&events_key, events);
            
            env.storage().instance().set(&balance_key, share_balance);
        }
        
        Ok(())
    }
    
    /// Get total shares for a token
    pub fn get_total_shares(env: Env, token_address: Address) -> Result<i128, BalanceError> {
        let share_balance = Self::get_share_balance(&env, token_address)?;
        Ok(share_balance.total_shares)
    }
    
    /// Get user's share count
    pub fn get_user_shares(env: Env, token_address: Address, user: Address) -> Result<i128, BalanceError> {
        let share_balance = Self::get_share_balance(&env, token_address)?;
        Ok(share_balance.user_shares.get(user).unwrap_or(0))
    }
    
    /// Get balance adjustment events
    pub fn get_balance_events(env: Env, token_address: Address) -> Result<Vec<BalanceAdjustmentEvent>, BalanceError> {
        let events_key = Symbol::new(&env, &format!("balance_events_{}", token_address));
        Ok(env.storage().instance().get(&events_key).unwrap_or_else(|| Vec::new(&env)))
    }
    
    /// Get share-based balance structure
    fn get_share_balance(env: &Env, token_address: Address) -> Result<ShareBasedBalance, BalanceError> {
        let balance_key = Symbol::new(env, &format!("balance_{}", token_address));
        env.storage().instance()
            .get(&balance_key)
            .ok_or(BalanceError::NotInitialized)
    }
    
    /// Get current physical balance from token contract
    fn get_current_physical_balance(env: &Env, token_address: Address) -> Result<i128, BalanceError> {
        // In a real implementation, this would query the token contract
        // For now, we'll use stored balance
        let contract_address = env.current_contract_address();
        let token_client = soroban_sdk::token::Client::new(env, &token_address);
        
        token_client.balance(&contract_address)
    }
    
    /// Calculate shares from token amount
    fn calculate_shares_from_amount(
        amount: i128,
        current_balance: i128,
        total_shares: i128,
    ) -> Result<i128, BalanceError> {
        if current_balance == 0 || total_shares == 0 {
            return Err(BalanceError::ZeroShares);
        }
        
        // shares = amount * total_shares / current_balance
        let scaled_amount = amount
            .checked_mul(SHARES_SCALING_FACTOR)
            .ok_or(BalanceError::MathOverflow)?;
        
        let shares = (scaled_amount * total_shares)
            .checked_div(current_balance)
            .ok_or(BalanceError::MathOverflow)?
            .checked_div(SHARES_SCALING_FACTOR)
            .ok_or(BalanceError::MathOverflow)?;
        
        Ok(shares)
    }
    
    /// Calculate token amount from shares
    fn calculate_amount_from_shares(
        shares: i128,
        current_balance: i128,
        total_shares: i128,
    ) -> Result<i128, BalanceError> {
        if total_shares == 0 {
            return Err(BalanceError::ZeroShares);
        }
        
        // amount = shares * current_balance / total_shares
        let amount = (shares * current_balance)
            .checked_div(total_shares)
            .ok_or(BalanceError::MathOverflow)?;
        
        Ok(amount)
    }
    
    /// Emergency rebalance after major clawback
    pub fn emergency_rebalance(
        env: Env,
        admin: Address,
        token_address: Address,
    ) -> Result<(), BalanceError> {
        admin.require_auth();
        
        // Force balance update and check for critical situations
        Self::handle_balance_change(env, token_address.clone())?;
        
        let share_balance = Self::get_share_balance(&env, token_address.clone())?;
        let current_balance = Self::get_current_physical_balance(&env, token_address.clone())?;
        
        // If balance is critically low, implement emergency measures
        if current_balance < share_balance.last_known_balance / 2 {
            // Emit emergency event
            let emergency_key = Symbol::new(&env, "emergency_clawback");
            env.events().publish(
                emergency_key,
                (
                    token_address,
                    current_balance,
                    share_balance.last_known_balance,
                    env.ledger().timestamp(),
                ),
            );
        }
        
        Ok(())
    }
}
