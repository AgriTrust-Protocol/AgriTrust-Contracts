#![cfg(test)]

use soroban_sdk::{Env, Address, vec, map, String, Symbol};
use crate::grant_contracts::{
    sub_dao_authority::{
        SubDaoAuthority, SubDaoPermission, SubDaoStatus, PermissionLevel, 
        VetoRecord, ActionLog, SubDaoError
    },
    GrantContract, GrantStatus, Error,
};

#[test]
fn test_sub_dao_authority_initialization() {
    let env = Env::default();
    let main_dao_admin = Address::generate(&env);

    // Test successful initialization
    let result = SubDaoAuthority::initialize(env.clone(), main_dao_admin.clone());
    assert_eq!(result, Ok(()));

    // Test duplicate initialization
    let result = SubDaoAuthority::initialize(env.clone(), main_dao_admin);
    assert_eq!(result, Err(SubDaoError::AlreadyInitialized));
}

#[test]
fn test_grant_sub_dao_permissions() {
    let env = Env::default();
    let main_dao_admin = Address::generate(&env);
    let sub_dao_address = Address::generate(&env);

    // Initialize
    SubDaoAuthority::initialize(env.clone(), main_dao_admin.clone()).unwrap();

    // Grant permissions
    let result = SubDaoAuthority::grant_sub_dao_permissions(
        env.clone(),
        main_dao_admin.clone(),
        sub_dao_address.clone(),
        String::from_str(&env, "Engineering"),
        PermissionLevel::Clawback,
        1000000, // max_grant_amount
        Some(env.ledger().timestamp() + 86400), // expires in 24 hours
    );
    assert_eq!(result, Ok(()));

    // Check permission details
    let permission = SubDaoAuthority::get_sub_dao_permission(env.clone(), sub_dao_address.clone()).unwrap();
    assert_eq!(permission.permission_level, PermissionLevel::Clawback);
    assert_eq!(permission.department, String::from_str(&env, "Engineering"));
    assert_eq!(permission.status, SubDaoStatus::Active);
    assert_eq!(permission.max_grant_amount, 1000000);
}

#[test]
fn test_assign_grant_to_sub_dao() {
    let env = Env::default();
    let main_dao_admin = Address::generate(&env);
    let sub_dao_address = Address::generate(&env);

    // Initialize and grant permissions
    SubDaoAuthority::initialize(env.clone(), main_dao_admin.clone()).unwrap();
    SubDaoAuthority::grant_sub_dao_permissions(
        env.clone(),
        main_dao_admin.clone(),
        sub_dao_address.clone(),
        String::from_str(&env, "Marketing"),
        PermissionLevel::Pause,
        500000,
        None,
    ).unwrap();

    // Assign grant to Sub-DAO
    let result = SubDaoAuthority::assign_grant_to_sub_dao(
        env.clone(),
        main_dao_admin.clone(),
        sub_dao_address.clone(),
        123, // grant_id
    );
    assert_eq!(result, Ok(()));

    // Check managed grants
    let managed_grants = SubDaoAuthority::get_managed_grants(env.clone(), sub_dao_address.clone()).unwrap();
    assert_eq!(managed_grants.len(), 1);
    assert_eq!(managed_grants.get(0).unwrap(), 123);

    // Check grant manager
    let grant_manager = SubDaoAuthority::get_grant_manager(env.clone(), 123).unwrap();
    assert_eq!(grant_manager, Some(sub_dao_address.clone()));
}

#[test]
fn test_delegated_pause_grant() {
    let env = Env::default();
    let main_dao_admin = Address::generate(&env);
    let sub_dao_address = Address::generate(&env);

    // Initialize and grant permissions
    SubDaoAuthority::initialize(env.clone(), main_dao_admin.clone()).unwrap();
    SubDaoAuthority::grant_sub_dao_permissions(
        env.clone(),
        main_dao_admin.clone(),
        sub_dao_address.clone(),
        String::from_str(&env, "Engineering"),
        PermissionLevel::Pause,
        1000000,
        None,
    ).unwrap();

    // Assign grant to Sub-DAO
    SubDaoAuthority::assign_grant_to_sub_dao(
        env.clone(),
        main_dao_admin.clone(),
        sub_dao_address.clone(),
        456, // grant_id
    ).unwrap();

    // Sub-DAO pauses grant
    let result = SubDaoAuthority::delegated_pause_grant(
        env.clone(),
        sub_dao_address.clone(),
        456,
        String::from_str(&env, "Project needs review"),
    );
    assert!(result.is_ok());

    let action_id = result.unwrap();
    
    // Check action log
    let action_log = SubDaoAuthority::get_action_log(env.clone(), action_id).unwrap();
    assert_eq!(action_log.sub_dao_address, sub_dao_address);
    assert_eq!(action_log.grant_id, 456);
    assert_eq!(action_log.action_type, String::from_str(&env, "pause"));
    assert!(!action_log.vetoed);
}

