//! Tests for Governance Activity Monitor
//! 
//! Tests the circuit breaker functionality that enforces mandatory timelocks
//! when admins attempt to change more than 3 protocol parameters in a single ledger.

#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short,
    testutils::{Address as TestAddress, Ledger as TestLedger},
    Address, Env, Bytes, String, Vec,
};

use crate::admin::governance_activity_monitor::{
    GovernanceActivityMonitor, ParameterType, ChangeStatus, 
    ParameterChange, LedgerActivity, MonitorError, MonitorKey
};

#[contract]
struct TestContract;

#[contractimpl]
impl TestContract {
    // Helper functions for testing parameter changes
    pub fn simulate_fee_change(env: Env, admin: Address, old_fee: i128, new_fee: i128) -> u64 {
        let old_bytes = Bytes::from_slice(&env, &old_fee.to_le_bytes());
        let new_bytes = Bytes::from_slice(&env, &new_fee.to_le_bytes());
        let param_name = String::from_str(&env, "protocol_fee");
        let reason = String::from_str(&env, "Adjust protocol fee for sustainability");
        
        GovernanceActivityMonitor::record_parameter_change(
            env,
            admin,
            ParameterType::Fee,
            param_name,
            old_bytes,
            new_bytes,
            reason,
        ).unwrap()
    }

    pub fn simulate_threshold_change(env: Env, admin: Address, old_threshold: u32, new_threshold: u32) -> u64 {
        let old_bytes = Bytes::from_slice(&env, &old_threshold.to_le_bytes());
        let new_bytes = Bytes::from_slice(&env, &new_threshold.to_le_bytes());
        let param_name = String::from_str(&env, "quorum_threshold");
        let reason = String::from_str(&env, "Update quorum requirements");
        
        GovernanceActivityMonitor::record_parameter_change(
            env,
            admin,
            ParameterType::Threshold,
            param_name,
            old_bytes,
            new_bytes,
            reason,
        ).unwrap()
    }

    pub fn simulate_address_change(env: Env, admin: Address, old_address: Address, new_address: Address) -> u64 {
        let old_bytes = old_address.to_xdr(&env);
        let new_bytes = new_address.to_xdr(&env);
        let param_name = String::from_str(&env, "treasury_address");
        let reason = String::from_str(&env, "Update treasury address");
        
        GovernanceActivityMonitor::record_parameter_change(
            env,
            admin,
            ParameterType::Address,
            param_name,
            old_bytes,
            new_bytes,
            reason,
        ).unwrap()
    }
}

#[test]
fn test_initialization() {
    let env = Env::new();
    let admin = Address::generate(&env);
    
    // Test successful initialization
    assert!(GovernanceActivityMonitor::initialize(env.clone(), admin.clone()).is_ok());
    
    // Verify admin is set
    assert_eq!(GovernanceActivityMonitor::get_admin(&env).unwrap(), admin);
    
    // Verify monitor is enabled
    assert!(GovernanceActivityMonitor::is_enabled(&env));
    
    // Test double initialization fails
    let result = GovernanceActivityMonitor::initialize(env.clone(), admin.clone());
    assert_eq!(result.unwrap_err(), MonitorError::NotInitialized);
}

#[test]
fn test_single_parameter_change() {
    let env = Env::new();
    let admin = Address::generate(&env);
    
    // Initialize
    GovernanceActivityMonitor::initialize(env.clone(), admin.clone()).unwrap();
    
    // Record a single parameter change
    let change_id = TestContract::simulate_fee_change(env.clone(), admin.clone(), 100, 150);
    
    // Verify change was recorded
    let change = GovernanceActivityMonitor::get_parameter_change(env.clone(), change_id).unwrap();
    assert_eq!(change.admin, admin);
    assert_eq!(change.parameter_type, ParameterType::Fee);
    assert_eq!(change.status, ChangeStatus::Pending);
    
    // Verify current ledger activity
    let activity = GovernanceActivityMonitor::get_current_ledger_activity(env.clone()).unwrap();
    assert_eq!(activity.change_count, 1);
    assert_eq!(activity.change_ids.len(), 1);
}

