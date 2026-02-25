#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env,
    Vec, Map,
};

use super::optimized::{
    GrantContract, Grant, Error, DataKey, read_grant, write_grant, settle_grant,
    STATUS_ACTIVE, STATUS_PAUSED, STATUS_COMPLETED, STATUS_CANCELLED,
    has_status, set_status, clear_status, read_admin, require_admin_auth,
};

fn emit_multi_token_snapshot(env: &Env, grant_id: u64, grant: &MultiTokenGrant) {
    env.events().publish(
        (symbol_short!("mt_snapshot"), grant_id),
        (grant.tokens.len(), grant.status_mask, grant.last_update_ts),
    );
}

/// Token balance structure for multi-token support
#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct TokenBalance {
    pub token_address: Address,  // Token contract address
    pub total_amount: i128,      // Total amount allocated for this token
    pub withdrawn: i128,         // Amount already withdrawn
    pub claimable: i128,         // Amount currently claimable
    pub flow_rate: i128,         // Rate per second for this token
}

/// Multi-token grant structure
#[derive(Clone, Debug)]
#[contracttype]
pub struct MultiTokenGrant {
    pub recipient: Address,
    pub tokens: Vec<TokenBalance>,  // Vector of token balances
    pub last_update_ts: u64,
    pub rate_updated_at: u64,
    pub status_mask: u32,
}

/// Withdrawal request for specific token
#[derive(Clone, Debug)]
#[contracttype]
pub struct TokenWithdrawal {
    pub token_address: Address,
    pub amount: i128,
}

/// Multi-token withdrawal result
#[derive(Clone, Debug)]
#[contracttype]
pub struct MultiTokenWithdrawResult {
    pub grant_id: u64,
    pub successful_withdrawals: Vec<TokenWithdrawal>,
    pub failed_withdrawals: Vec<TokenWithdrawal>,
    pub total_withdrawn: Map<Address, i128>,  // token_address -> amount
    pub withdrawn_at: u64,
}

#[contracterror]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum MultiTokenError {
    TokenNotFound = 16,
    InvalidTokenAddress = 17,
    DuplicateToken = 18,
    EmptyTokenList = 19,
    TokenTransferFailed = 20,
    InsufficientTokenBalance = 21,
    InvalidTokenAmount = 22,
}

/// Multi-token grant implementation
impl GrantContract {
    /// Create a multi-token grant
    /// 
    /// # Arguments
    /// * `grant_id` - Unique grant identifier
    /// * `recipient` - Grant recipient address
    /// * `tokens` - Vector of token balances with initial amounts and flow rates
    /// * `initial_status_mask` - Initial status flags
    pub fn create_multi_token_grant(
        env: Env,
        grant_id: u64,
        recipient: Address,
        tokens: Vec<TokenBalance>,
        initial_status_mask: u32,
    ) -> Result<(), Error> {
        // Validate input
        if tokens.is_empty() {
            return Err(Error::InvalidAmount);
        }

        // Validate tokens
        Self::validate_token_balances(&tokens)?;

        // Check for duplicate tokens
        Self::check_duplicate_tokens(&tokens)?;

        // Check if grant already exists
        let key = DataKey::Grant(grant_id);
        if env.storage().instance().has(&key) {
            return Err(Error::GrantAlreadyExists);
        }

        // Validate initial status
        super::optimized::validate_status_transition(0, initial_status_mask)?;

        let now = env.ledger().timestamp();
        let multi_token_grant = MultiTokenGrant {
            recipient: recipient.clone(),
            tokens,
            last_update_ts: now,
            rate_updated_at: now,
            status_mask: initial_status_mask,
        };

        // Store the multi-token grant
        env.storage().instance().set(&key, &multi_token_grant);

        // Emit creation event
        env.events().publish(
            (symbol_short!("multi_create"), grant_id),
            (recipient, multi_token_grant.tokens.len(), now),
        );
        emit_multi_token_snapshot(&env, grant_id, &multi_token_grant);

        Ok(())
    }