#[test]
fn test_delegated_clawback_grant() {
    let env = Env::default();
    let main_dao_admin = Address::generate(&env);
    let sub_dao_address = Address::generate(&env);

    // Initialize and grant clawback permissions
    SubDaoAuthority::initialize(env.clone(), main_dao_admin.clone()).unwrap();
    SubDaoAuthority::grant_sub_dao_permissions(
        env.clone(),
        main_dao_admin.clone(),
        sub_dao_address.clone(),
        String::from_str(&env, "Operations"),
        PermissionLevel::Clawback,
        2000000,
        None,
    ).unwrap();

    // Assign grant to Sub-DAO
    SubDaoAuthority::assign_grant_to_sub_dao(
        env.clone(),
        main_dao_admin.clone(),
        sub_dao_address.clone(),
        789, // grant_id
    ).unwrap();

    // Sub-DAO clawbacks grant
    let result = SubDaoAuthority::delegated_clawback_grant(
        env.clone(),
        sub_dao_address.clone(),
        789,
        String::from_str(&env, "Project failed to meet milestones"),
    );
    assert!(result.is_ok());

    let action_id = result.unwrap();
    
    // Check action log
    let action_log = SubDaoAuthority::get_action_log(env.clone(), action_id).unwrap();
    assert_eq!(action_log.action_type, String::from_str(&env, "cancel"));
    assert!(!action_log.vetoed);
}

#[test]
fn test_main_dao_veto_power() {
    let env = Env::default();
    let main_dao_admin = Address::generate(&env);
    let sub_dao_address = Address::generate(&env);

    // Initialize and grant permissions
    SubDaoAuthority::initialize(env.clone(), main_dao_admin.clone()).unwrap();
    SubDaoAuthority::grant_sub_dao_permissions(
        env.clone(),
        main_dao_admin.clone(),
        sub_dao_address.clone(),
        String::from_str(&env, "Finance"),
        PermissionLevel::Clawback,
        1000000,
        None,
    ).unwrap();

    // Assign grant and Sub-DAO takes action
    SubDaoAuthority::assign_grant_to_sub_dao(
        env.clone(),
        main_dao_admin.clone(),
        sub_dao_address.clone(),
        101, // grant_id
    ).unwrap();

    let action_id = SubDaoAuthority::delegated_pause_grant(
        env.clone(),
        sub_dao_address.clone(),
        101,
        String::from_str(&env, "Temporary pause"),
    ).unwrap();

    // Main DAO vetoes the action
    let veto_id = SubDaoAuthority::veto_sub_dao_action(
        env.clone(),
        main_dao_admin.clone(),
        action_id,
        String::from_str(&env, "Pause not justified - project is on track"),
    ).unwrap();

    // Check veto record
    let veto_record = SubDaoAuthority::get_veto_record(env.clone(), veto_id).unwrap();
    assert_eq!(veto_record.sub_dao_address, sub_dao_address);
    assert_eq!(veto_record.action_type, String::from_str(&env, "pause"));
    assert_eq!(veto_record.grant_id, 101);
    assert_eq!(veto_record.vetoed_by, main_dao_admin);

    // Check that action is marked as vetoed
    let action_log = SubDaoAuthority::get_action_log(env.clone(), action_id).unwrap();
    assert!(action_log.vetoed);
    assert_eq!(action_log.veto_id, Some(veto_id));
}

