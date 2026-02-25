#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env,
};

use super::optimized::{
    GrantContract, Grant, Error, DataKey, read_grant, write_grant, settle_grant,
    STATUS_ACTIVE, STATUS_PAUSED, STATUS_COMPLETED, STATUS_CANCELLED,
    has_status, set_status, clear_status, read_admin,
};

// Additional status flag for self-termination
pub const STATUS_SELF_TERMINATED: u32 = 0b100000000; // Grant was self-terminated by grantee

#[contracterror]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum SelfTerminateError {
    AlreadyTerminated = 11,
    CannotTerminateCompleted = 12,
    CannotTerminateCancelled = 13,
    InsufficientBalance = 14,
    TransferFailed = 15,
}

/// Self-termination result structure
#[derive(Clone, Debug)]
#[contracttype]
pub struct SelfTerminateResult {
    pub grant_id: u64,
    pub final_claimable: i128,
    pub refunded_amount: i128,
    pub terminated_at: u64,
    pub termination_reason: String,
}

/// Grant self-termination implementation
impl GrantContract {
    /// Allow a grantee to gracefully terminate their own grant
    /// 
    /// This function enables grantees to:
    /// 1. Settle their final accrued balance
    /// 2. Return unspent portion to the DAO/admin
    /// 3. Mark the grant as self-terminated
    /// 
    /// # Arguments
    /// * `grant_id` - The ID of the grant to terminate
    /// 
    /// # Returns
    /// * `SelfTerminateResult` - Details about the termination outcome
    /// 
    /// # Errors
    /// * `SelfTerminateError::AlreadyTerminated` - Grant is already terminated
    /// * `SelfTerminateError::CannotTerminateCompleted` - Cannot terminate completed grants
    /// * `SelfTerminateError::CannotTerminateCancelled` - Cannot terminate cancelled grants
    /// * `SelfTerminateError::InsufficientBalance` - Insufficient balance for operations
    /// * `SelfTerminateError::TransferFailed` - Token transfer failed
    pub fn self_terminate(env: Env, grant_id: u64) -> Result<SelfTerminateResult, Error> {
        // Read the grant
        let mut grant = read_grant(&env, grant_id)?;
        
        // Validate grant can be self-terminated
        SelfTerminateResult::validate_termination_eligibility(&grant)?;
        
        // Require grantee authentication
        grant.recipient.require_auth();
        
        // Settle final balance
        settle_grant(&mut grant, env.ledger().timestamp())?;
        
        // Calculate amounts
        let final_claimable = grant.claimable;
        let total_withdrawn = grant.withdrawn;
        let total_accounted = total_withdrawn + final_claimable;
        let remaining_balance = grant.total_amount - total_accounted;
        
        // Settle final claimable amount to grantee
        if final_claimable > 0 {
            SelfTerminateResult::transfer_to_grantee(&env, &grant, final_claimable)?;
        }
        
        // Refund remaining balance to admin
        if remaining_balance > 0 {
            SelfTerminateResult::refund_to_admin(&env, remaining_balance)?;
        }
        
        // Update grant status
        grant.status_mask = set_status(grant.status_mask, STATUS_SELF_TERMINATED);
        grant.status_mask = clear_status(grant.status_mask, STATUS_ACTIVE);
        grant.status_mask = clear_status(grant.status_mask, STATUS_PAUSED);
        grant.flow_rate = 0; // Stop further accrual
        
        // Update grant in storage
        write_grant(&env, grant_id, &grant);
        
        // Create termination result
        let result = SelfTerminateResult {
            grant_id,
            final_claimable,
            refunded_amount: remaining_balance,
            terminated_at: env.ledger().timestamp(),
            termination_reason: "Self-terminated by grantee".to_string(),
        };
        
        // Emit termination event
        env.events().publish(
            (symbol_short!("selfterm"), grant_id),
            (
                result.final_claimable,
                result.refunded_amount,
                result.terminated_at,
                result.termination_reason.clone(),
            ),
        );
        
        Ok(result)
    }
    
    /// Get termination details for a grant
    /// 
    /// # Arguments
    /// * `grant_id` - The ID of the grant
    /// 
    /// # Returns
    /// * `SelfTerminateResult` - Termination details if terminated, error otherwise
    pub fn get_termination_details(env: Env, grant_id: u64) -> Result<SelfTerminateResult, Error> {
        let grant = read_grant(&env, grant_id)?;
        
        if !has_status(grant.status_mask, STATUS_SELF_TERMINATED) {
            return Err(Error::InvalidState);
        }
        
        let total_accounted = grant.withdrawn + grant.claimable;
        let remaining_balance = grant.total_amount - total_accounted;
        
        Ok(SelfTerminateResult {
            grant_id,
            final_claimable: grant.claimable,
            refunded_amount: remaining_balance,
            terminated_at: grant.rate_updated_at, // Use as approximation
            termination_reason: "Self-terminated by grantee".to_string(),
        })
    }
    
