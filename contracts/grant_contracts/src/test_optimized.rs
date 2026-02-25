#![cfg(test)]

use soroban_sdk::testutils::{Ledger, LedgerInfo};
use super::optimized::{GrantContract, STATUS_ACTIVE, STATUS_PAUSED, STATUS_COMPLETED, STATUS_CANCELLED};

#[test]
fn test_bitwise_status_operations() {
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
    let contract_id = ledger.contract_id();
    
    // Initialize contract
    GrantContract::initialize(&ledger, &contract_id, admin);
    
    // Test 1: Create grant with active status
    GrantContract::create_grant(
        &ledger,
        &contract_id,
        1u64,
        Address::from_public_key(&[1; 32]),
        1000000i128,
        100i128,
        STATUS_ACTIVE,
    ).unwrap();
    
    // Verify status is active
    assert!(GrantContract::is_grant_active(&ledger, &contract_id, 1u64).unwrap());
    assert!(!GrantContract::is_grant_paused(&ledger, &contract_id, 1u64).unwrap());
    assert!(!GrantContract::is_grant_completed(&ledger, &contract_id, 1u64).unwrap());
    assert!(!GrantContract::is_grant_cancelled(&ledger, &contract_id, 1u64).unwrap());
    
    // Test 2: Pause grant
    GrantContract::pause_grant(&ledger, &contract_id, 1u64).unwrap();
    
    // Verify status is paused
    assert!(!GrantContract::is_grant_active(&ledger, &contract_id, 1u64).unwrap());
    assert!(GrantContract::is_grant_paused(&ledger, &contract_id, 1u64).unwrap());
    assert!(!GrantContract::is_grant_completed(&ledger, &contract_id, 1u64).unwrap());
    assert!(!GrantContract::is_grant_cancelled(&ledger, &contract_id, 1u64).unwrap());
    
    // Test 3: Resume grant
    GrantContract::resume_grant(&ledger, &contract_id, 1u64).unwrap();
    
    // Verify status is active again
    assert!(GrantContract::is_grant_active(&ledger, &contract_id, 1u64).unwrap());
    assert!(!GrantContract::is_grant_paused(&ledger, &contract_id, 1u64).unwrap());
    
    // Test 4: Cancel grant
    GrantContract::cancel_grant(&ledger, &contract_id, 1u64).unwrap();
    
    // Verify status is cancelled
    assert!(!GrantContract::is_grant_active(&ledger, &contract_id, 1u64).unwrap());
    assert!(!GrantContract::is_grant_paused(&ledger, &contract_id, 1u64).unwrap());
    assert!(!GrantContract::is_grant_completed(&ledger, &contract_id, 1u64).unwrap());
    assert!(GrantContract::is_grant_cancelled(&ledger, &contract_id, 1u64).unwrap());
}

#[test]
fn test_multiple_status_flags() {
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
    let contract_id = ledger.contract_id();
    
    // Initialize contract
    GrantContract::initialize(&ledger, &contract_id, admin);
    
    // Create grant with multiple flags
    let initial_flags = STATUS_ACTIVE | super::STATUS_REVOCABLE | super::STATUS_MILESTONE_BASED;
    GrantContract::create_grant(
        &ledger,
        &contract_id,
        2u64,
        Address::from_public_key(&[2; 32]),
        1000000i128,
        100i128,
        initial_flags,
    ).unwrap();
    
    // Verify multiple flags are set
    assert!(GrantContract::is_grant_active(&ledger, &contract_id, 2u64).unwrap());
    
    // Check individual flags using bitwise operations
    let status = GrantContract::get_grant_status(&ledger, &contract_id, 2u64).unwrap();
    assert!(status & STATUS_ACTIVE != 0);
    assert!(status & super::STATUS_REVOCABLE != 0);
    assert!(status & super::STATUS_MILESTONE_BASED != 0);
    
    // Test flag manipulation
    GrantContract::set_grant_flags(
        &ledger,
        &contract_id,
        2u64,
        STATUS_PAUSED,  // Add paused flag
        STATUS_ACTIVE, // Remove active flag
    ).unwrap();
    
    // Verify flag changes
    assert!(!GrantContract::is_grant_active(&ledger, &contract_id, 2u64).unwrap());
    assert!(GrantContract::is_grant_paused(&ledger, &contract_id, 2u64).unwrap());
    
    let updated_status = GrantContract::get_grant_status(&ledger, &contract_id, 2u64).unwrap();
    assert!(updated_status & STATUS_PAUSED != 0);
    assert!(updated_status & STATUS_ACTIVE == 0);
    assert!(updated_status & super::STATUS_REVOCABLE != 0); // Should still be set
}

#[test]
fn test_status_transition_validation() {
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
    let contract_id = ledger.contract_id();
    
    // Initialize contract
    GrantContract::initialize(&ledger, &contract_id, admin);
    
    // Create active grant
    GrantContract::create_grant(
        &ledger,
        &contract_id,
        3u64,
        Address::from_public_key(&[3; 32]),
        1000000i128,
        100i128,
        STATUS_ACTIVE,
    ).unwrap();
    
    // Test valid transition: Active -> Paused
    GrantContract::pause_grant(&ledger, &contract_id, 3u64).unwrap();
    
    // Test valid transition: Paused -> Active
    GrantContract::resume_grant(&ledger, &contract_id, 3u64).unwrap();
    
    // Test valid transition: Active -> Completed (via withdrawal)
    let recipient = Address::from_public_key(&[3; 32]);
    ledger.set_source_account(&recipient);
    GrantContract::withdraw(&ledger, &contract_id, 3u64, 1000000i128).unwrap();
    
    // Verify completed status
    assert!(GrantContract::is_grant_completed(&ledger, &contract_id, 3u64).unwrap());
    
    // Test invalid transition: Completed -> Active (should fail)
    let result = GrantContract::pause_grant(&ledger, &contract_id, 3u64);
    assert!(result.is_err());
}