    /// Withdraw from multiple tokens in a single transaction
    /// 
    /// # Arguments
    /// * `grant_id` - Grant identifier
    /// * `withdrawals` - Vector of token withdrawal requests
    /// 
    /// # Returns
    /// * `MultiTokenWithdrawResult` - Details of successful and failed withdrawals
    pub fn multi_token_withdraw(
        env: Env,
        grant_id: u64,
        withdrawals: Vec<TokenWithdrawal>,
    ) -> Result<MultiTokenWithdrawResult, Error> {
        if withdrawals.is_empty() {
            return Err(Error::InvalidAmount);
        }

        let mut grant = Self::read_multi_token_grant(&env, grant_id)?;

        // Check grant status
        if !has_status(grant.status_mask, STATUS_ACTIVE) {
            return Err(Error::InvalidState);
        }

        // Require recipient authentication
        grant.recipient.require_auth();

        // Settle all token balances
        Self::settle_multi_token_grant(&mut grant, env.ledger().timestamp())?;

        let mut successful_withdrawals = Vec::new(&env);
        let mut failed_withdrawals = Vec::new(&env);
        let mut total_withdrawn = Map::new(&env);

        // Process each withdrawal
        for withdrawal in withdrawals {
            match Self::process_token_withdrawal(&mut grant, &withdrawal) {
                Ok(amount_withdrawn) => {
                    successful_withdrawals.push_back(withdrawal.clone());
                    total_withdrawn.set(withdrawal.token_address, amount_withdrawn);
                }
                Err(_) => {
                    failed_withdrawals.push_back(withdrawal.clone());
                }
            }
        }

        // Check if all tokens are fully withdrawn
        if Self::is_grant_fully_withdrawn(&grant) {
            grant.status_mask = set_status(grant.status_mask, STATUS_COMPLETED);
            grant.status_mask = clear_status(grant.status_mask, STATUS_ACTIVE);
        }

        // Update grant in storage
        Self::write_multi_token_grant(&env, grant_id, &grant);

        let result = MultiTokenWithdrawResult {
            grant_id,
            successful_withdrawals,
            failed_withdrawals,
            total_withdrawn,
            withdrawn_at: env.ledger().timestamp(),
        };

        // Emit withdrawal event
        env.events().publish(
            (symbol_short!("multi_withdraw"), grant_id),
            (
                result.successful_withdrawals.len(),
                result.failed_withdrawals.len(),
                result.withdrawn_at,
            ),
        );

        Ok(result)
    }

    /// Get claimable amount for a specific token
    pub fn get_token_claimable(env: Env, grant_id: u64, token_address: Address) -> Result<i128, Error> {
        let mut grant = Self::read_multi_token_grant(&env, grant_id)?;
        Self::settle_multi_token_grant(&mut grant, env.ledger().timestamp())?;

        for token_balance in grant.tokens.iter() {
            if token_balance.token_address == token_address {
                return Ok(token_balance.claimable);
            }
        }

        Err(Error::GrantNotFound) // Token not found
    }

    /// Get all token balances for a grant
    pub fn get_multi_token_grant(env: Env, grant_id: u64) -> Result<MultiTokenGrant, Error> {
        let mut grant = Self::read_multi_token_grant(&env, grant_id)?;
        Self::settle_multi_token_grant(&mut grant, env.ledger().timestamp())?;
        Ok(grant)
    }

    /// Update flow rates for multiple tokens
    pub fn update_multi_token_rates(
        env: Env,
        grant_id: u64,
        token_updates: Vec<TokenBalance>, // Contains token_address and new flow_rate
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;

        let mut grant = Self::read_multi_token_grant(&env, grant_id)?;

        // Check grant status
        if !has_status(grant.status_mask, STATUS_ACTIVE) && !has_status(grant.status_mask, STATUS_PAUSED) {
            return Err(Error::InvalidState);
        }

        // Settle current balances
        Self::settle_multi_token_grant(&mut grant, env.ledger().timestamp())?;

        // Update flow rates
        for update in token_updates.iter() {
            Self::update_token_flow_rate(&mut grant, &update.token_address, update.flow_rate)?;
        }

        grant.rate_updated_at = env.ledger().timestamp();

        // Update grant in storage
        Self::write_multi_token_grant(&env, grant_id, &grant);

        // Emit rate update event
        env.events().publish(
            (symbol_short!("multi_rateupdt"), grant_id),
            (token_updates.len(), grant.rate_updated_at),
        );
        emit_multi_token_snapshot(&env, grant_id, &grant);

        Ok(())
    }

