#![cfg(test)]

use super::{GrantContract, GrantContractClient, Grant, GrantStatus, StreamType, DataKey, Error};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _},
    token, Address, Env, Map, String, Vec, Symbol, u128, i128,
};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

const DAY: u64 = 24 * 60 * 60;
const INITIAL_CONTRACT_BALANCE: i128 = 10_000_000_000; // 10,000 tokens in stroops
const MAX_GRANTS_PER_TEST: usize = 100;
const MAX_ITERATIONS: usize = 10_000;

/// Fuzz test configuration
#[derive(Clone)]
struct FuzzConfig {
    max_grants: usize,
    max_iterations: usize,
    seed: u64,
    max_grant_amount: i128,
    min_grant_amount: i128,
}

impl Default for FuzzConfig {
    fn default() -> Self {
        Self {
            max_grants: MAX_GRANTS_PER_TEST,
            max_iterations: MAX_ITERATIONS,
            seed: 42,
            max_grant_amount: 1_000_000_000, // 1000 tokens max per grant
            min_grant_amount: 10_000_000,    // 10 tokens min per grant
        }
    }
}

/// Grant operation for fuzz testing
#[derive(Clone, Debug)]
enum FuzzOperation {
    CreateGrant {
        recipient: Address,
        amount: i128,
        flow_rate: i128,
        duration: u64,
    },
    Withdraw {
        grant_id: Symbol,
        recipient: Address,
        amount: i128,
    },
    CloseGrant {
        grant_id: Symbol,
    },
    PauseGrant {
        grant_id: Symbol,
    },
    ResumeGrant {
        grant_id: Symbol,
    },
    TimeAdvance {
        days: u64,
    },
}

/// Global invariant verifier
struct InvariantVerifier {
    env: Env,
    contract_client: GrantContractClient,
    token_address: Address,
    violations: Vec<String>,
}

impl InvariantVerifier {
    fn new(env: &Env, contract_client: GrantContractClient, token_address: Address) -> Self {
        Self {
            env: env.clone(),
            contract_client,
            token_address,
            violations: Vec::new(env),
        }
    }

    /// Main invariant: Contract Balance >= Sum of all active stream allocations
    fn verify_balance_invariant(&mut self) -> bool {
        let contract_balance = self.get_contract_balance();
        let total_active_allocations = self.calculate_total_active_allocations();
        
        if contract_balance < total_active_allocations {
            let violation = format!(
                "BALANCE VIOLATION: Contract balance {} < Total active allocations {}",
                contract_balance, total_active_allocations
            );
            self.violations.push_back(String::from_str(&self.env, &violation));
            return false;
        }
        
        true
    }

    /// Secondary invariant: Sum of individual grant balances <= Contract balance
    fn verify_grant_balance_sum_invariant(&mut self) -> bool {
        let contract_balance = self.get_contract_balance();
        let total_grant_balances = self.calculate_total_grant_balances();
        
        if total_grant_balances > contract_balance {
            let violation = format!(
                "GRANT BALANCE VIOLATION: Sum of grant balances {} > Contract balance {}",
                total_grant_balances, contract_balance
            );
            self.violations.push_back(String::from_str(&self.env, &violation));
            return false;
        }
        
        true
    }

    /// Verify that withdrawn amounts don't exceed allocated amounts
    fn verify_withdrawal_invariant(&mut self) -> bool {
        let grant_ids = self.get_all_grant_ids();
        
        for grant_id_symbol in grant_ids.iter() {
            // Convert Symbol to u64
            let grant_str = grant_id_symbol.to_string();
            if let Some(num_str) = grant_str.strip_prefix('g') {
                if let Ok(grant_id) = num_str.parse::<u64>() {
                    let grant = self.contract_client.get_grant(&grant_id);
                    
                    if grant.withdrawn > grant.total_amount {
                        let violation = format!(
                            "WITHDRAWAL VIOLATION: Grant {} withdrawn {} > total amount {}",
                            grant_id, grant.withdrawn, grant.total_amount
                        );
                        self.violations.push_back(String::from_str(&self.env, &violation));
                        return false;
                    }
                }
            }
        }
        
        true
    }

    fn get_contract_balance(&self) -> i128 {
        token::Client::new(&self.env, &self.token_address)
            .balance(&self.env.current_contract_address())
    }