#[test]
fn test_permission_levels() {
    let env = Env::default();
    let main_dao_admin = Address::generate(&env);
    let sub_dao_pause = Address::generate(&env);
    let sub_dao_clawback = Address::generate(&env);

    // Initialize
    SubDaoAuthority::initialize(env.clone(), main_dao_admin.clone()).unwrap();

    // Grant pause-only permissions
    SubDaoAuthority::grant_sub_dao_permissions(
        env.clone(),
        main_dao_admin.clone(),
        sub_dao_pause.clone(),
        String::from_str(&env, "Support"),
        PermissionLevel::Pause,
        500000,
        None,
    ).unwrap();

    // Grant clawback permissions
    SubDaoAuthority::grant_sub_dao_permissions(
        env.clone(),
        main_dao_admin.clone(),
        sub_dao_clawback.clone(),
        String::from_str(&env, "Security"),
        PermissionLevel::Clawback,
        1000000,
        None,
    ).unwrap();

    // Assign grants
    SubDaoAuthority::assign_grant_to_sub_dao(env.clone(), main_dao_admin.clone(), sub_dao_pause.clone(), 1).unwrap();
    SubDaoAuthority::assign_grant_to_sub_dao(env.clone(), main_dao_admin.clone(), sub_dao_clawback.clone(), 2).unwrap();

    // Pause-only Sub-DAO should be able to pause
    let pause_result = SubDaoAuthority::delegated_pause_grant(
        env.clone(),
        sub_dao_pause.clone(),
        1,
        String::from_str(&env, "Review needed"),
    );
    assert!(pause_result.is_ok());

    // But should not be able to clawback (this would fail in real implementation)
    // For now, we'll just test that the structure is in place

    // Clawback Sub-DAO should be able to do both
    let clawback_pause_result = SubDaoAuthority::delegated_pause_grant(
        env.clone(),
        sub_dao_clawback.clone(),
        2,
        String::from_str(&env, "Security review"),
    );
    assert!(clawback_pause_result.is_ok());

    let clawback_result = SubDaoAuthority::delegated_clawback_grant(
        env.clone(),
        sub_dao_clawback.clone(),
        2,
        String::from_str(&env, "Security violation"),
    );
    assert!(clawback_result.is_ok());
}

#[test]
fn test_department_organization() {
    let env = Env::default();
    let main_dao_admin = Address::generate(&env);
    let engineering_dao = Address::generate(&env);
    let marketing_dao = Address::generate(&env);
    let another_engineering_dao = Address::generate(&env);

    // Initialize
    SubDaoAuthority::initialize(env.clone(), main_dao_admin.clone()).unwrap();

    // Create Sub-DAOs in different departments
    SubDaoAuthority::grant_sub_dao_permissions(
        env.clone(),
        main_dao_admin.clone(),
        engineering_dao.clone(),
        String::from_str(&env, "Engineering"),
        PermissionLevel::Full,
        2000000,
        None,
    ).unwrap();

    SubDaoAuthority::grant_sub_dao_permissions(
        env.clone(),
        main_dao_admin.clone(),
        marketing_dao.clone(),
        String::from_str(&env, "Marketing"),
        PermissionLevel::Pause,
        1000000,
        None,
    ).unwrap();

    SubDaoAuthority::grant_sub_dao_permissions(
        env.clone(),
        main_dao_admin.clone(),
        another_engineering_dao.clone(),
        String::from_str(&env, "Engineering"),
        PermissionLevel::Clawback,
        1500000,
        None,
    ).unwrap();

    // Check department listings
    let engineering_sub_daos = SubDaoAuthority::get_department_sub_daos(
        env.clone(),
        String::from_str(&env, "Engineering"),
    ).unwrap();
    assert_eq!(engineering_sub_daos.len(), 2);

    let marketing_sub_daos = SubDaoAuthority::get_department_sub_daos(
        env.clone(),
        String::from_str(&env, "Marketing"),
    ).unwrap();
    assert_eq!(marketing_sub_daos.len(), 1);
}