    /// Add a new token to an existing grant
    pub fn add_token_to_grant(
        env: Env,
        grant_id: u64,
        token_balance: TokenBalance,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;

        let mut grant = Self::read_multi_token_grant(&env, grant_id)?;

        // Check if token already exists
        for existing_token in grant.tokens.iter() {
            if existing_token.token_address == token_balance.token_address {
                return Err(Error::GrantAlreadyExists); // Duplicate token
            }
        }

        // Validate token balance
        if token_balance.total_amount <= 0 || token_balance.flow_rate < 0 {
            return Err(Error::InvalidAmount);
        }

        // Add new token
        grant.tokens.push_back(token_balance);

        // Update grant in storage
        Self::write_multi_token_grant(&env, grant_id, &grant);

        // Emit token addition event
        env.events().publish(
            (symbol_short!("token_added"), grant_id),
            (grant.tokens.len(), env.ledger().timestamp()),
        );

        Ok(())
    }

    /// Remove a token from an existing grant
    pub fn remove_token_from_grant(
        env: Env,
        grant_id: u64,
        token_address: Address,
    ) -> Result<(), Error> {
        require_admin_auth(&env)?;

        let mut grant = Self::read_multi_token_grant(&env, grant_id)?;

        // Settle balances first
        Self::settle_multi_token_grant(&mut grant, env.ledger().timestamp())?;

        // Find and remove token
        let mut found = false;
        let mut new_tokens = Vec::new(&env);
        
        for token in grant.tokens.iter() {
            if token.token_address != token_address {
                new_tokens.push_back(token.clone());
            } else {
                found = true;
                // Check if token has remaining balance
                if token.claimable > 0 || (token.total_amount - token.withdrawn) > 0 {
                    return Err(Error::InvalidState); // Cannot remove token with remaining balance
                }
            }
        }

        if !found {
            return Err(Error::GrantNotFound); // Token not found
        }

        if new_tokens.is_empty() {
            return Err(Error::InvalidAmount); // Cannot remove last token
        }

        grant.tokens = new_tokens;

        // Update grant in storage
        Self::write_multi_token_grant(&env, grant_id, &grant);

        // Emit token removal event
        env.events().publish(
            (symbol_short!("token_removed"), grant_id),
            (token_address, grant.tokens.len()),
        );

        Ok(())
    }
}