    fn calculate_total_active_allocations(&self) -> i128 {
        let grant_ids = self.get_all_grant_ids();
        let mut total = 0i128;
        
        for grant_id_symbol in grant_ids.iter() {
            // Convert Symbol to u64
            let grant_str = grant_id_symbol.to_string();
            if let Some(num_str) = grant_str.strip_prefix('g') {
                if let Ok(grant_id) = num_str.parse::<u64>() {
                    let grant = self.contract_client.get_grant(&grant_id);
                    
                    // Only count active grants
                    if matches!(grant.status, GrantStatus::Active) {
                        let remaining = grant.total_amount - grant.withdrawn;
                        total = total.checked_add(remaining).unwrap_or(i128::MAX);
                    }
                }
            }
        }
        
        total
    }

    fn calculate_total_grant_balances(&self) -> i128 {
        let grant_ids = self.get_all_grant_ids();
        let mut total = 0i128;
        
        for grant_id_symbol in grant_ids.iter() {
            // Convert Symbol to u64
            let grant_str = grant_id_symbol.to_string();
            if let Some(num_str) = grant_str.strip_prefix('g') {
                if let Ok(grant_id) = num_str.parse::<u64>() {
                    let grant = self.contract_client.get_grant(&grant_id);
                    let remaining = grant.total_amount - grant.withdrawn;
                    total = total.checked_add(remaining).unwrap_or(i128::MAX);
                }
            }
        }
        
        total
    }

    fn get_all_grant_ids(&self) -> Vec<Symbol> {
        // Since we don't have a get_all_grants function, we'll track the grants we create
        // For this test, we'll use a reasonable range based on our created grants
        let mut ids = Vec::new(&self.env);
        for i in 1u64..=1000 {
            let id = Symbol::new(&self.env, &format!("g{}", i));
            // Try to get the grant - if it exists, add it to our list
            let grant_result: Result<Grant, soroban_sdk::Error> = self.contract_client.try_get_grant(&id);
            if grant_result.is_ok() {
                ids.push_back(id);
            }
        }
        ids
    }

    fn verify_all_invariants(&mut self) -> bool {
        let mut all_valid = true;
        
        all_valid &= self.verify_balance_invariant();
        all_valid &= self.verify_grant_balance_sum_invariant();
        all_valid &= self.verify_withdrawal_invariant();
        
        all_valid
    }

    fn get_violations(&self) -> Vec<String> {
        self.violations.clone()
    }
}

/// Property-based fuzz test generator
struct FuzzTestGenerator {
    rng: ChaCha8Rng,
    config: FuzzConfig,
    env: Env,
    contract_client: GrantContractClient,
    token_address: Address,
    admin: Address,
    verifier: InvariantVerifier,
    created_grants: Vec<Symbol>,
    operation_count: usize,
}

impl FuzzTestGenerator {
    fn new(config: FuzzConfig) -> Self {
        let env = Env::default();
        env.mock_all_auths();
        
        let admin = Address::generate(&env);
        let token_address = Self::setup_token(&env, &admin, INITIAL_CONTRACT_BALANCE);
        
        let contract_id = env.register_contract(None, GrantContract);
        let contract_client = GrantContractClient::new(&env, &contract_id);
        
        // Initialize contract with correct signature
        contract_client.initialize(
            &admin,
            &token_address,
        );
        
        let verifier = InvariantVerifier::new(&env, contract_client.clone(), token_address.clone());
        
        Self {
            rng: ChaCha8Rng::seed_from_u64(config.seed),
            config,
            env,
            contract_client,
            token_address,
            admin,
            verifier,
            created_grants: Vec::new(&env),
            operation_count: 0,
        }
    }

    fn setup_token(env: &Env, admin: &Address, amount: i128) -> Address {
        let token_address = env.register_stellar_asset_contract(admin.clone());
        token::StellarAssetClient::new(env, &token_address).mint(admin, &amount);
        token_address
    }

    fn generate_random_address(&mut self) -> Address {
        Address::generate(&self.env)
    }

    fn generate_random_amount(&mut self) -> i128 {
        self.rng.gen_range(self.config.min_grant_amount..=self.config.max_grant_amount)
    }

    fn generate_random_flow_rate(&mut self, amount: i128) -> i128 {
        // Flow rate should allow the grant to complete within a reasonable time
        let min_duration = 30 * DAY; // 30 days minimum
        let max_duration = 365 * DAY; // 1 year maximum
        
        let duration = self.rng.gen_range(min_duration..=max_duration);
        amount / (duration as i128).max(1)
    }

    fn generate_random_duration(&mut self) -> u64 {
        self.rng.gen_range(30 * DAY..=365 * DAY)
    }

    fn generate_next_grant_id(&mut self) -> Symbol {
        let id = self.created_grants.len() + 1;
        Symbol::new(&self.env, &format!("g{}", id))
    }

