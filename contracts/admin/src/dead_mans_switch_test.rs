#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        contract, contractimpl,
        testutils::{Address as _, Ledger, LedgerInfo},
        Env,
    };

    use crate::dead_mans_switch::DeadMansSwitchModule;

    #[contract]
    struct TestDmsContract;

    #[contractimpl]
    impl TestDmsContract {
        pub fn initialize(env: Env, admin: Address, vault: Address) {
            DeadMansSwitchModule::initialize(&env, admin, vault);
        }

        pub fn heartbeat(env: Env, admin: Address) {
            DeadMansSwitchModule::reset_last_activity(&env);
        }

        pub fn get_primary_admin(env: Env) -> Address {
            DeadMansSwitchModule::get_primary_admin(&env).unwrap()
        }

        pub fn set_recovery_vault(env: Env, new_vault: Address) {
            DeadMansSwitchModule::set_recovery_vault(&env, new_vault);
        }

        pub fn is_recovery_due(env: Env) -> bool {
            DeadMansSwitchModule::is_recovery_due(&env)
        }

        pub fn execute_recovery(env: Env) -> Address {
            DeadMansSwitchModule::execute_recovery(&env)
        }
    }

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let contract_id = env.register_contract(None, TestDmsContract);
        let client = env.contract(contract_id);
        client.initialize(&admin, &vault);
        (env, admin, vault)
    }

    #[test]
    fn test_heartbeat_resets_countdown() {
        let (env, admin, _vault) = setup();
        // Advance 100 days
        env.ledger().set(LedgerInfo {
            timestamp: env.ledger().timestamp() + (100 * 24 * 60 * 60),
            ..env.ledger().get()
        });
        // Recovery should not be due yet
        assert!(!TestDmsContract::is_recovery_due(env.clone()));
        // Heartbeat resets the countdown
        TestDmsContract::heartbeat(env.clone(), admin.clone());
        // Advance another 100 days
        env.ledger().set(LedgerInfo {
            timestamp: env.ledger().timestamp() + (100 * 24 * 60 * 60),
            ..env.ledger().get()
        });
        // Still not due (only 100 days since heartbeat)
        assert!(!TestDmsContract::is_recovery_due(env.clone()));
    }

    #[test]
    fn test_recovery_after_inactivity() {
        let (env, admin, vault) = setup();
        // Advance 200 days (beyond 180-day threshold)
        env.ledger().set(LedgerInfo {
            timestamp: env.ledger().timestamp() + (200 * 24 * 60 * 60),
            ..env.ledger().get()
        });
        // Recovery should be due
        assert!(TestDmsContract::is_recovery_due(env.clone()));
        // Execute recovery
        let new_admin = TestDmsContract::execute_recovery(env.clone());
        assert_eq!(new_admin, vault);
        // Primary admin should now be the vault
        assert_eq!(TestDmsContract::get_primary_admin(env.clone()), vault);
    }

    #[test]
    fn test_set_recovery_vault() {
        let (env, admin, _vault) = setup();
        let new_vault = Address::generate(&env);
        TestDmsContract::set_recovery_vault(env.clone(), new_vault.clone());
        assert_eq!(TestDmsContract::get_primary_admin(env.clone()), admin);
    }
}