#[test]
fn test_gas_efficiency() {
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
    let contract_id = ledger.contract_id();
    
    // Initialize contract
    GrantContract::initialize(&ledger, &contract_id, admin);
    
    // Measure gas for creating 100 grants
    let before_gas = ledger.get_gas();
    for i in 1..=100u64 {
        GrantContract::create_grant(
            &ledger,
            &contract_id,
            i,
            Address::from_public_key(&[i as u8; 32]),
            1000000i128,
            100i128,
            STATUS_ACTIVE,
        ).unwrap();
    }
    let after_gas = ledger.get_gas();
    let gas_per_grant = (before_gas - after_gas) / 100;
    
    // Gas per grant should be significantly lower than traditional approach
    // Traditional approach would require multiple storage entries per grant
    // Optimized approach uses single storage entry with bitwise flags
    assert!(gas_per_grant < 50000, "Gas per grant should be under 50,000 units");
    
    // Measure gas for 1000 status checks
    let before_gas = ledger.get_gas();
    for i in 1..=1000u64 {
        let _ = GrantContract::is_grant_active(&ledger, &contract_id, i % 100 + 1).unwrap();
    }
    let after_gas = ledger.get_gas();
    let gas_per_status_check = (before_gas - after_gas) / 1000;
    
    // Status checks should be very efficient with bitwise operations
    assert!(gas_per_status_check < 5000, "Gas per status check should be under 5,000 units");
}

#[test]
fn test_large_scale_simulation() {
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
    let contract_id = ledger.contract_id();
    
    // Initialize contract
    GrantContract::initialize(&ledger, &contract_id, admin);
    
    // Simulate large-scale deployment: 10,000 grants
    let before_gas = ledger.get_gas();
    for i in 1..=10000u64 {
        GrantContract::create_grant(
            &ledger,
            &contract_id,
            i,
            Address::from_public_key(&[(i % 255) as u8; 32]),
            1000000i128,
            100i128,
            STATUS_ACTIVE,
        ).unwrap();
    }
    let after_gas = ledger.get_gas();
    let total_gas = before_gas - after_gas;
    
    // Calculate efficiency metrics
    let gas_per_grant = total_gas / 10000;
    let estimated_storage_cost_per_grant = 10000; // Estimated storage cost
    let estimated_total_cost = total_gas + (estimated_storage_cost_per_grant * 10000);
    
    // Assertions for large-scale efficiency
    assert!(gas_per_grant < 45000, "Gas per grant should scale efficiently");
    assert!(total_gas < 450_000_000, "Total gas should be reasonable for 10k grants");
    
    // Simulate batch operations
    let before_batch_gas = ledger.get_gas();
    for i in 1..=1000u64 {
        // Batch status updates
        GrantContract::set_grant_flags(
            &ledger,
            &contract_id,
            i,
            STATUS_PAUSED,
            0,
        ).unwrap();
    }
    let after_batch_gas = ledger.get_gas();
    let batch_gas_per_operation = (before_batch_gas - after_batch_gas) / 1000;
    
    assert!(batch_gas_per_operation < 8000, "Batch operations should be efficient");
}

#[test]
fn test_storage_optimization() {
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
    let contract_id = ledger.contract_id();
    
    // Initialize contract
    GrantContract::initialize(&ledger, &contract_id, admin);
    
    // Create grant with all possible flags set
    let all_flags = STATUS_ACTIVE | STATUS_PAUSED | STATUS_COMPLETED | 
                    STATUS_CANCELLED | super::STATUS_REVOCABLE | 
                    super::STATUS_MILESTONE_BASED | super::STATUS_AUTO_RENEW | 
                    super::STATUS_EMERGENCY_PAUSE;
    
    GrantContract::create_grant(
        &ledger,
        &contract_id,
        1u64,
        Address::from_public_key(&[1; 32]),
        1000000i128,
        100i128,
        all_flags,
    ).unwrap();
    
    // Verify all flags can be read independently
    let status = GrantContract::get_grant_status(&ledger, &contract_id, 1u64).unwrap();
    
    // Each flag should be independently checkable
    assert_eq!(status & STATUS_ACTIVE, STATUS_ACTIVE);
    assert_eq!(status & STATUS_PAUSED, STATUS_PAUSED);
    assert_eq!(status & STATUS_COMPLETED, STATUS_COMPLETED);
    assert_eq!(status & STATUS_CANCELLED, STATUS_CANCELLED);
    assert_eq!(status & super::STATUS_REVOCABLE, super::STATUS_REVOCABLE);
    assert_eq!(status & super::STATUS_MILESTONE_BASED, super::STATUS_MILESTONE_BASED);
    assert_eq!(status & super::STATUS_AUTO_RENEW, super::STATUS_AUTO_RENEW);
    assert_eq!(status & super::STATUS_EMERGENCY_PAUSE, super::STATUS_EMERGENCY_PAUSE);
    
    // Storage should use only one u32 instead of multiple booleans
    // This represents significant storage cost savings
    let grant = GrantContract::get_grant(&ledger, &contract_id, 1u64).unwrap();
    assert_eq!(grant.status_mask, all_flags);
}