    fn execute_create_grant(&mut self) -> Result<(), String> {
        if self.created_grants.len() >= self.config.max_grants {
            return Err("Maximum grants reached".to_string());
        }

        let recipient = self.generate_random_address();
        let amount = self.generate_random_amount();
        let flow_rate = self.generate_random_flow_rate(amount);
        let duration = self.generate_random_duration();
        let grant_id = self.generate_next_grant_id();
        let id = self.created_grants.len() + 1;

        let now = self.env.ledger().timestamp();

        self.contract_client.create_grant(
            &(id as u64),
            &recipient,
            &(amount as u128),
            &self.token_address,
        );

        self.contract_client.configure_stream(
            &(id as u64),
            &now,
            &duration,
        );

        self.created_grants.push_back(grant_id);
        Ok(())
    }

    fn execute_withdraw(&mut self) -> Result<(), String> {
        if self.created_grants.is_empty() {
            return Err("No grants to withdraw from".to_string());
        }

        let grant_idx = self.rng.gen_range(0..self.created_grants.len());
        let grant_id = grant_idx as u64 + 1;
        
        let grant = self.contract_client.get_grant(&grant_id);
        let withdrawable = self.contract_client.get_withdrawable_amount(&grant_id, &grant.recipient);
        
        if withdrawable > 0 {
            self.contract_client.withdraw(&grant_id, &withdrawable as i128);
        }
        
        Ok(())
    }

    fn execute_close_grant(&mut self) -> Result<(), String> {
        if self.created_grants.is_empty() {
            return Err("No grants to close".to_string());
        }

        let grant_idx = self.rng.gen_range(0..self.created_grants.len());
        let grant_id = grant_idx as u64 + 1;
        
        let grant = self.contract_client.get_grant(&grant_id);
        
        // Only close if grant is completed or can be rage quit
        if matches!(grant.status, GrantStatus::Active | GrantStatus::Paused) {
            self.contract_client.rage_quit(&grant_id);
        }
        
        Ok(())
    }

    fn execute_pause_grant(&mut self) -> Result<(), String> {
        if self.created_grants.is_empty() {
            return Err("No grants to pause".to_string());
        }

        let grant_idx = self.rng.gen_range(0..self.created_grants.len());
        let grant_id = grant_idx as u64 + 1;
        
        let grant = self.contract_client.get_grant(&grant_id);
        
        if matches!(grant.status, GrantStatus::Active) {
            // Note: pause_grant takes Symbol, but we need to convert our u64 to Symbol
            let grant_id_symbol = Symbol::new(&self.env, &format!("g{}", grant_id));
            self.contract_client.pause_grant(&grant_id_symbol);
        }
        
        Ok(())
    }

    fn execute_resume_grant(&mut self) -> Result<(), String> {
        if self.created_grants.is_empty() {
            return Err("No grants to resume".to_string());
        }

        let grant_idx = self.rng.gen_range(0..self.created_grants.len());
        let grant_id = grant_idx as u64 + 1;
        
        let grant = self.contract_client.get_grant(&grant_id);
        
        if matches!(grant.status, GrantStatus::Paused) {
            // Note: resume_grant takes Symbol, but we need to convert our u64 to Symbol
            let grant_id_symbol = Symbol::new(&self.env, &format!("g{}", grant_id));
            self.contract_client.resume_grant(&grant_id_symbol);
        }
        
        Ok(())
    }

    fn execute_time_advance(&mut self) -> Result<(), String> {
        let days = self.rng.gen_range(1..=30); // Advance 1-30 days
        let current_time = self.env.ledger().timestamp();
        let new_time = current_time + (days * DAY);
        
        self.env.ledger().with_mut(|li| {
            li.timestamp = new_time;
        });
        
        Ok(())
    }

    fn generate_random_operation(&mut self) -> FuzzOperation {
        let operation_choice = self.rng.gen_range(0..=6);
        
        match operation_choice {
            0 => FuzzOperation::CreateGrant {
                recipient: self.generate_random_address(),
                amount: self.generate_random_amount(),
                flow_rate: self.generate_random_flow_rate(self.generate_random_amount()),
                duration: self.generate_random_duration(),
            },
            1 => FuzzOperation::Withdraw {
                grant_id: if !self.created_grants.is_empty() {
                    self.created_grants.get(self.rng.gen_range(0..self.created_grants.len())).unwrap().clone()
                } else {
                    Symbol::new(&self.env, "nonexistent")
                },
                recipient: self.generate_random_address(),
                amount: self.generate_random_amount(),
            },
            2 => FuzzOperation::CloseGrant {
                grant_id: if !self.created_grants.is_empty() {
                    self.created_grants.get(self.rng.gen_range(0..self.created_grants.len())).unwrap().clone()
                } else {
                    Symbol::new(&self.env, "nonexistent")
                },
            },
            3 => FuzzOperation::PauseGrant {
                grant_id: if !self.created_grants.is_empty() {
                    self.created_grants.get(self.rng.gen_range(0..self.created_grants.len())).unwrap().clone()
                } else {
                    Symbol::new(&self.env, "nonexistent")
                },
            },
            4 => FuzzOperation::ResumeGrant {
                grant_id: if !self.created_grants.is_empty() {
                    self.created_grants.get(self.rng.gen_range(0..self.created_grants.len())).unwrap().clone()
                } else {
                    Symbol::new(&self.env, "nonexistent")
                },
            },
            5 => FuzzOperation::TimeAdvance {
                days: self.rng.gen_range(1..=30),
            },
            _ => FuzzOperation::TimeAdvance {
                days: 1,
            },
        }
    }