    /// Check if a grant can be self-terminated
    /// 
    /// # Arguments
    /// * `grant_id` - The ID of the grant
    /// 
    /// # Returns
    /// * `bool` - True if the grant can be self-terminated
    pub fn can_self_terminate(env: Env, grant_id: u64) -> Result<bool, Error> {
        let grant = read_grant(&env, grant_id)?;
        
        // Grant can be self-terminated if:
        // 1. It's active or paused
        // 2. It hasn't been completed, cancelled, or self-terminated
        let can_terminate = (has_status(grant.status_mask, STATUS_ACTIVE) || 
                           has_status(grant.status_mask, STATUS_PAUSED)) &&
                          !has_status(grant.status_mask, STATUS_COMPLETED) &&
                          !has_status(grant.status_mask, STATUS_CANCELLED) &&
                          !has_status(grant.status_mask, STATUS_SELF_TERMINATED);
        
        Ok(can_terminate)
    }
}

impl SelfTerminateResult {
    /// Validate that a grant can be self-terminated
    fn validate_termination_eligibility(grant: &Grant) -> Result<(), Error> {
        // Check if already terminated
        if has_status(grant.status_mask, STATUS_SELF_TERMINATED) {
            return Err(Error::InvalidStatusTransition);
        }
        
        // Check if completed
        if has_status(grant.status_mask, STATUS_COMPLETED) {
            return Err(Error::InvalidStatusTransition);
        }
        
        // Check if cancelled
        if has_status(grant.status_mask, STATUS_CANCELLED) {
            return Err(Error::InvalidStatusTransition);
        }
        
        // Must be active or paused
        if !has_status(grant.status_mask, STATUS_ACTIVE) && 
           !has_status(grant.status_mask, STATUS_PAUSED) {
            return Err(Error::InvalidState);
        }
        
        Ok(())
    }
    
    /// Transfer final claimable amount to grantee
    fn transfer_to_grantee(env: &Env, grant: &Grant, amount: i128) -> Result<(), Error> {
        if amount <= 0 {
            return Ok(()); // No transfer needed
        }
        
        // In a real implementation, this would transfer tokens
        // For now, we'll simulate the transfer
        // TODO: Implement actual token transfer logic
        
        env.events().publish(
            (symbol_short!("grantee_settle"), grant.recipient.clone()),
            (amount, "Final claimable amount settled"),
        );
        
        Ok(())
    }
    
    /// Refund remaining balance to admin
    fn refund_to_admin(env: &Env, amount: i128) -> Result<(), Error> {
        if amount <= 0 {
            return Ok(()); // No refund needed
        }
        
        let admin = read_admin(env)?;
        
        // In a real implementation, this would transfer tokens to admin
        // For now, we'll simulate the transfer
        // TODO: Implement actual token transfer logic
        
        env.events().publish(
            (symbol_short!("admin_refund"), admin),
            (amount, "Unspent grant balance refunded"),
        );
        
        Ok(())
    }
}

// Extension of status validation for self-termination
pub fn validate_self_terminate_transition(current_mask: u32, new_mask: u32) -> Result<(), Error> {
    // Can only transition to self-terminated from active or paused states
    if has_status(new_mask, STATUS_SELF_TERMINATED) {
        if !has_status(current_mask, STATUS_ACTIVE) && 
           !has_status(current_mask, STATUS_PAUSED) {
            return Err(Error::InvalidStatusTransition);
        }
        
        // Cannot be self-terminated if already completed, cancelled, or self-terminated
        if has_status(current_mask, STATUS_COMPLETED) || 
           has_status(current_mask, STATUS_CANCELLED) ||
           has_status(current_mask, STATUS_SELF_TERMINATED) {
            return Err(Error::InvalidStatusTransition);
        }
    }
    
    Ok(())
}

// Helper functions for self-termination status checks
pub fn is_self_terminated(status_mask: u32) -> bool {
    has_status(status_mask, STATUS_SELF_TERMINATED)
}

pub fn can_be_self_terminated(status_mask: u32) -> bool {
    (has_status(status_mask, STATUS_ACTIVE) || has_status(status_mask, STATUS_PAUSED)) &&
    !has_status(status_mask, STATUS_COMPLETED) &&
    !has_status(status_mask, STATUS_CANCELLED) &&
    !has_status(status_mask, STATUS_SELF_TERMINATED)
}