// Helper functions for multi-token operations
impl GrantContract {
    /// Read multi-token grant from storage
    fn read_multi_token_grant(env: &Env, grant_id: u64) -> Result<MultiTokenGrant, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Grant(grant_id))
            .ok_or(Error::GrantNotFound)
    }

    /// Write multi-token grant to storage
    fn write_multi_token_grant(env: &Env, grant_id: u64, grant: &MultiTokenGrant) {
        env.storage().instance().set(&DataKey::Grant(grant_id), grant);
    }

    /// Validate token balances
    fn validate_token_balances(tokens: &Vec<TokenBalance>) -> Result<(), Error> {
        for token in tokens.iter() {
            if token.total_amount <= 0 {
                return Err(Error::InvalidAmount);
            }
            if token.flow_rate < 0 {
                return Err(Error::InvalidRate);
            }
            if token.withdrawn < 0 || token.claimable < 0 {
                return Err(Error::InvalidAmount);
            }
            if token.withdrawn > token.total_amount {
                return Err(Error::InvalidState);
            }
        }
        Ok(())
    }

    /// Check for duplicate tokens
    fn check_duplicate_tokens(tokens: &Vec<TokenBalance>) -> Result<(), Error> {
        let mut seen = Vec::new();
        
        for token in tokens.iter() {
            for existing in seen.iter() {
                if *existing == token.token_address {
                    return Err(Error::GrantAlreadyExists);
                }
            }
            seen.push_back(token.token_address.clone());
        }
        Ok(())
    }

    /// Settle multi-token grant balances
    fn settle_multi_token_grant(grant: &mut MultiTokenGrant, now: u64) -> Result<(), Error> {
        if now < grant.last_update_ts {
            return Err(Error::InvalidState);
        }

        let elapsed = now - grant.last_update_ts;
        grant.last_update_ts = now;

        // Only accrue if grant is active
        if !has_status(grant.status_mask, STATUS_ACTIVE) || elapsed == 0 {
            return Ok(());
        }

        let elapsed_i128 = i128::from(elapsed);

        // Settle each token
        for token_balance in grant.tokens.iter_mut() {
            if token_balance.flow_rate == 0 {
                continue;
            }

            let accrued = token_balance
                .flow_rate
                .checked_mul(elapsed_i128)
                .ok_or(Error::MathOverflow)?;

            let accounted = token_balance
                .withdrawn
                .checked_add(token_balance.claimable)
                .ok_or(Error::MathOverflow)?;

            if accounted > token_balance.total_amount {
                return Err(Error::InvalidState);
            }

            let remaining = token_balance
                .total_amount
                .checked_sub(accounted)
                .ok_or(Error::MathOverflow)?;

            let delta = if accrued > remaining {
                remaining
            } else {
                accrued
            };

            token_balance.claimable = token_balance
                .claimable
                .checked_add(delta)
                .ok_or(Error::MathOverflow)?;
        }

        Ok(())
    }

    /// Process withdrawal for a specific token
    fn process_token_withdrawal(
        grant: &mut MultiTokenGrant,
        withdrawal: &TokenWithdrawal,
    ) -> Result<i128, Error> {
        if withdrawal.amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        // Find the token
        let mut token_index = None;
        for (i, token_balance) in grant.tokens.iter().enumerate() {
            if token_balance.token_address == withdrawal.token_address {
                token_index = Some(i);
                break;
            }
        }

        let token_index = token_index.ok_or(Error::GrantNotFound)?;

        let token_balance = &mut grant.tokens[token_index];

        // Check claimable amount
        if withdrawal.amount > token_balance.claimable {
            return Err(Error::InvalidAmount);
        }

        // Update token balance
        token_balance.claimable = token_balance
            .claimable
            .checked_sub(withdrawal.amount)
            .ok_or(Error::MathOverflow)?;

        token_balance.withdrawn = token_balance
            .withdrawn
            .checked_add(withdrawal.amount)
            .ok_or(Error::MathOverflow)?;

        // In a real implementation, this would transfer the token
        // For now, we'll simulate the transfer
        // TODO: Implement actual token transfer logic

        Ok(withdrawal.amount)
    }

    /// Update flow rate for a specific token
    fn update_token_flow_rate(
        grant: &mut MultiTokenGrant,
        token_address: &Address,
        new_flow_rate: i128,
    ) -> Result<(), Error> {
        if new_flow_rate < 0 {
            return Err(Error::InvalidRate);
        }

        // Find the token
        for token_balance in grant.tokens.iter_mut() {
            if token_balance.token_address == *token_address {
                token_balance.flow_rate = new_flow_rate;
                return Ok(());
            }
        }

        Err(Error::GrantNotFound) // Token not found
    }

    /// Check if all tokens are fully withdrawn
    fn is_grant_fully_withdrawn(grant: &MultiTokenGrant) -> bool {
        for token_balance in grant.tokens.iter() {
            let accounted = token_balance.withdrawn + token_balance.claimable;
            if accounted < token_balance.total_amount {
                return false;
            }
        }
        true
    }
}

// Utility functions for multi-token operations
pub fn create_token_balance(
    env: &Env,
    token_address: Address,
    total_amount: i128,
    flow_rate: i128,
) -> TokenBalance {
    TokenBalance {
        token_address,
        total_amount,
        withdrawn: 0,
        claimable: 0,
        flow_rate,
    }
}

pub fn create_token_withdrawal(
    env: &Env,
    token_address: Address,
    amount: i128,
) -> TokenWithdrawal {
    TokenWithdrawal {
        token_address,
        amount,
    }
}