    fn execute_operation(&mut self, operation: FuzzOperation) -> Result<(), String> {
        match operation {
            FuzzOperation::CreateGrant { .. } => self.execute_create_grant(),
            FuzzOperation::Withdraw { .. } => self.execute_withdraw(),
            FuzzOperation::CloseGrant { .. } => self.execute_close_grant(),
            FuzzOperation::PauseGrant { .. } => self.execute_pause_grant(),
            FuzzOperation::ResumeGrant { .. } => self.execute_resume_grant(),
            FuzzOperation::TimeAdvance { .. } => self.execute_time_advance(),
        }
    }

    fn run_fuzz_test(&mut self) -> FuzzTestResult {
        let mut operations_executed = 0;
        let mut operations_failed = 0;
        let mut invariants_checked = 0;
        let mut invariant_violations = 0;

        // Check initial invariants
        invariants_checked += 1;
        if !self.verifier.verify_all_invariants() {
            invariant_violations += 1;
        }

        for i in 0..self.config.max_iterations {
            let operation = self.generate_random_operation();
            
            match self.execute_operation(operation) {
                Ok(_) => {
                    operations_executed += 1;
                    
                    // Check invariants after each successful operation
                    invariants_checked += 1;
                    if !self.verifier.verify_all_invariants() {
                        invariant_violations += 1;
                        
                        // If we find a violation, we can stop early
                        break;
                    }
                }
                Err(_) => {
                    operations_failed += 1;
                }
            }

            self.operation_count += 1;
        }

        FuzzTestResult {
            iterations: self.config.max_iterations,
            operations_executed,
            operations_failed,
            invariants_checked,
            invariant_violations,
            violations: self.verifier.get_violations(),
            final_contract_balance: self.verifier.get_contract_balance(),
            final_grant_count: self.created_grants.len(),
        }
    }
}

#[derive(Clone, Debug)]
struct FuzzTestResult {
    iterations: usize,
    operations_executed: usize,
    operations_failed: usize,
    invariants_checked: usize,
    invariant_violations: usize,
    violations: Vec<String>,
    final_contract_balance: i128,
    final_grant_count: usize,
}

#[test]
fn fuzz_test_global_balance_invariant() {
    let config = FuzzConfig {
        max_grants: 50,
        max_iterations: 5_000,
        seed: 12345,
        max_grant_amount: 100_000_000, // 10 tokens max per grant
        min_grant_amount: 1_000_000,   // 0.1 tokens min per grant
    };

    let mut generator = FuzzTestGenerator::new(config);
    let result = generator.run_fuzz_test();

    // Assert no invariant violations
    assert_eq!(result.invariant_violations, 0, "Invariant violations detected: {:?}", result.violations);
    
    // Print test statistics
    println!("Fuzz Test Results:");
    println!("  Iterations: {}", result.iterations);
    println!("  Operations Executed: {}", result.operations_executed);
    println!("  Operations Failed: {}", result.operations_failed);
    println!("  Invariants Checked: {}", result.invariants_checked);
    println!("  Invariant Violations: {}", result.invariant_violations);
    println!("  Final Contract Balance: {}", result.final_contract_balance);
    println!("  Final Grant Count: {}", result.final_grant_count);
}

#[test]
fn fuzz_test_stress_millions_of_operations() {
    let config = FuzzConfig {
        max_grants: 100,
        max_iterations: 50_000, // 50k iterations for stress testing
        seed: 54321,
        max_grant_amount: 500_000_000, // 50 tokens max per grant
        min_grant_amount: 100_000,     // 0.01 tokens min per grant
    };

    let mut generator = FuzzTestGenerator::new(config);
    let result = generator.run_fuzz_test();

    // Assert no invariant violations even under stress
    assert_eq!(result.invariant_violations, 0, "Invariant violations detected under stress: {:?}", result.violations);
    
    println!("Stress Test Results:");
    println!("  Iterations: {}", result.iterations);
    println!("  Operations Executed: {}", result.operations_executed);
    println!("  Operations Failed: {}", result.operations_failed);
    println!("  Invariants Checked: {}", result.invariants_checked);
    println!("  Invariant Violations: {}", result.invariant_violations);
    println!("  Final Contract Balance: {}", result.final_contract_balance);
    println!("  Final Grant Count: {}", result.final_grant_count);
}