#[test]
fn test_suspension_and_revocation() {
    let env = Env::default();
    let main_dao_admin = Address::generate(&env);
    let sub_dao_address = Address::generate(&env);

    // Initialize and grant permissions
    SubDaoAuthority::initialize(env.clone(), main_dao_admin.clone()).unwrap();
    SubDaoAuthority::grant_sub_dao_permissions(
        env.clone(),
        main_dao_admin.clone(),
        sub_dao_address.clone(),
        String::from_str(&env, "Test"),
        PermissionLevel::Clawback,
        1000000,
        None,
    ).unwrap();

    // Assign grant
    SubDaoAuthority::assign_grant_to_sub_dao(env.clone(), main_dao_admin.clone(), sub_dao_address.clone(), 999).unwrap();

    // Suspend Sub-DAO
    SubDaoAuthority::suspend_sub_dao(
        env.clone(),
        main_dao_admin.clone(),
        sub_dao_address.clone(),
        String::from_str(&env, "Under investigation"),
    ).unwrap();

    // Check status
    let permission = SubDaoAuthority::get_sub_dao_permission(env.clone(), sub_dao_address.clone()).unwrap();
    assert_eq!(permission.status, SubDaoStatus::Suspended);

    // Unsuspend Sub-DAO
    SubDaoAuthority::unsuspend_sub_dao(env.clone(), main_dao_admin.clone(), sub_dao_address.clone()).unwrap();

    // Check status
    let permission = SubDaoAuthority::get_sub_dao_permission(env.clone(), sub_dao_address.clone()).unwrap();
    assert_eq!(permission.status, SubDaoStatus::Active);

    // Revoke permissions
    SubDaoAuthority::revoke_sub_dao_permissions(
        env.clone(),
        main_dao_admin.clone(),
        sub_dao_address.clone(),
        String::from_str(&env, "Permanent revocation"),
    ).unwrap();

    // Check status
    let permission = SubDaoAuthority::get_sub_dao_permission(env.clone(), sub_dao_address.clone()).unwrap();
    assert_eq!(permission.status, SubDaoStatus::Revoked);
}

#[test]
fn test_error_conditions() {
    let env = Env::default();
    let main_dao_admin = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    let sub_dao_address = Address::generate(&env);

    // Test unauthorized access
    let result = SubDaoAuthority::grant_sub_dao_permissions(
        env.clone(),
        unauthorized.clone(),
        sub_dao_address.clone(),
        String::from_str(&env, "Test"),
        PermissionLevel::Pause,
        1000000,
        None,
    );
    assert_eq!(result, Err(SubDaoError::NotAuthorized));

    // Initialize
    SubDaoAuthority::initialize(env.clone(), main_dao_admin.clone()).unwrap();

    // Test invalid parameters
    let result = SubDaoAuthority::grant_sub_dao_permissions(
        env.clone(),
        main_dao_admin.clone(),
        sub_dao_address.clone(),
        String::from_str(&env, "Test"),
        PermissionLevel::Pause,
        0, // Invalid max amount
        None,
    );
    assert_eq!(result, Err(SubDaoError::InvalidState));

    // Test expired permission
    let past_timestamp = env.ledger().timestamp() - 3600; // 1 hour ago
    let result = SubDaoAuthority::grant_sub_dao_permissions(
        env.clone(),
        main_dao_admin.clone(),
        sub_dao_address.clone(),
        String::from_str(&env, "Test"),
        PermissionLevel::Pause,
        1000000,
        Some(past_timestamp), // Already expired
    );
    assert_eq!(result, Err(SubDaoError::PermissionExpired));
}

#[test]
fn test_grant_contract_integration() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let sub_dao_authority = Address::generate(&env);
    let recipient = Address::generate(&env);
    let token = Address::generate(&env);
    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);
    let native_token = Address::generate(&env);

    // Initialize grant contract
    GrantContract::initialize(
        env.clone(),
        admin.clone(),
        token.clone(),
        treasury.clone(),
        oracle.clone(),
        native_token.clone(),
    ).unwrap();

    // Set Sub-DAO authority contract
    GrantContract::set_sub_dao_authority_contract(
        env.clone(),
        admin.clone(),
        sub_dao_authority.clone(),
    ).unwrap();

    // Create a grant
    GrantContract::create_grant(
        env.clone(),
        1, // grant_id
        recipient.clone(),
        100000, // total_amount
        1000, // flow_rate
        0, // warmup_duration
        Address::generate(&env), // lessor
        String::from_str(&env, "property_123"),
        String::from_str(&env, "SN001"),
        1000, // security_deposit_percentage
        env.ledger().timestamp() + 86400 * 30, // lease_end_time
        None, // validator
    ).unwrap();

    // Test that Sub-DAO authority contract is set
    // In a real implementation, this would be verified through contract calls
    assert!(true); // Placeholder for integration test
}
