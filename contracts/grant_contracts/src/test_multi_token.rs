#![cfg(test)]

use soroban_sdk::testutils::{Ledger, LedgerInfo};
use super::multi_token::{
    GrantContract, TokenBalance, TokenWithdrawal, MultiTokenWithdrawResult,
    MultiTokenGrant, MultiTokenError, create_token_balance, create_token_withdrawal,
};

#[test]
fn test_create_multi_token_grant() {
    let ledger_info = LedgerInfo {
        protocol_version: 20,
        sequence_number: 12345,
        timestamp: 1620000000,
        network_id: 1,
        base_reserve: 10,
        min_persistent_entry_fee: 100,
        min_temp_entry_fee: 100,
    };
    
    let ledger = Ledger::with_info(&ledger_info);
    let admin = Address::from_public_key(&[0; 32]);
    let recipient = Address::from_public_key(&[1; 32]);
    let token1 = Address::from_public_key(&[2; 32]);
    let token2 = Address::from_public_key(&[3; 32]);
    let contract_id = ledger.contract_id();
    
    // Initialize contract
    GrantContract::initialize(&ledger, &contract_id, admin).unwrap();
    
    // Create token balances
    let mut tokens = Vec::new(&ledger);
    tokens.push_back(create_token_balance(&ledger, token1, 1000000i128, 100i128));
    tokens.push_back(create_token_balance(&ledger, token2, 500000i128, 50i128));
    
    // Create multi-token grant
    GrantContract::create_multi_token_grant(
        &ledger,
        &contract_id,
        1u64,
        recipient,
        tokens,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    // Verify grant was created
    let grant = GrantContract::get_multi_token_grant(&ledger, &contract_id, 1u64).unwrap();
    assert_eq!(grant.recipient, recipient);
    assert_eq!(grant.tokens.len(), 2);
    assert_eq!(grant.tokens.get_unchecked(0).token_address, token1);
    assert_eq!(grant.tokens.get_unchecked(1).token_address, token2);
}

#[test]
fn test_multi_token_withdraw() {
    let ledger_info = LedgerInfo {
        protocol_version: 20,
        sequence_number: 12345,
        timestamp: 1620000000,
        network_id: 1,
        base_reserve: 10,
        min_persistent_entry_fee: 100,
        min_temp_entry_fee: 100,
    };
    
    let ledger = Ledger::with_info(&ledger_info);
    let admin = Address::from_public_key(&[0; 32]);
    let recipient = Address::from_public_key(&[1; 32]);
    let token1 = Address::from_public_key(&[2; 32]);
    let token2 = Address::from_public_key(&[3; 32]);
    let contract_id = ledger.contract_id();
    
    // Initialize contract
    GrantContract::initialize(&ledger, &contract_id, admin).unwrap();
    
    // Create token balances
    let mut tokens = Vec::new(&ledger);
    tokens.push_back(create_token_balance(&ledger, token1, 1000000i128, 100i128));
    tokens.push_back(create_token_balance(&ledger, token2, 500000i128, 50i128));
    
    // Create multi-token grant
    GrantContract::create_multi_token_grant(
        &ledger,
        &contract_id,
        2u64,
        recipient.clone(),
        tokens,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    // Advance time to accrue some balance
    ledger.set_timestamp(1620100000); // 100,000 seconds later
    
    // Create withdrawal requests
    let mut withdrawals = Vec::new(&ledger);
    withdrawals.push_back(create_token_withdrawal(&ledger, token1, 5000i128));
    withdrawals.push_back(create_token_withdrawal(&ledger, token2, 2500i128));
    
    // Set source to recipient
    ledger.set_source_account(&recipient);
    
    // Execute multi-token withdrawal
    let result = GrantContract::multi_token_withdraw(&ledger, &contract_id, 2u64, withdrawals).unwrap();
    
    // Verify withdrawal results
    assert_eq!(result.grant_id, 2u64);
    assert_eq!(result.successful_withdrawals.len(), 2);
    assert_eq!(result.failed_withdrawals.len(), 0);
    assert_eq!(result.total_withdrawn.get(token1), Some(5000i128));
    assert_eq!(result.total_withdrawn.get(token2), Some(2500i128));
}

#[test]
fn test_multi_token_partial_withdrawal_failure() {
    let ledger_info = LedgerInfo {
        protocol_version: 20,
        sequence_number: 12345,
        timestamp: 1620000000,
        network_id: 1,
        base_reserve: 10,
        min_persistent_entry_fee: 100,
        min_temp_entry_fee: 100,
    };
    
    let ledger = Ledger::with_info(&ledger_info);
    let admin = Address::from_public_key(&[0; 32]);
    let recipient = Address::from_public_key(&[1; 32]);
    let token1 = Address::from_public_key(&[2; 32]);
    let token2 = Address::from_public_key(&[3; 32]);
    let contract_id = ledger.contract_id();
    
    // Initialize contract
    GrantContract::initialize(&ledger, &contract_id, admin).unwrap();
    
    // Create token balances
    let mut tokens = Vec::new(&ledger);
    tokens.push_back(create_token_balance(&ledger, token1, 1000i128, 1i128)); // Small amount
    tokens.push_back(create_token_balance(&ledger, token2, 1000i128, 1i128));
    
    // Create multi-token grant
    GrantContract::create_multi_token_grant(
        &ledger,
        &contract_id,
        3u64,
        recipient.clone(),
        tokens,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    // Advance time to accrue some balance
    ledger.set_timestamp(1620100000); // 100,000 seconds later
    
    // Create withdrawal requests (one too large)
    let mut withdrawals = Vec::new(&ledger);
    withdrawals.push_back(create_token_withdrawal(&ledger, token1, 50000i128)); // Too large
    withdrawals.push_back(create_token_withdrawal(&ledger, token2, 5000i128));  // Valid
    
    // Set source to recipient
    ledger.set_source_account(&recipient);
    
    // Execute multi-token withdrawal
    let result = GrantContract::multi_token_withdraw(&ledger, &contract_id, 3u64, withdrawals).unwrap();
    
    // Verify partial failure
    assert_eq!(result.grant_id, 3u64);
    assert_eq!(result.successful_withdrawals.len(), 1);
    assert_eq!(result.failed_withdrawals.len(), 1);
    assert_eq!(result.total_withdrawn.get(token2), Some(5000i128));
    assert_eq!(result.total_withdrawn.get(token1), None);
}

#[test]
fn test_get_token_claimable() {
    let ledger_info = LedgerInfo {
        protocol_version: 20,
        sequence_number: 12345,
        timestamp: 1620000000,
        network_id: 1,
        base_reserve: 10,
        min_persistent_entry_fee: 100,
        min_temp_entry_fee: 100,
    };
    
    let ledger = Ledger::with_info(&ledger_info);
    let admin = Address::from_public_key(&[0; 32]);
    let recipient = Address::from_public_key(&[1; 32]);
    let token1 = Address::from_public_key(&[2; 32]);
    let contract_id = ledger.contract_id();
    
    // Initialize contract
    GrantContract::initialize(&ledger, &contract_id, admin).unwrap();
    
    // Create token balance
    let mut tokens = Vec::new(&ledger);
    tokens.push_back(create_token_balance(&ledger, token1, 1000000i128, 100i128));
    
    // Create multi-token grant
    GrantContract::create_multi_token_grant(
        &ledger,
        &contract_id,
        4u64,
        recipient,
        tokens,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    // Advance time to accrue balance
    ledger.set_timestamp(1620100000); // 100,000 seconds later
    
    // Get claimable amount
    let claimable = GrantContract::get_token_claimable(&ledger, &contract_id, 4u64, token1).unwrap();
    assert_eq!(claimable, 10000000i128); // 100 * 100,000
}

#[test]
fn test_update_multi_token_rates() {
    let ledger_info = LedgerInfo {
        protocol_version: 20,
        sequence_number: 12345,
        timestamp: 1620000000,
        network_id: 1,
        base_reserve: 10,
        min_persistent_entry_fee: 100,
        min_temp_entry_fee: 100,
    };
    
    let ledger = Ledger::with_info(&ledger_info);
    let admin = Address::from_public_key(&[0; 32]);
    let recipient = Address::from_public_key(&[1; 32]);
    let token1 = Address::from_public_key(&[2; 32]);
    let token2 = Address::from_public_key(&[3; 32]);
    let contract_id = ledger.contract_id();
    
    // Initialize contract
    GrantContract::initialize(&ledger, &contract_id, admin).unwrap();
    
    // Create token balances
    let mut tokens = Vec::new(&ledger);
    tokens.push_back(create_token_balance(&ledger, token1, 1000000i128, 100i128));
    tokens.push_back(create_token_balance(&ledger, token2, 500000i128, 50i128));
    
    // Create multi-token grant
    GrantContract::create_multi_token_grant(
        &ledger,
        &contract_id,
        5u64,
        recipient,
        tokens,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    // Create rate updates
    let mut token_updates = Vec::new(&ledger);
    token_updates.push_back(create_token_balance(&ledger, token1, 0i128, 200i128)); // New rate
    token_updates.push_back(create_token_balance(&ledger, token2, 0i128, 100i128)); // New rate
    
    // Update rates (requires admin)
    GrantContract::update_multi_token_rates(&ledger, &contract_id, 5u64, token_updates).unwrap();
    
    // Verify rates were updated
    let grant = GrantContract::get_multi_token_grant(&ledger, &contract_id, 5u64).unwrap();
    assert_eq!(grant.tokens.get_unchecked(0).flow_rate, 200i128);
    assert_eq!(grant.tokens.get_unchecked(1).flow_rate, 100i128);
}

#[test]
fn test_add_token_to_grant() {
    let ledger_info = LedgerInfo {
        protocol_version: 20,
        sequence_number: 12345,
        timestamp: 1620000000,
        network_id: 1,
        base_reserve: 10,
        min_persistent_entry_fee: 100,
        min_temp_entry_fee: 100,
    };
    
    let ledger = Ledger::with_info(&ledger_info);
    let admin = Address::from_public_key(&[0; 32]);
    let recipient = Address::from_public_key(&[1; 32]);
    let token1 = Address::from_public_key(&[2; 32]);
    let token2 = Address::from_public_key(&[3; 32]);
    let contract_id = ledger.contract_id();
    
    // Initialize contract
    GrantContract::initialize(&ledger, &contract_id, admin).unwrap();
    
    // Create initial token balance
    let mut tokens = Vec::new(&ledger);
    tokens.push_back(create_token_balance(&ledger, token1, 1000000i128, 100i128));
    
    // Create multi-token grant
    GrantContract::create_multi_token_grant(
        &ledger,
        &contract_id,
        6u64,
        recipient,
        tokens,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    // Add new token
    let new_token = create_token_balance(&ledger, token2, 500000i128, 50i128);
    GrantContract::add_token_to_grant(&ledger, &contract_id, 6u64, new_token).unwrap();
    
    // Verify token was added
    let grant = GrantContract::get_multi_token_grant(&ledger, &contract_id, 6u64).unwrap();
    assert_eq!(grant.tokens.len(), 2);
    assert_eq!(grant.tokens.get_unchecked(1).token_address, token2);
}

#[test]
fn test_remove_token_from_grant() {
    let ledger_info = LedgerInfo {
        protocol_version: 20,
        sequence_number: 12345,
        timestamp: 1620000000,
        network_id: 1,
        base_reserve: 10,
        min_persistent_entry_fee: 100,
        min_temp_entry_fee: 100,
    };
    
    let ledger = Ledger::with_info(&ledger_info);
    let admin = Address::from_public_key(&[0; 32]);
    let recipient = Address::from_public_key(&[1; 32]);
    let token1 = Address::from_public_key(&[2; 32]);
    let token2 = Address::from_public_key(&[3; 32]);
    let contract_id = ledger.contract_id();
    
    // Initialize contract
    GrantContract::initialize(&ledger, &contract_id, admin).unwrap();
    
    // Create token balances
    let mut tokens = Vec::new(&ledger);
    tokens.push_back(create_token_balance(&ledger, token1, 1000000i128, 100i128));
    tokens.push_back(create_token_balance(&ledger, token2, 500000i128, 50i128));
    
    // Create multi-token grant
    GrantContract::create_multi_token_grant(
        &ledger,
        &contract_id,
        7u64,
        recipient,
        tokens,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    // Remove token2
    GrantContract::remove_token_from_grant(&ledger, &contract_id, 7u64, token2).unwrap();
    
    // Verify token was removed
    let grant = GrantContract::get_multi_token_grant(&ledger, &contract_id, 7u64).unwrap();
    assert_eq!(grant.tokens.len(), 1);
    assert_eq!(grant.tokens.get_unchecked(0).token_address, token1);
}

#[test]
fn test_multi_token_grant_completion() {
    let ledger_info = LedgerInfo {
        protocol_version: 20,
        sequence_number: 12345,
        timestamp: 1620000000,
        network_id: 1,
        base_reserve: 10,
        min_persistent_entry_fee: 100,
        min_temp_entry_fee: 100,
    };
    
    let ledger = Ledger::with_info(&ledger_info);
    let admin = Address::from_public_key(&[0; 32]);
    let recipient = Address::from_public_key(&[1; 32]);
    let token1 = Address::from_public_key(&[2; 32]);
    let token2 = Address::from_public_key(&[3; 32]);
    let contract_id = ledger.contract_id();
    
    // Initialize contract
    GrantContract::initialize(&ledger, &contract_id, admin).unwrap();
    
    // Create small token balances for easy completion
    let mut tokens = Vec::new(&ledger);
    tokens.push_back(create_token_balance(&ledger, token1, 1000i128, 1000i128));
    tokens.push_back(create_token_balance(&ledger, token2, 500i128, 500i128));
    
    // Create multi-token grant
    GrantContract::create_multi_token_grant(
        &ledger,
        &contract_id,
        8u64,
        recipient.clone(),
        tokens,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    // Advance time to accrue full amounts
    ledger.set_timestamp(1620100000); // 100,000 seconds later
    
    // Withdraw full amounts
    let mut withdrawals = Vec::new(&ledger);
    withdrawals.push_back(create_token_withdrawal(&ledger, token1, 100000i128));
    withdrawals.push_back(create_token_withdrawal(&ledger, token2, 50000i128));
    
    // Set source to recipient
    ledger.set_source_account(&recipient);
    
    // Execute withdrawal
    let result = GrantContract::multi_token_withdraw(&ledger, &contract_id, 8u64, withdrawals).unwrap();
    
    // Verify grant is completed
    let grant = GrantContract::get_multi_token_grant(&ledger, &contract_id, 8u64).unwrap();
    assert!(super::optimized::has_status(grant.status_mask, super::optimized::STATUS_COMPLETED));
    assert!(!super::optimized::has_status(grant.status_mask, super::optimized::STATUS_ACTIVE));
    
    // Verify withdrawal result
    assert_eq!(result.successful_withdrawals.len(), 2);
    assert_eq!(result.failed_withdrawals.len(), 0);
}

#[test]
fn test_multi_token_validation_errors() {
    let ledger_info = LedgerInfo {
        protocol_version: 20,
        sequence_number: 12345,
        timestamp: 1620000000,
        network_id: 1,
        base_reserve: 10,
        min_persistent_entry_fee: 100,
        min_temp_entry_fee: 100,
    };
    
    let ledger = Ledger::with_info(&ledger_info);
    let admin = Address::from_public_key(&[0; 32]);
    let recipient = Address::from_public_key(&[1; 32]);
    let token1 = Address::from_public_key(&[2; 32]);
    let contract_id = ledger.contract_id();
    
    // Initialize contract
    GrantContract::initialize(&ledger, &contract_id, admin).unwrap();
    
    // Test empty token list
    let empty_tokens = Vec::new(&ledger);
    let result = GrantContract::create_multi_token_grant(
        &ledger,
        &contract_id,
        9u64,
        recipient,
        empty_tokens,
        super::optimized::STATUS_ACTIVE,
    );
    assert!(result.is_err());
    
    // Test negative amounts
    let mut invalid_tokens = Vec::new(&ledger);
    invalid_tokens.push_back(create_token_balance(&ledger, token1, -1000i128, 100i128));
    let result = GrantContract::create_multi_token_grant(
        &ledger,
        &contract_id,
        10u64,
        recipient,
        invalid_tokens,
        super::optimized::STATUS_ACTIVE,
    );
    assert!(result.is_err());
    
    // Test negative flow rate
    let mut invalid_tokens = Vec::new(&ledger);
    invalid_tokens.push_back(create_token_balance(&ledger, token1, 1000i128, -100i128));
    let result = GrantContract::create_multi_token_grant(
        &ledger,
        &contract_id,
        11u64,
        recipient,
        invalid_tokens,
        super::optimized::STATUS_ACTIVE,
    );
    assert!(result.is_err());
}

#[test]
fn test_duplicate_token_error() {
    let ledger_info = LedgerInfo {
        protocol_version: 20,
        sequence_number: 12345,
        timestamp: 1620000000,
        network_id: 1,
        base_reserve: 10,
        min_persistent_entry_fee: 100,
        min_temp_entry_fee: 100,
    };
    
    let ledger = Ledger::with_info(&ledger_info);
    let admin = Address::from_public_key(&[0; 32]);
    let recipient = Address::from_public_key(&[1; 32]);
    let token1 = Address::from_public_key(&[2; 32]);
    let contract_id = ledger.contract_id();
    
    // Initialize contract
    GrantContract::initialize(&ledger, &contract_id, admin).unwrap();
    
    // Create duplicate tokens
    let mut tokens = Vec::new(&ledger);
    tokens.push_back(create_token_balance(&ledger, token1, 1000000i128, 100i128));
    tokens.push_back(create_token_balance(&ledger, token1, 500000i128, 50i128)); // Duplicate
    
    // Should fail with duplicate token error
    let result = GrantContract::create_multi_token_grant(
        &ledger,
        &contract_id,
        12u64,
        recipient,
        tokens,
        super::optimized::STATUS_ACTIVE,
    );
    assert!(result.is_err());
}

#[test]
fn test_multi_token_gas_efficiency() {
    let ledger_info = LedgerInfo {
        protocol_version: 20,
        sequence_number: 12345,
        timestamp: 1620000000,
        network_id: 1,
        base_reserve: 10,
        min_persistent_entry_fee: 100,
        min_temp_entry_fee: 100,
    };
    
    let ledger = Ledger::with_info(&ledger_info);
    let admin = Address::from_public_key(&[0; 32]);
    let recipient = Address::from_public_key(&[1; 32]);
    let contract_id = ledger.contract_id();
    
    // Initialize contract
    GrantContract::initialize(&ledger, &contract_id, admin).unwrap();
    
    // Create multiple grants with different token counts
    let token_addresses: Vec<Address> = (0..10).map(|i| Address::from_public_key(&[i as u8; 32])).collect();
    
    for i in 1..=5u64 {
        let mut tokens = Vec::new(&ledger);
        for j in 0..(i as usize) {
            tokens.push_back(create_token_balance(&ledger, token_addresses.get(j).unwrap().clone(), 1000000i128, 100i128));
        }
        
        GrantContract::create_multi_token_grant(
            &ledger,
            &contract_id,
            i + 100,
            recipient.clone(),
            tokens,
            super::optimized::STATUS_ACTIVE,
        ).unwrap();
    }
    
    // Measure gas for multi-token withdrawals
    let before_gas = ledger.get_gas();
    for i in 1..=5u64 {
        let mut withdrawals = Vec::new(&ledger);
        for j in 0..(i as usize) {
            withdrawals.push_back(create_token_withdrawal(&ledger, token_addresses.get(j).unwrap().clone(), 1000i128));
        }
        
        ledger.set_source_account(&recipient);
        GrantContract::multi_token_withdraw(&ledger, &contract_id, i + 100, withdrawals).unwrap();
    }
    let after_gas = ledger.get_gas();
    
    // Should be gas efficient even with multiple tokens
    let gas_per_withdrawal = (before_gas - after_gas) / 5;
    assert!(gas_per_withdrawal < 50000, "Gas per multi-token withdrawal should be under 50,000");
}

#[test]
fn test_multi_token_edge_cases() {
    let ledger_info = LedgerInfo {
        protocol_version: 20,
        sequence_number: 12345,
        timestamp: 1620000000,
        network_id: 1,
        base_reserve: 10,
        min_persistent_entry_fee: 100,
        min_temp_entry_fee: 100,
    };
    
    let ledger = Ledger::with_info(&ledger_info);
    let admin = Address::from_public_key(&[0; 32]);
    let recipient = Address::from_public_key(&[1; 32]);
    let token1 = Address::from_public_key(&[2; 32]);
    let contract_id = ledger.contract_id();
    
    // Initialize contract
    GrantContract::initialize(&ledger, &contract_id, admin).unwrap();
    
    // Test zero flow rate
    let mut tokens = Vec::new(&ledger);
    tokens.push_back(create_token_balance(&ledger, token1, 1000000i128, 0i128));
    
    GrantContract::create_multi_token_grant(
        &ledger,
        &contract_id,
        13u64,
        recipient.clone(),
        tokens,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    // Advance time
    ledger.set_timestamp(1620100000);
    
    // Should have zero claimable
    let claimable = GrantContract::get_token_claimable(&ledger, &contract_id, 13u64, token1).unwrap();
    assert_eq!(claimable, 0i128);
    
    // Test withdrawal with zero flow rate (should fail)
    let mut withdrawals = Vec::new(&ledger);
    withdrawals.push_back(create_token_withdrawal(&ledger, token1, 1i128));
    
    ledger.set_source_account(&recipient);
    let result = GrantContract::multi_token_withdraw(&ledger, &contract_id, 13u64, withdrawals).unwrap();
    assert_eq!(result.successful_withdrawals.len(), 0);
    assert_eq!(result.failed_withdrawals.len(), 1);
}