#[test]
fn fuzz_test_edge_cases() {
    // Test with extreme values and edge cases
    let config = FuzzConfig {
        max_grants: 20,
        max_iterations: 1_000,
        seed: 99999,
        max_grant_amount: INITIAL_CONTRACT_BALANCE / 2, // Very large grants
        min_grant_amount: 1,                          // Very small grants
    };

    let mut generator = FuzzTestGenerator::new(config);
    let result = generator.run_fuzz_test();

    // Assert no invariant violations in edge cases
    assert_eq!(result.invariant_violations, 0, "Invariant violations detected in edge cases: {:?}", result.violations);
    
    println!("Edge Case Test Results:");
    println!("  Iterations: {}", result.iterations);
    println!("  Operations Executed: {}", result.operations_executed);
    println!("  Operations Failed: {}", result.operations_failed);
    println!("  Invariants Checked: {}", result.invariants_checked);
    println!("  Invariant Violations: {}", result.invariant_violations);
    println!("  Final Contract Balance: {}", result.final_contract_balance);
    println!("  Final Grant Count: {}", result.final_grant_count);
}

// --- Temporal Invariant Fuzz Testing ---

/// Temporal invariant verifier focused on calculate_flow logic
struct TemporalInvariantVerifier {
    env: Env,
    contract_client: GrantContractClient,
    token_address: Address,
    violations: Vec<String>,
}

impl TemporalInvariantVerifier {
    fn new(env: &Env, contract_client: GrantContractClient, token_address: Address) -> Self {
        Self {
            env: env.clone(),
            contract_client,
            token_address,
            violations: Vec::new(env),
        }
    }

    /// Main temporal invariant: withdrawn amount never exceeds total_allocation
    fn verify_withdrawal_vs_total_allocation(&mut self, grant_id: u64) -> bool {
        let grant_id_symbol = Symbol::new(&self.env, &format!("g{}", grant_id));
        let grant = self.contract_client.get_grant(&grant_id_symbol);
        let total_withdrawable = grant.withdrawn + grant.claimable;
        
        if total_withdrawable > grant.total_amount {
            let violation = format!(
                "TEMPORAL VIOLATION: Grant {} total_withdrawable {} > total_amount {}",
                grant_id, total_withdrawable, grant.total_amount
            );
            self.violations.push_back(String::from_str(&self.env, &violation));
            return false;
        }
        
        true
    }

    /// Verify stream boundary invariants - no extra tokens at start/end
    fn verify_stream_boundary_invariant(&mut self, grant_id: u64) -> bool {
        let grant_id_symbol = Symbol::new(&self.env, &format!("g{}", grant_id));
        let grant = self.contract_client.get_grant(&grant_id_symbol);
        let now = self.env.ledger().timestamp();
        
        // Check stream start boundary
        if now == grant.stream_start {
            let withdrawable = self.contract_client.get_withdrawable_amount(&grant_id_symbol, &grant.recipient);
            // At stream start, should have minimal tokens (warmup period logic)
            let expected_max_at_start = (grant.total_amount * 2500) / 10000; // 25% max at start
            
            if withdrawable > expected_max_at_start {
                let violation = format!(
                    "STREAM START VIOLATION: Grant {} withdrawable at start {} > expected max {}",
                    grant_id, withdrawable, expected_max_at_start
                );
                self.violations.push_back(String::from_str(&self.env, &violation));
                return false;
            }
        }
        
        // Check stream end boundary
        if grant.stream_start > 0 && grant.stream_duration > 0 {
            let stream_end = grant.stream_start + grant.stream_duration;
            if now >= stream_end {
                let total_withdrawable = grant.withdrawn + grant.claimable;
                // At stream end, total should not exceed total_amount
                if total_withdrawable > grant.total_amount {
                    let violation = format!(
                        "STREAM END VIOLATION: Grant {} total_withdrawable at end {} > total_amount {}",
                        grant_id, total_withdrawable, grant.total_amount
                    );
                    self.violations.push_back(String::from_str(&self.env, &violation));
                    return false;
                }
            }
        }
        
        true
    }