#[test]
fn test_circuit_breaker_not_triggered_under_limit() {
    let env = Env::new();
    let admin = Address::generate(&env);
    
    // Initialize
    GovernanceActivityMonitor::initialize(env.clone(), admin.clone()).unwrap();
    
    // Make exactly 3 changes (should not trigger breaker)
    let change1 = TestContract::simulate_fee_change(env.clone(), admin.clone(), 100, 150);
    let change2 = TestContract::simulate_threshold_change(env.clone(), admin.clone(), 50, 60);
    let change3 = TestContract::simulate_address_change(env.clone(), admin.clone(), 
        Address::generate(&env), Address::generate(&env));
    
    // All changes should have normal timelock
    for change_id in [change1, change2, change3] {
        let change = GovernanceActivityMonitor::get_parameter_change(env.clone(), change_id).unwrap();
        assert_eq!(change.status, ChangeStatus::Pending);
    }
    
    // Verify ledger activity
    let activity = GovernanceActivityMonitor::get_current_ledger_activity(env.clone()).unwrap();
    assert_eq!(activity.change_count, 3);
}

#[test]
fn test_circuit_breaker_triggered_on_fourth_change() {
    let env = Env::new();
    let admin = Address::generate(&env);
    
    // Initialize
    GovernanceActivityMonitor::initialize(env.clone(), admin.clone()).unwrap();
    
    // Make 3 changes (normal timelock)
    TestContract::simulate_fee_change(env.clone(), admin.clone(), 100, 150);
    TestContract::simulate_threshold_change(env.clone(), admin.clone(), 50, 60);
    TestContract::simulate_address_change(env.clone(), admin.clone(), 
        Address::generate(&env), Address::generate(&env));
    
    // 4th change should trigger circuit breaker
    let change4 = TestContract::simulate_fee_change(env.clone(), admin.clone(), 150, 200);
    
    let change = GovernanceActivityMonitor::get_parameter_change(env.clone(), change4).unwrap();
    assert_eq!(change.status, ChangeStatus::MandatoryTimelock);
    
    // Verify ledger activity shows breaker triggered
    let activity = GovernanceActivityMonitor::get_current_ledger_activity(env.clone()).unwrap();
    assert_eq!(activity.change_count, 4);
}

#[test]
fn test_mandatory_timelock_duration() {
    let env = Env::new();
    let admin = Address::generate(&env);
    
    // Initialize
    GovernanceActivityMonitor::initialize(env.clone(), admin.clone()).unwrap();
    
    // Make 4 changes to trigger breaker
    TestContract::simulate_fee_change(env.clone(), admin.clone(), 100, 150);
    TestContract::simulate_threshold_change(env.clone(), admin.clone(), 50, 60);
    TestContract::simulate_address_change(env.clone(), admin.clone(), 
        Address::generate(&env), Address::generate(&env));
    let change4 = TestContract::simulate_fee_change(env.clone(), admin.clone(), 150, 200);
    
    let change = GovernanceActivityMonitor::get_parameter_change(env.clone(), change4).unwrap();
    
    // Should have 7-day timelock (604800 seconds)
    let expected_executable = change.proposed_at + 7 * 24 * 60 * 60;
    assert_eq!(change.executable_at, expected_executable);
}

#[test]
fn test_execute_after_timelock() {
    let env = Env::new();
    let admin = Address::generate(&env);
    
    // Initialize
    GovernanceActivityMonitor::initialize(env.clone(), admin.clone()).unwrap();
    
    // Make a change
    let change_id = TestContract::simulate_fee_change(env.clone(), admin.clone(), 100, 150);
    
    // Try to execute immediately (should fail)
    let result = GovernanceActivityMonitor::execute_parameter_change(env.clone(), admin.clone(), change_id);
    assert_eq!(result.unwrap_err(), MonitorError::TimelockNotExpired);
    
    // Advance time past timelock
    env.ledger().set_timestamp(env.ledger().timestamp() + 25 * 60 * 60); // 25 hours
    
    // Should succeed now
    assert!(GovernanceActivityMonitor::execute_parameter_change(env.clone(), admin.clone(), change_id).is_ok());
    
    // Verify status changed
    let change = GovernanceActivityMonitor::get_parameter_change(env.clone(), change_id).unwrap();
    assert_eq!(change.status, ChangeStatus::Executed);
}

