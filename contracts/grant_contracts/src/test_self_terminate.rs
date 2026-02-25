#![cfg(test)]

use soroban_sdk::testutils::{Ledger, LedgerInfo};
use super::self_terminate::{
    GrantContract, SelfTerminateResult, SelfTerminateError,
    STATUS_SELF_TERMINATED, is_self_terminated, can_be_self_terminated,
};

#[test]
fn test_self_terminate_basic() {
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
    
    // Create active grant
    GrantContract::create_grant(
        &ledger,
        &contract_id,
        1u64,
        recipient.clone(),
        1000000i128,
        100i128,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    // Set source to recipient for self-termination
    ledger.set_source_account(&recipient);
    
    // Self-terminate the grant
    let result = GrantContract::self_terminate(&ledger, &contract_id, 1u64).unwrap();
    
    // Verify termination result
    assert_eq!(result.grant_id, 1u64);
    assert_eq!(result.final_claimable, 0i128); // No time passed, so no accrual
    assert_eq!(result.refunded_amount, 1000000i128); // Full amount refunded
    assert_eq!(result.termination_reason, "Self-terminated by grantee");
    
    // Verify grant status is self-terminated
    let grant = GrantContract::get_grant(&ledger, &contract_id, 1u64).unwrap();
    assert!(is_self_terminated(grant.status_mask));
    assert!(!super::optimized::has_status(grant.status_mask, super::optimized::STATUS_ACTIVE));
    assert_eq!(grant.flow_rate, 0i128); // Flow rate stopped
}

#[test]
fn test_self_terminate_with_claimable_balance() {
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
    
    // Create grant and let it accrue some balance
    GrantContract::create_grant(
        &ledger,
        &contract_id,
        2u64,
        recipient.clone(),
        1000000i128,
        100i128,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    // Advance time to accrue some balance
    ledger.set_timestamp(1620100000); // 100,000 seconds later
    
    // Set source to recipient for self-termination
    ledger.set_source_account(&recipient);
    
    // Self-terminate the grant
    let result = GrantContract::self_terminate(&ledger, &contract_id, 2u64).unwrap();
    
    // Verify termination result
    assert_eq!(result.grant_id, 2u64);
    assert!(result.final_claimable > 0i128); // Should have accrued balance
    assert!(result.refunded_amount < 1000000i128); // Less than full amount
    assert_eq!(result.final_claimable + result.refunded_amount, 1000000i128);
    
    // Verify grant status
    let grant = GrantContract::get_grant(&ledger, &contract_id, 2u64).unwrap();
    assert!(is_self_terminated(grant.status_mask));
    assert_eq!(grant.claimable, 0i128); // Claimable should be settled
}

#[test]
fn test_self_terminate_paused_grant() {
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
    GrantContract::initialize(&ledger, &contract_id, admin.clone()).unwrap();
    
    // Create and pause grant
    GrantContract::create_grant(
        &ledger,
        &contract_id,
        3u64,
        recipient.clone(),
        1000000i128,
        100i128,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    // Pause the grant
    GrantContract::pause_grant(&ledger, &contract_id, 3u64).unwrap();
    
    // Set source to recipient for self-termination
    ledger.set_source_account(&recipient);
    
    // Self-terminate the paused grant
    let result = GrantContract::self_terminate(&ledger, &contract_id, 3u64).unwrap();
    
    // Verify termination result
    assert_eq!(result.grant_id, 3u64);
    assert_eq!(result.final_claimable, 0i128); // No accrual while paused
    assert_eq!(result.refunded_amount, 1000000i128); // Full amount refunded
    
    // Verify grant status
    let grant = GrantContract::get_grant(&ledger, &contract_id, 3u64).unwrap();
    assert!(is_self_terminated(grant.status_mask));
}

#[test]
fn test_self_terminate_unauthorized() {
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
    let unauthorized_user = Address::from_public_key(&[2; 32]);
    let contract_id = ledger.contract_id();
    
    // Initialize contract
    GrantContract::initialize(&ledger, &contract_id, admin).unwrap();
    
    // Create grant
    GrantContract::create_grant(
        &ledger,
        &contract_id,
        4u64,
        recipient,
        1000000i128,
        100i128,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    // Set source to unauthorized user
    ledger.set_source_account(&unauthorized_user);
    
    // Attempt self-termination should fail
    let result = GrantContract::self_terminate(&ledger, &contract_id, 4u64);
    assert!(result.is_err());
}

#[test]
fn test_self_terminate_completed_grant() {
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
    GrantContract::initialize(&ledger, &contract_id, admin.clone()).unwrap();
    
    // Create and complete grant
    GrantContract::create_grant(
        &ledger,
        &contract_id,
        5u64,
        recipient.clone(),
        1000i128, // Small amount for easy completion
        1000i128,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    // Complete the grant by withdrawing full amount
    ledger.set_source_account(&recipient);
    GrantContract::withdraw(&ledger, &contract_id, 5u64, 1000i128).unwrap();
    
    // Attempt self-termination should fail
    let result = GrantContract::self_terminate(&ledger, &contract_id, 5u64);
    assert!(result.is_err());
}

#[test]
fn test_self_terminate_cancelled_grant() {
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
    GrantContract::initialize(&ledger, &contract_id, admin.clone()).unwrap();
    
    // Create and cancel grant
    GrantContract::create_grant(
        &ledger,
        &contract_id,
        6u64,
        recipient.clone(),
        1000000i128,
        100i128,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    // Cancel the grant
    GrantContract::cancel_grant(&ledger, &contract_id, 6u64).unwrap();
    
    // Attempt self-termination should fail
    let result = GrantContract::self_terminate(&ledger, &contract_id, 6u64);
    assert!(result.is_err());
}

#[test]
fn test_self_terminate_already_terminated() {
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
    
    // Create and self-terminate grant
    GrantContract::create_grant(
        &ledger,
        &contract_id,
        7u64,
        recipient.clone(),
        1000000i128,
        100i128,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    ledger.set_source_account(&recipient);
    GrantContract::self_terminate(&ledger, &contract_id, 7u64).unwrap();
    
    // Attempt second self-termination should fail
    let result = GrantContract::self_terminate(&ledger, &contract_id, 7u64);
    assert!(result.is_err());
}

#[test]
fn test_can_self_terminate() {
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
    
    // Test active grant
    GrantContract::create_grant(
        &ledger,
        &contract_id,
        8u64,
        recipient,
        1000000i128,
        100i128,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    assert!(GrantContract::can_self_terminate(&ledger, &contract_id, 8u64).unwrap());
    
    // Test paused grant
    GrantContract::create_grant(
        &ledger,
        &contract_id,
        9u64,
        recipient,
        1000000i128,
        100i128,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    GrantContract::pause_grant(&ledger, &contract_id, 9u64).unwrap();
    assert!(GrantContract::can_self_terminate(&ledger, &contract_id, 9u64).unwrap());
    
    // Test completed grant
    GrantContract::create_grant(
        &ledger,
        &contract_id,
        10u64,
        recipient,
        1000i128,
        1000i128,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    ledger.set_source_account(&recipient);
    GrantContract::withdraw(&ledger, &contract_id, 10u64, 1000i128).unwrap();
    assert!(!GrantContract::can_self_terminate(&ledger, &contract_id, 10u64).unwrap());
    
    // Test cancelled grant
    GrantContract::create_grant(
        &ledger,
        &contract_id,
        11u64,
        recipient,
        1000000i128,
        100i128,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    GrantContract::cancel_grant(&ledger, &contract_id, 11u64).unwrap();
    assert!(!GrantContract::can_self_terminate(&ledger, &contract_id, 11u64).unwrap());
}

#[test]
fn test_get_termination_details() {
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
    
    // Create and self-terminate grant
    GrantContract::create_grant(
        &ledger,
        &contract_id,
        12u64,
        recipient.clone(),
        1000000i128,
        100i128,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    ledger.set_source_account(&recipient);
    GrantContract::self_terminate(&ledger, &contract_id, 12u64).unwrap();
    
    // Get termination details
    let details = GrantContract::get_termination_details(&ledger, &contract_id, 12u64).unwrap();
    
    assert_eq!(details.grant_id, 12u64);
    assert_eq!(details.termination_reason, "Self-terminated by grantee");
    assert!(details.terminated_at > 0);
}

#[test]
fn test_get_termination_details_non_terminated() {
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
    
    // Create active grant (not terminated)
    GrantContract::create_grant(
        &ledger,
        &contract_id,
        13u64,
        recipient,
        1000000i128,
        100i128,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    // Getting termination details should fail
    let result = GrantContract::get_termination_details(&ledger, &contract_id, 13u64);
    assert!(result.is_err());
}

#[test]
fn test_self_terminate_gas_efficiency() {
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
    
    // Create multiple grants
    for i in 1..=100u64 {
        GrantContract::create_grant(
            &ledger,
            &contract_id,
            i,
            Address::from_public_key(&[i as u8; 32]),
            1000000i128,
            100i128,
            super::optimized::STATUS_ACTIVE,
        ).unwrap();
    }
    
    // Measure gas for batch self-termination
    let before_gas = ledger.get_gas();
    for i in 1..=100u64 {
        ledger.set_source_account(&Address::from_public_key(&[i as u8; 32]));
        GrantContract::self_terminate(&ledger, &contract_id, i).unwrap();
    }
    let after_gas = ledger.get_gas();
    let gas_per_termination = (before_gas - after_gas) / 100;
    
    // Self-termination should be gas efficient
    assert!(gas_per_termination < 15000, "Gas per self-termination should be under 15,000");
}

#[test]
fn test_self_terminate_edge_cases() {
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
    
    // Test with zero flow rate
    GrantContract::create_grant(
        &ledger,
        &contract_id,
        14u64,
        recipient.clone(),
        1000000i128,
        0i128, // Zero flow rate
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    ledger.set_source_account(&recipient);
    let result = GrantContract::self_terminate(&ledger, &contract_id, 14u64).unwrap();
    
    // Should work with zero flow rate
    assert_eq!(result.final_claimable, 0i128);
    assert_eq!(result.refunded_amount, 1000000i128);
    
    // Test with very small amount
    GrantContract::create_grant(
        &ledger,
        &contract_id,
        15u64,
        recipient.clone(),
        1i128, // Very small amount
        1i128,
        super::optimized::STATUS_ACTIVE,
    ).unwrap();
    
    let result = GrantContract::self_terminate(&ledger, &contract_id, 15u64).unwrap();
    
    // Should handle small amounts correctly
    assert_eq!(result.final_claimable, 0i128);
    assert_eq!(result.refunded_amount, 1i128);
}