    /// Verify flow rate calculation invariants
    fn verify_flow_rate_invariant(&mut self, grant_id: u64, time_delta: u64) -> bool {
        let grant_id_symbol = Symbol::new(&self.env, &format!("g{}", grant_id));
        let grant = self.contract_client.get_grant(&grant_id_symbol);
        
        // Calculate expected flow based on flow_rate and time_delta
        let expected_flow = grant.flow_rate.checked_mul(time_delta as i128).unwrap_or(i128::MAX);
        
        // The actual accrued should not exceed expected flow (considering warmup multipliers)
        let withdrawable = self.contract_client.get_withdrawable_amount(&grant_id_symbol, &grant.recipient);
        
        // With warmup multiplier, actual could be less, but never more than theoretical max
        let theoretical_max = expected_flow.checked_add(grant.withdrawn).unwrap_or(i128::MAX);
        
        if withdrawable > theoretical_max {
            let violation = format!(
                "FLOW RATE VIOLATION: Grant {} withdrawable {} > theoretical max {} for time_delta {}",
                grant_id, withdrawable, theoretical_max, time_delta
            );
            self.violations.push_back(String::from_str(&self.env, &violation));
            return false;
        }
        
        true
    }

    fn verify_all_temporal_invariants(&mut self, grant_id: u64, time_delta: u64) -> bool {
        let mut all_valid = true;
        
        all_valid &= self.verify_withdrawal_vs_total_allocation(grant_id);
        all_valid &= self.verify_stream_boundary_invariant(grant_id);
        all_valid &= self.verify_flow_rate_invariant(grant_id, time_delta);
        
        all_valid
    }

    fn get_violations(&self) -> Vec<String> {
        self.violations.clone()
    }
}

/// Temporal fuzz test generator with random time jumps
struct TemporalFuzzTestGenerator {
    rng: ChaCha8Rng,
    config: FuzzConfig,
    env: Env,
    contract_client: GrantContractClient,
    token_address: Address,
    admin: Address,
    verifier: TemporalInvariantVerifier,
    created_grants: Vec<u64>,
    operation_count: usize,
}

impl TemporalFuzzTestGenerator {
    fn new(config: FuzzConfig) -> Self {
        let env = Env::default();
        env.mock_all_auths();
        
        let admin = Address::generate(&env);
        let token_address = Self::setup_token(&env, &admin, INITIAL_CONTRACT_BALANCE);
        
        let contract_id = env.register_contract(None, GrantContract);
        let contract_client = GrantContractClient::new(&env, &contract_id);
        
        // Initialize contract
        contract_client.initialize(
            &admin,
            &token_address,
            &Address::generate(&env), // treasury
            &Address::generate(&env), // oracle
            &token_address, // native token
        );
        
        let verifier = TemporalInvariantVerifier::new(&env, contract_client.clone(), token_address.clone());
        
        Self {
            rng: ChaCha8Rng::seed_from_u64(config.seed),
            config,
            env,
            contract_client,
            token_address,
            admin,
            verifier,
            created_grants: Vec::new(&env),
            operation_count: 0,
        }
    }

    fn setup_token(env: &Env, admin: &Address, amount: i128) -> Address {
        let token_address = env.register_stellar_asset_contract(admin.clone());
        token::StellarAssetClient::new(env, &token_address).mint(admin, &amount);
        token_address
    }

    fn generate_random_time_jump(&mut self) -> u64 {
        // Random time jumps from 1 second to 10 years
        let min_seconds = 1u64;
        let max_seconds = 10u64 * 365 * 24 * 60 * 60; // 10 years
        
        self.rng.gen_range(min_seconds..=max_seconds)
    }

    fn create_test_grant(&mut self) -> Result<u64, String> {
        if self.created_grants.len() >= self.config.max_grants {
            return Err("Maximum grants reached".to_string());
        }

        let recipient = Address::generate(&self.env);
        let amount = self.rng.gen_range(self.config.min_grant_amount..=self.config.max_grant_amount);
        let flow_rate = amount / (365 * 24 * 60 * 60); // 1 year duration
        let warmup_duration = self.rng.gen_range(0..=30 * 24 * 60 * 60); // 0-30 days warmup
        let grant_id = self.created_grants.len() + 1;

        let now = self.env.ledger().timestamp();

        // Use the correct create_grant signature based on existing tests
        let grant_id_symbol = Symbol::new(&self.env, &format!("g{}", grant_id));
        
        // Create a simple grantee config for testing
        let grantees = Map::new(&self.env);
        grantees.set(recipient.clone(), 10000); // 100% share
        
        self.contract_client.create_grant(
            &grant_id_symbol,
            &self.admin,
            &grantees,
        );

        // Configure stream with random start and duration
        let stream_start = now + self.rng.gen_range(0..=86400); // Start within 24 hours
        let stream_duration = self.rng.gen_range(30 * DAY..=365 * DAY); // 30-365 days
        
        self.contract_client.configure_stream(
            &grant_id_symbol,
            &stream_start,
            &stream_duration,
        );

        self.created_grants.push_back(grant_id);
        Ok(grant_id)
    }