#[test]
fn test_cancel_parameter_change() {
    let env = Env::new();
    let admin = Address::generate(&env);
    
    // Initialize
    GovernanceActivityMonitor::initialize(env.clone(), admin.clone()).unwrap();
    
    // Make a change
    let change_id = TestContract::simulate_fee_change(env.clone(), admin.clone(), 100, 150);
    
    // Cancel the change
    assert!(GovernanceActivityMonitor::cancel_parameter_change(env.clone(), admin.clone(), change_id).is_ok());
    
    // Verify status changed
    let change = GovernanceActivityMonitor::get_parameter_change(env.clone(), change_id).unwrap();
    assert_eq!(change.status, ChangeStatus::Cancelled);
}

#[test]
fn test_unauthorized_access() {
    let env = Env::new();
    let admin = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    
    // Initialize
    GovernanceActivityMonitor::initialize(env.clone(), admin.clone()).unwrap();
    
    // Try to execute change as unauthorized user
    let change_id = TestContract::simulate_fee_change(env.clone(), admin.clone(), 100, 150);
    let result = GovernanceActivityMonitor::execute_parameter_change(env.clone(), unauthorized.clone(), change_id);
    assert_eq!(result.unwrap_err(), MonitorError::NotAuthorized);
}

#[test]
fn test_monitor_disable_enable() {
    let env = Env::new();
    let admin = Address::generate(&env);
    
    // Initialize
    GovernanceActivityMonitor::initialize(env.clone(), admin.clone()).unwrap();
    
    // Disable monitor
    assert!(GovernanceActivityMonitor::set_enabled(env.clone(), admin.clone(), false).is_ok());
    assert!(!GovernanceActivityMonitor::is_enabled(&env));
    
    // When disabled, record_parameter_change should return 0 (no tracking)
    let result = TestContract::simulate_fee_change(env.clone(), admin.clone(), 100, 150);
    assert_eq!(result, 0);
    
    // Re-enable monitor
    assert!(GovernanceActivityMonitor::set_enabled(env.clone(), admin.clone(), true).is_ok());
    assert!(GovernanceActivityMonitor::is_enabled(&env));
    
    // Now tracking should work again
    let result = TestContract::simulate_fee_change(env.clone(), admin.clone(), 100, 150);
    assert!(result > 0);
}

#[test]
fn test_configuration_update() {
    let env = Env::new();
    let admin = Address::generate(&env);
    
    // Initialize
    GovernanceActivityMonitor::initialize(env.clone(), admin.clone()).unwrap();
    
    // Update configuration
    assert!(GovernanceActivityMonitor::update_config(
        env.clone(), 
        admin.clone(), 
        Some(5), // Increase max changes to 5
        Some(10 * 24 * 60 * 60) // 10 days timelock
    ).is_ok());
    
    // Make 5 changes (should not trigger with new config)
    for i in 0..5 {
        TestContract::simulate_fee_change(env.clone(), admin.clone(), 100 + i as i128, 150 + i as i128);
    }
    
    // Verify no circuit breaker triggered
    let activity = GovernanceActivityMonitor::get_current_ledger_activity(env.clone()).unwrap();
    assert_eq!(activity.change_count, 5);
    
    // 6th change should trigger
    let change6 = TestContract::simulate_fee_change(env.clone(), admin.clone(), 105, 155);
    let change = GovernanceActivityMonitor::get_parameter_change(env.clone(), change6).unwrap();
    assert_eq!(change.status, ChangeStatus::MandatoryTimelock);
    
    // Verify new timelock duration (10 days)
    let expected_executable = change.proposed_at + 10 * 24 * 60 * 60;
    assert_eq!(change.executable_at, expected_executable);
}

