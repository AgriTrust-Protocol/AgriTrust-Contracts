#[cfg(test)]
mod depth_tracker_tests {
    use crate::depth_tracker::*;
    use soroban_sdk::{contract, contractimpl, Env};
    use soroban_sdk::testutils::Address as _;

    /// Test wrapper contract that exposes depth_tracker operations.
    #[contract]
    pub struct DepthTestContract;

    #[contractimpl]
    impl DepthTestContract {
        pub fn init(env: Env) {
            initialize_depth_tracking(&env);
        }

        pub fn get_depth(env: Env) -> u32 {
            get_current_depth(&env)
        }

        pub fn get_max(env: Env) -> u32 {
            get_max_depth(&env)
        }

        pub fn is_enabled(env: Env) -> bool {
            is_depth_tracking_enabled(&env)
        }

        pub fn push(env: Env) -> u32 {
            push_depth(&env).unwrap_or(0)
        }

        pub fn pop(env: Env) -> u32 {
            pop_depth(&env).unwrap_or(0)
        }

        pub fn flush(env: Env) {
            flush_depth(&env);
        }

        pub fn approaching(env: Env) -> bool {
            is_approaching_limit(&env)
        }

        pub fn set_max(env: Env, val: u32) {
            set_max_depth(&env, val);
        }

        pub fn disable(env: Env) {
            env.storage()
                .instance()
                .set(&DepthKey::DepthTrackingEnabled, &false);
        }
    }

    fn setup() -> (Env, DepthTestContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, DepthTestContract);
        let client = DepthTestContractClient::new(&env, &contract_id);
        client.init();
        (env, client)
    }

    #[test]
    fn test_initialize_sets_zero_depth() {
        let (env, client) = setup();
        assert_eq!(client.get_depth(), 0);
        assert!(client.is_enabled());
        assert_eq!(client.get_max(), DEFAULT_MAX_ADMIN_CHAIN_DEPTH);
    }

    #[test]
    fn test_push_increments_depth() {
        let (env, client) = setup();
        let d = client.push();
        assert_eq!(d, 1);
        assert_eq!(client.get_depth(), 1);
    }

    #[test]
    fn test_pop_decrements_depth() {
        let (env, client) = setup();
        client.push();
        client.push();
        assert_eq!(client.get_depth(), 2);
        let d = client.pop();
        assert_eq!(d, 1);
        assert_eq!(client.get_depth(), 1);
    }

    #[test]
    fn test_pop_at_zero_stays_zero() {
        let (env, client) = setup();
        let d = client.pop();
        assert_eq!(d, 0);
    }

    #[test]
    fn test_push_at_max_returns_error() {
        let (env, client) = setup();
        for _ in 0..DEFAULT_MAX_ADMIN_CHAIN_DEPTH {
            client.push();
        }
        // At max, push returns 0 (unwrap_or(0) on error)
        assert_eq!(client.push(), 0);
    }

    #[test]
    fn test_flush_resets_depth() {
        let (env, client) = setup();
        for _ in 0..10 {
            client.push();
        }
        assert_eq!(client.get_depth(), 10);
        client.flush();
        assert_eq!(client.get_depth(), 0);
    }

    #[test]
    fn test_is_approaching_limit() {
        let (env, client) = setup();
        let target = (DEFAULT_MAX_ADMIN_CHAIN_DEPTH - 3) as usize;
        for _ in 0..target {
            client.push();
        }
        assert!(client.approaching());

        client.pop();
        client.pop();
        assert!(!client.approaching());
    }

    #[test]
    fn test_set_max_depth() {
        let (env, client) = setup();
        client.set_max(&16);
        assert_eq!(client.get_max(), 16);
    }

    #[test]
    fn test_disabled_tracking_returns_zero() {
        let (env, client) = setup();
        client.disable();
        assert_eq!(client.get_depth(), 0);
        assert_eq!(client.push(), 0);
        assert_eq!(client.pop(), 0);
    }

    #[test]
    fn test_10_concurrent_admin_ops_no_overflow() {
        let (env, client) = setup();
        for _ in 0..10 {
            let d = client.push();
            assert!(d > 0);
            client.pop();
        }
        assert_eq!(client.get_depth(), 0);
    }

    #[test]
    fn test_nested_push_pop_16_hop_simulation() {
        let (env, client) = setup();
        for hop in 0..16 {
            let d = client.push();
            assert_eq!(d, (hop + 1) as u32);
        }
        assert_eq!(client.get_depth(), 16);

        for hop in 0..16 {
            let d = client.pop();
            assert_eq!(d, (16 - hop - 1) as u32);
        }
        assert_eq!(client.get_depth(), 0);
    }
}