    fn execute_time_jump_and_verify(&mut self) -> Result<(), String> {
        if self.created_grants.is_empty() {
            return Err("No grants to test".to_string());
        }

        let time_jump = self.generate_random_time_jump();
        let current_time = self.env.ledger().timestamp();
        let new_time = current_time + time_jump;
        
        // Store state before time jump
        let mut before_states = Vec::new(&self.env);
        for &grant_id in self.created_grants.iter() {
            let grant_id_symbol = Symbol::new(&self.env, &format!("g{}", grant_id));
            let grant = self.contract_client.get_grant(&grant_id_symbol);
            before_states.push_back((grant_id, grant.withdrawn, grant.claimable));
        }

        // Apply time jump
        self.env.ledger().with_mut(|li| {
            li.timestamp = new_time;
        });

        // Verify invariants after time jump
        for &(grant_id, before_withdrawn, before_claimable) in before_states.iter() {
            if !self.verifier.verify_all_temporal_invariants(grant_id, time_jump) {
                return Err(format!("Temporal invariant violation for grant {} after time jump of {} seconds", grant_id, time_jump));
            }
        }

        Ok(())
    }

    fn execute_withdrawal_and_verify(&mut self) -> Result<(), String> {
        if self.created_grants.is_empty() {
            return Err("No grants to withdraw from".to_string());
        }

        let grant_idx = self.rng.gen_range(0..self.created_grants.len());
        let grant_id = self.created_grants.get(grant_idx).unwrap();
        let grant_id_symbol = Symbol::new(&self.env, &format!("g{}", grant_id));
        
        let grant = self.contract_client.get_grant(&grant_id_symbol);
        let withdrawable = self.contract_client.get_withdrawable_amount(&grant_id_symbol, &grant.recipient);
        
        if withdrawable > 0 {
            // Store state before withdrawal
            let before_withdrawn = grant.withdrawn;
            let before_claimable = grant.claimable;
            
            // Withdrawal amount needs to be i128
            let withdraw_amount = std::cmp::min(withdrawable as i128, self.config.max_grant_amount / 100);
            self.contract_client.withdraw(&grant_id_symbol, &withdraw_amount);
            
            // Verify invariants after withdrawal
            if !self.verifier.verify_withdrawal_vs_total_allocation(*grant_id) {
                return Err(format!("Withdrawal invariant violation for grant {} after withdrawing {}", grant_id, withdraw_amount));
            }
        }
        
        Ok(())
    }

    fn run_temporal_fuzz_test(&mut self) -> TemporalFuzzTestResult {
        let mut operations_executed = 0;
        let mut operations_failed = 0;
        let mut invariants_checked = 0;
        let mut invariant_violations = 0;
        let mut time_jumps_tested = 0;

        // Create initial grants
        for _ in 0..std::cmp::min(10, self.config.max_grants) {
            if let Ok(_) = self.create_test_grant() {
                operations_executed += 1;
            } else {
                operations_failed += 1;
            }
        }

        // Main fuzz loop
        for i in 0..self.config.max_iterations {
            let operation_choice = self.rng.gen_range(0..=2);
            
            let result = match operation_choice {
                0 => self.execute_time_jump_and_verify(),
                1 => self.execute_withdrawal_and_verify(),
                2 => {
                    // Occasionally create new grants
                    if self.created_grants.len() < self.config.max_grants {
                        self.create_test_grant().map(|_| ())
                    } else {
                        self.execute_time_jump_and_verify()
                    }
                }
                _ => self.execute_time_jump_and_verify(),
            };

            match result {
                Ok(_) => {
                    operations_executed += 1;
                    invariants_checked += 1;
                    
                    if operation_choice == 0 {
                        time_jumps_tested += 1;
                    }
                }
                Err(e) => {
                    operations_failed += 1;
                    invariant_violations += 1;
                    
                    // Stop early on invariant violations
                    break;
                }
            }

            self.operation_count += 1;
        }

        TemporalFuzzTestResult {
            iterations: self.config.max_iterations,
            operations_executed,
            operations_failed,
            invariants_checked,
            invariant_violations,
            time_jumps_tested,
            violations: self.verifier.get_violations(),
            final_contract_balance: token::Client::new(&self.env, &self.token_address)
                .balance(&self.env.current_contract_address()),
            final_grant_count: self.created_grants.len(),
        }
    }
}