#[test]
fn test_get_pending_changes() {
    let env = Env::new();
    let admin = Address::generate(&env);
    let other_admin = Address::generate(&env);
    
    // Initialize
    GovernanceActivityMonitor::initialize(env.clone(), admin.clone()).unwrap();
    
    // Make changes as different admins
    let change1 = TestContract::simulate_fee_change(env.clone(), admin.clone(), 100, 150);
    let change2 = TestContract::simulate_threshold_change(env.clone(), other_admin.clone(), 50, 60);
    let change3 = TestContract::simulate_fee_change(env.clone(), admin.clone(), 150, 200);
    
    // Get pending changes for first admin
    let pending = GovernanceActivityMonitor::get_pending_changes(env.clone(), admin.clone()).unwrap();
    assert_eq!(pending.len(), 2);
    
    // Verify correct changes are returned
    let change_ids: Vec<u64> = pending.iter().map(|c| c.change_id).collect();
    assert!(change_ids.contains(&change1));
    assert!(change_ids.contains(&change3));
    assert!(!change_ids.contains(&change2));
}

#[test]
fn test_edge_cases() {
    let env = Env::new();
    let admin = Address::generate(&env);
    
    // Initialize
    GovernanceActivityMonitor::initialize(env.clone(), admin.clone()).unwrap();
    
    // Test executing non-existent change
    let result = GovernanceActivityMonitor::execute_parameter_change(env.clone(), admin.clone(), 999);
    assert_eq!(result.unwrap_err(), MonitorError::ChangeNotFound);
    
    // Make a change
    let change_id = TestContract::simulate_fee_change(env.clone(), admin.clone(), 100, 150);
    
    // Execute it
    env.ledger().set_timestamp(env.ledger().timestamp() + 25 * 60 * 60);
    assert!(GovernanceActivityMonitor::execute_parameter_change(env.clone(), admin.clone(), change_id).is_ok());
    
    // Try to execute again
    let result = GovernanceActivityMonitor::execute_parameter_change(env.clone(), admin.clone(), change_id);
    assert_eq!(result.unwrap_err(), MonitorError::AlreadyExecuted);
    
    // Try to cancel executed change
    let result = GovernanceActivityMonitor::cancel_parameter_change(env.clone(), admin.clone(), change_id);
    assert_eq!(result.unwrap_err(), MonitorError::AlreadyExecuted);
}

#[test]
fn test_ledger_boundary() {
    let env = Env::new();
    let admin = Address::generate(&env);
    
    // Initialize
    GovernanceActivityMonitor::initialize(env.clone(), admin.clone()).unwrap();
    
    // Make changes in first ledger
    TestContract::simulate_fee_change(env.clone(), admin.clone(), 100, 150);
    TestContract::simulate_threshold_change(env.clone(), admin.clone(), 50, 60);
    
    let activity1 = GovernanceActivityMonitor::get_current_ledger_activity(env.clone()).unwrap();
    assert_eq!(activity1.change_count, 2);
    
    // Advance to next ledger
    env.ledger().set_sequence(env.ledger().sequence() + 1);
    
    // Make changes in new ledger (should reset counter)
    TestContract::simulate_fee_change(env.clone(), admin.clone(), 150, 200);
    TestContract::simulate_threshold_change(env.clone(), admin.clone(), 60, 70);
    TestContract::simulate_address_change(env.clone(), admin.clone(), 
        Address::generate(&env), Address::generate(&env));
    
    let activity2 = GovernanceActivityMonitor::get_current_ledger_activity(env.clone()).unwrap();
    assert_eq!(activity2.change_count, 3); // Should be reset for new ledger
    assert_ne!(activity2.ledger_number, activity1.ledger_number);
}