#[derive(Clone, Debug)]
struct TemporalFuzzTestResult {
    iterations: usize,
    operations_executed: usize,
    operations_failed: usize,
    invariants_checked: usize,
    invariant_violations: usize,
    time_jumps_tested: usize,
    violations: Vec<String>,
    final_contract_balance: i128,
    final_grant_count: usize,
}

#[test]
fn fuzz_test_temporal_invariant_withdrawal_vs_allocation() {
    let config = FuzzConfig {
        max_grants: 20,
        max_iterations: 2_000,
        seed: 298, // Issue number as seed
        max_grant_amount: 100_000_000, // 10 tokens max per grant
        min_grant_amount: 1_000_000,   // 0.1 tokens min per grant
    };

    let mut generator = TemporalFuzzTestGenerator::new(config);
    let result = generator.run_temporal_fuzz_test();

    // Assert no invariant violations
    assert_eq!(result.invariant_violations, 0, "Temporal invariant violations detected: {:?}", result.violations);
    
    println!("Temporal Fuzz Test Results:");
    println!("  Iterations: {}", result.iterations);
    println!("  Operations Executed: {}", result.operations_executed);
    println!("  Operations Failed: {}", result.operations_failed);
    println!("  Invariants Checked: {}", result.invariants_checked);
    println!("  Time Jumps Tested: {}", result.time_jumps_tested);
    println!("  Invariant Violations: {}", result.invariant_violations);
    println!("  Final Contract Balance: {}", result.final_contract_balance);
    println!("  Final Grant Count: {}", result.final_grant_count);
    
    if result.invariant_violations > 0 {
        println!("  Violations: {:?}", result.violations);
    }
}

#[test]
fn fuzz_test_stream_boundaries_start_end() {
    // Test specifically focused on stream start and end boundaries
    let config = FuzzConfig {
        max_grants: 15,
        max_iterations: 1_000,
        seed: 29801, // Issue number with extra digits
        max_grant_amount: 50_000_000, // 5 tokens max per grant
        min_grant_amount: 500_000,    // 0.05 tokens min per grant
    };

    let mut generator = TemporalFuzzTestGenerator::new(config);
    let result = generator.run_temporal_fuzz_test();

    // Assert no boundary violations
    assert_eq!(result.invariant_violations, 0, "Stream boundary violations detected: {:?}", result.violations);
    
    println!("Stream Boundary Fuzz Test Results:");
    println!("  Iterations: {}", result.iterations);
    println!("  Operations Executed: {}", result.operations_executed);
    println!("  Time Jumps Tested: {}", result.time_jumps_tested);
    println!("  Boundary Violations: {}", result.invariant_violations);
    println!("  Final Grant Count: {}", result.final_grant_count);
}

#[test]
fn fuzz_test_extreme_time_jumps() {
    // Test with extreme time jumps (1 second to 10 years)
    let config = FuzzConfig {
        max_grants: 10,
        max_iterations: 500,
        seed: 29802,
        max_grant_amount: 200_000_000, // 20 tokens max per grant
        min_grant_amount: 100_000,      // 0.01 tokens min per grant
    };

    let mut generator = TemporalFuzzTestGenerator::new(config);
    let result = generator.run_temporal_fuzz_test();

    // Assert no violations with extreme time jumps
    assert_eq!(result.invariant_violations, 0, "Extreme time jump violations detected: {:?}", result.violations);
    
    println!("Extreme Time Jump Fuzz Test Results:");
    println!("  Iterations: {}", result.iterations);
    println!("  Time Jumps Tested: {}", result.time_jumps_tested);
    println!("  Invariant Violations: {}", result.invariant_violations);
    println!("  Final Contract Balance: {}", result.final_contract_balance);
}

// Helper extension for GrantContractClient to handle non-existent grants gracefully
trait GrantContractClientExt {
    fn try_get_grant(&self, grant_id: &Symbol) -> Result<Grant, soroban_sdk::Error>;
}

impl GrantContractClientExt for GrantContractClient {
    fn try_get_grant(&self, grant_id: &Symbol) -> Result<Grant, soroban_sdk::Error> {
        // Convert Symbol to u64 for the actual function call
        let grant_str = grant_id.to_string();
        // Extract the numeric part from "gN" format
        if let Some(num_str) = grant_str.strip_prefix('g') {
            if let Ok(id) = num_str.parse::<u64>() {
                return Ok(self.get_grant(&id));
            }
        }
        Err(soroban_sdk::Error::from_contract_error(4)) // GrantNotFound
    }
}