#[test]
fn test_comprehensive_workflow() {
    let env = Env::new();
    let admin = Address::generate(&env);
    
    // Initialize
    GovernanceActivityMonitor::initialize(env.clone(), admin.clone()).unwrap();
    
    // Phase 1: Normal operations (under limit)
    let change1 = TestContract::simulate_fee_change(env.clone(), admin.clone(), 100, 120);
    let change2 = TestContract::simulate_threshold_change(env.clone(), admin.clone(), 50, 55);
    
    // Verify normal timelocks
    for change_id in [change1, change2] {
        let change = GovernanceActivityMonitor::get_parameter_change(env.clone(), change_id).unwrap();
        assert_eq!(change.status, ChangeStatus::Pending);
    }
    
    // Phase 2: Trigger circuit breaker
    let change3 = TestContract::simulate_address_change(env.clone(), admin.clone(), 
        Address::generate(&env), Address::generate(&env));
    let change4 = TestContract::simulate_fee_change(env.clone(), admin.clone(), 120, 130); // This triggers
    
    // Verify circuit breaker triggered
    let change = GovernanceActivityMonitor::get_parameter_change(env.clone(), change4).unwrap();
    assert_eq!(change.status, ChangeStatus::MandatoryTimelock);
    
    // Phase 3: Try to execute normal changes
    env.ledger().set_timestamp(env.ledger().timestamp() + 25 * 60 * 60);
    
    assert!(GovernanceActivityMonitor::execute_parameter_change(env.clone(), admin.clone(), change1).is_ok());
    assert!(GovernanceActivityMonitor::execute_parameter_change(env.clone(), admin.clone(), change2).is_ok());
    assert!(GovernanceActivityMonitor::execute_parameter_change(env.clone(), admin.clone(), change3).is_ok());
    
    // Phase 4: Try to execute mandatory timelock change (should fail)
    let result = GovernanceActivityMonitor::execute_parameter_change(env.clone(), admin.clone(), change4);
    assert_eq!(result.unwrap_err(), MonitorError::TimelockNotExpired);
    
    // Phase 5: Advance time and execute mandatory timelock change
    env.ledger().set_timestamp(env.ledger().timestamp() + 7 * 24 * 60 * 60);
    assert!(GovernanceActivityMonitor::execute_parameter_change(env.clone(), admin.clone(), change4).is_ok());
    
    // Verify final state
    let final_change = GovernanceActivityMonitor::get_parameter_change(env.clone(), change4).unwrap();
    assert_eq!(final_change.status, ChangeStatus::Executed);
    
    // Check all changes are executed
    let all_changes = GovernanceActivityMonitor::get_all_changes(env.clone()).unwrap();
    assert_eq!(all_changes.len(), 4);
    for change in all_changes.iter() {
        assert_eq!(change.status, ChangeStatus::Executed);
    }
}

#[test]
fn test_admin_rotation_grace_period() {
    let env = Env::new();
    let admin = Address::generate(&env);
    
    GovernanceActivityMonitor::initialize(env.clone(), admin.clone()).unwrap();
    
    // Simulate admin rotation at ledger 1000
    env.ledger().set_sequence(1000);
    GovernanceActivityMonitor::record_activity(env.clone(), admin.clone());
    
    // Run monitor check at ledger 1001
    env.ledger().set_sequence(1001);
    
    let events_before = env.events().all().len();
    GovernanceActivityMonitor::check_activity(env.clone(), admin.clone());
    let events_after = env.events().all().len();
    
    // Assert no warning emitted
    assert_eq!(events_before, events_after);
}

#[test]
fn test_integration_rapid_rotation() {
    let env = Env::new();
    let mut admin = Address::generate(&env);
    
    GovernanceActivityMonitor::initialize(env.clone(), admin.clone()).unwrap();
    
    let mut ledger = 1000;
    for _ in 0..5 {
        ledger += 10;
        env.ledger().set_sequence(ledger);
        
        let new_admin = Address::generate(&env);
        GovernanceActivityMonitor::record_activity(env.clone(), new_admin.clone());
        admin = new_admin;
        
        let events_before = env.events().all().len();
        GovernanceActivityMonitor::check_activity(env.clone(), admin.clone());
        let events_after = env.events().all().len();
        
        // Assert no false warnings
        assert_eq!(events_before, events_after);
    }
}
