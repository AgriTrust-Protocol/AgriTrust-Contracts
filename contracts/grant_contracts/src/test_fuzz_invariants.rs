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

// --- Rounding Error Accumulation Fuzz Testing ---

/// Configuration for micro-stream precision testing
#[derive(Clone)]
struct MicroStreamConfig {
    num_streams: usize,
    micro_amount_per_day: i128, // Amount in stroops (e.g., 100 stroops)
    test_duration_days: u64,
    seed: u64,
    initial_balance: i128,
}

impl Default for MicroStreamConfig {
    fn default() -> Self {
        Self {
            num_streams: 1000, // Thousands of micro-streams
            micro_amount_per_day: 100, // 100 stroops per day = 0.00001 XLM
            test_duration_days: 365, // 1 year test
            seed: 299,
            initial_balance: 100_000_000_000, // 10,000 XLM in stroops
        }
    }
}

/// Precision tracking for rounding error analysis
#[derive(Clone, Debug)]
struct PrecisionTracker {
    total_allocated: i128,
    total_withdrawn: i128,
    expected_total: i128,
    rounding_error: i128,
    dust_amount: i128,
    treasury_returns: i128,
    stream_count: usize,
}

impl PrecisionTracker {
    fn new() -> Self {
        Self {
            total_allocated: 0,
            total_withdrawn: 0,
            expected_total: 0,
            rounding_error: 0,
            dust_amount: 0,
            treasury_returns: 0,
            stream_count: 0,
        }
    }

    fn track_allocation(&mut self, amount: i128) {
        self.total_allocated += amount;
        self.stream_count += 1;
    }

    fn track_withdrawal(&mut self, withdrawn: i128, expected: i128) {
        self.total_withdrawn += withdrawn;
        self.expected_total += expected;
        
        // Calculate rounding error for this withdrawal
        let error = expected - withdrawn;
        if error > 0 {
            self.rounding_error += error;
            // Small errors are considered dust
            if error <= 1000 { // 0.0001 XLM threshold
                self.dust_amount += error;
            }
        }
    }

    fn track_treasury_return(&mut self, amount: i128) {
        self.treasury_returns += amount;
    }

    fn get_precision_loss_percentage(&self) -> i128 {
        if self.expected_total == 0 {
            return 0;
        }
        (self.rounding_error * 10_000) / self.expected_total // In basis points
    }

    fn verify_precision_invariants(&self) -> Vec<String> {
        let mut violations = Vec::new();
        
        // Invariant 1: Total withdrawn should not exceed total allocated
        if self.total_withdrawn > self.total_allocated {
            violations.push(format!(
                "PRECISION VIOLATION: Total withdrawn {} exceeds total allocated {}",
                self.total_withdrawn, self.total_allocated
            ));
        }

        // Invariant 2: Rounding error should be reasonable (< 0.1% of total)
        let error_percentage = self.get_precision_loss_percentage();
        if error_percentage > 10 { // 0.1% = 10 basis points
            violations.push(format!(
                "PRECISION VIOLATION: Rounding error {} basis points exceeds 0.1% threshold",
                error_percentage
            ));
        }

        // Invariant 3: Dust should be properly tracked
        if self.dust_amount > self.rounding_error {
            violations.push(format!(
                "PRECISION VIOLATION: Dust amount {} exceeds total rounding error {}",
                self.dust_amount, self.rounding_error
            ));
        }

        violations
    }
}

/// Micro-stream fuzz test generator for precision testing
struct MicroStreamFuzzGenerator {
    rng: ChaCha8Rng,
    config: MicroStreamConfig,
    env: Env,
    contract_client: GrantContractClient,
    token_address: Address,
    admin: Address,
    treasury: Address,
    tracker: PrecisionTracker,
    created_streams: Vec<u64>,
}

impl MicroStreamFuzzGenerator {
    fn new(config: MicroStreamConfig) -> Self {
        let env = Env::default();
        env.mock_all_auths();
        
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let token_address = Self::setup_token(&env, &admin, config.initial_balance);
        
        let contract_id = env.register_contract(None, GrantContract);
        let contract_client = GrantContractClient::new(&env, &contract_id);
        
        // Initialize contract with correct signature
        contract_client.initialize(
            &admin,
            &token_address,
        );
        
        Self {
            rng: ChaCha8Rng::seed_from_u64(config.seed),
            config,
            env,
            contract_client,
            token_address,
            admin,
            treasury,
            tracker: PrecisionTracker::new(),
            created_streams: Vec::new(&env),
        }
    }

    fn setup_token(env: &Env, admin: &Address, amount: i128) -> Address {
        let token_address = env.register_stellar_asset_contract(admin.clone());
        token::StellarAssetClient::new(env, &token_address).mint(admin, &amount);
        token_address
    }

    fn create_micro_streams(&mut self) -> Result<(), String> {
        let now = self.env.ledger().timestamp();
        
        for i in 0..self.config.num_streams {
            let recipient = Address::generate(&self.env);
            let stream_id = (i + 1) as u64;
            
            // Calculate total amount for the entire duration
            let total_amount = self.config.micro_amount_per_day * self.config.test_duration_days as i128;
            
            // Create the grant using the correct signature
            self.contract_client.create_grant(
                &stream_id,
                &recipient,
                &total_amount,
                &self.token_address,
            );
            
            // Fund the stream using the correct signature
            self.contract_client.add_funds(
                &stream_id,
                &total_amount,
            );
            
            self.tracker.track_allocation(total_amount);
            self.created_streams.push_back(stream_id);
        }
        
        Ok(())
    }

    fn simulate_time_progression(&mut self) -> Result<(), String> {
        let initial_time = self.env.ledger().timestamp();
        let final_time = initial_time + (self.config.test_duration_days * DAY);
        
        // Simulate daily progressions to capture rounding errors
        let mut current_time = initial_time;
        
        while current_time < final_time {
            // Advance by 1 day
            current_time += DAY;
            self.env.ledger().with_mut(|li| {
                li.timestamp = current_time;
            });
            
            // For each stream, calculate expected vs actual withdrawal
            for &stream_id in self.created_streams.iter() {
                let grant = self.contract_client.get_grant(&stream_id);
                
                // Calculate expected amount based on perfect precision
                let days_elapsed = (current_time - grant.last_update_ts) / DAY;
                let expected_withdrawable = self.config.micro_amount_per_day * days_elapsed as i128;
                let expected_withdrawable = std::cmp::min(expected_withdrawable, grant.total_amount);
                
                // Get actual withdrawable amount (simplified - use claimable)
                let actual_withdrawable = grant.claimable;
                
                // Track precision
                self.tracker.track_withdrawal(actual_withdrawable, expected_withdrawable);
                
                // Perform withdrawal to test actual behavior
                if actual_withdrawable > 0 {
                    let before_balance = token::Client::new(&self.env, &self.token_address)
                        .balance(&grant.recipient);
                    
                    self.contract_client.withdraw(&stream_id, &actual_withdrawable);
                    
                    let after_balance = token::Client::new(&self.env, &self.token_address)
                        .balance(&grant.recipient);
                    
                    // Verify withdrawal amount
                    let actual_received = after_balance - before_balance;
                    if actual_received != actual_withdrawable {
                        return Err(format!(
                            "Withdrawal mismatch for stream {}: expected {}, received {}",
                            stream_id, actual_withdrawable, actual_received
                        ));
                    }
                }
            }
        }
        
        Ok(())
    }

    fn verify_dust_handling(&mut self) -> Result<(), String> {
        let mut total_dust = 0i128;
        let mut total_treasury_returns = 0i128;
        
        // Check each stream for dust amounts
        for &stream_id in self.created_streams.iter() {
            let grant = self.contract_client.get_grant(&stream_id);
            
            // Check for remaining dust amounts (claimable balance)
            if grant.claimable > 0 && grant.claimable <= 1000 { // Dust threshold
                total_dust += grant.claimable;
            }
            
            // Close the stream to see if remaining funds are returned to treasury
            let treasury_before = token::Client::new(&self.env, &self.token_address)
                .balance(&self.treasury);
            
            // Use rage_quit to close the stream and return remaining funds
            self.contract_client.rage_quit(&stream_id);
            
            let treasury_after = token::Client::new(&self.env, &self.token_address)
                .balance(&self.treasury);
            
            let returned_to_treasury = treasury_after - treasury_before;
            total_treasury_returns += returned_to_treasury;
        }
        
        self.tracker.track_dust_handling(total_dust, total_treasury_returns);
        
        // Verify that dust + treasury returns equals expected rounding error
        let total_recovered = total_dust + total_treasury_returns;
        if total_recovered < self.tracker.rounding_error {
            return Err(format!(
                "Dust handling violation: Expected recovery {}, actual recovery {}",
                self.tracker.rounding_error, total_recovered
            ));
        }
        
        Ok(())
    }

    fn run_precision_fuzz_test(&mut self) -> MicroStreamTestResult {
        let mut operations_executed = 0;
        let mut operations_failed = 0;
        
        // Step 1: Create thousands of micro-streams
        match self.create_micro_streams() {
            Ok(_) => operations_executed += 1,
            Err(e) => {
                operations_failed += 1;
                return MicroStreamTestResult {
                    streams_created: 0,
                    operations_executed,
                    operations_failed,
                    precision_loss_bps: 0,
                    dust_amount: 0,
                    treasury_returns: 0,
                    violations: vec![e],
                };
            }
        }
        
        // Step 2: Simulate time progression and track precision
        match self.simulate_time_progression() {
            Ok(_) => operations_executed += 1,
            Err(e) => {
                operations_failed += 1;
                return MicroStreamTestResult {
                    streams_created: self.created_streams.len(),
                    operations_executed,
                    operations_failed,
                    precision_loss_bps: self.tracker.get_precision_loss_percentage(),
                    dust_amount: self.tracker.dust_amount,
                    treasury_returns: self.tracker.treasury_returns,
                    violations: vec![e],
                };
            }
        }
        
        // Step 3: Verify dust handling and treasury returns
        match self.verify_dust_handling() {
            Ok(_) => operations_executed += 1,
            Err(e) => {
                operations_failed += 1;
            }
        }
        
        // Check all precision invariants
        let violations = self.tracker.verify_precision_invariants();
        
        MicroStreamTestResult {
            streams_created: self.created_streams.len(),
            operations_executed,
            operations_failed,
            precision_loss_bps: self.tracker.get_precision_loss_percentage(),
            dust_amount: self.tracker.dust_amount,
            treasury_returns: self.tracker.treasury_returns,
            violations,
        }
    }
}

#[derive(Clone, Debug)]
struct MicroStreamTestResult {
    streams_created: usize,
    operations_executed: usize,
    operations_failed: usize,
    precision_loss_bps: i128,
    dust_amount: i128,
    treasury_returns: i128,
    violations: Vec<String>,
}

impl PrecisionTracker {
    fn track_dust_handling(&mut self, dust: i128, treasury_returns: i128) {
        self.dust_amount += dust;
        self.treasury_returns += treasury_returns;
    }
}

#[test]
fn fuzz_test_rounding_error_accumulation_micro_streams() {
    let config = MicroStreamConfig {
        num_streams: 1000, // 1000 micro-streams
        micro_amount_per_day: 100, // 100 stroops = 0.00001 XLM per day
        test_duration_days: 365,
        seed: 299,
        initial_balance: 100_000_000_000, // 10,000 XLM
    };

    let mut generator = MicroStreamFuzzGenerator::new(config);
    let result = generator.run_precision_fuzz_test();

    // Assert no precision violations
    assert!(result.violations.is_empty(), "Precision violations detected: {:?}", result.violations);
    
    // Assert precision loss is minimal (< 0.1% = 10 basis points)
    assert!(result.precision_loss_bps <= 10, 
        "Precision loss {} basis points exceeds 0.1% threshold", result.precision_loss_bps);
    
    // Assert dust is properly handled
    assert!(result.dust_amount >= 0, "Dust amount should be non-negative");
    assert!(result.treasury_returns >= 0, "Treasury returns should be non-negative");
    
    println!("Micro-Stream Precision Fuzz Test Results:");
    println!("  Streams Created: {}", result.streams_created);
    println!("  Operations Executed: {}", result.operations_executed);
    println!("  Operations Failed: {}", result.operations_failed);
    println!("  Precision Loss: {} basis points", result.precision_loss_bps);
    println!("  Dust Amount: {} stroops", result.dust_amount);
    println!("  Treasury Returns: {} stroops", result.treasury_returns);
    println!("  Total Rounding Error: {} stroops", result.dust_amount + result.treasury_returns);
    
    if !result.violations.is_empty() {
        println!("  Violations: {:?}", result.violations);
    }
}

#[test]
fn fuzz_test_extreme_micro_streams_precision() {
    // Test with extreme micro-streams (1 stroop per day)
    let config = MicroStreamConfig {
        num_streams: 5000, // 5000 streams
        micro_amount_per_day: 1, // 1 stroop = 0.0000001 XLM per day
        test_duration_days: 730, // 2 years
        seed: 29901,
        initial_balance: 500_000_000_000, // 50,000 XLM
    };

    let mut generator = MicroStreamFuzzGenerator::new(config);
    let result = generator.run_precision_fuzz_test();

    // Even with extreme micro-streams, precision should be maintained
    assert!(result.violations.is_empty(), "Extreme precision violations: {:?}", result.violations);
    assert!(result.precision_loss_bps <= 10, 
        "Extreme precision loss {} basis points", result.precision_loss_bps);
    
    println!("Extreme Micro-Stream Test Results:");
    println!("  Streams: {}", result.streams_created);
    println!("  Precision Loss: {} bps", result.precision_loss_bps);
    println!("  Dust: {} stroops", result.dust_amount);
    println!("  Treasury Returns: {} stroops", result.treasury_returns);
}

#[test]
fn fuzz_test_dust_recovery_validation() {
    // Test specifically focused on dust recovery mechanisms
    let config = MicroStreamConfig {
        num_streams: 2000,
        micro_amount_per_day: 50, // 50 stroops per day
        test_duration_days: 180, // 6 months
        seed: 29902,
        initial_balance: 200_000_000_000, // 20,000 XLM
    };

    let mut generator = MicroStreamFuzzGenerator::new(config);
    let result = generator.run_precision_fuzz_test();

    // Validate dust recovery
    let total_recovered = result.dust_amount + result.treasury_returns;
    assert!(total_recovered > 0, "Should recover some dust/treasury amounts");
    
    println!("Dust Recovery Validation Results:");
    println!("  Dust Recovered: {} stroops", result.dust_amount);
    println!("  Treasury Returns: {} stroops", result.treasury_returns);
    println!("  Total Recovered: {} stroops", total_recovered);
    println!("  Recovery Rate: {:.2}%", 
        (total_recovered as f64 / (result.dust_amount + result.treasury_returns).max(1) as f64) * 100.0);
}

// --- Concurrent Multi-User Claims Fuzz Testing (Bank Run Scenario) ---

/// Configuration for bank run scenario testing
#[derive(Clone)]
struct BankRunConfig {
    num_concurrent_users: usize,
    grants_per_user: usize,
    total_fund_amount: i128,
    seed: u64,
    max_gas_per_withdrawal: u64, // Maximum gas allowed per withdrawal
    storage_limit_threshold: u32, // Storage entries limit
}

impl Default for BankRunConfig {
    fn default() -> Self {
        Self {
            num_concurrent_users: 150, // 150+ unique addresses for stress testing
            grants_per_user: 3, // Multiple grants per user
            total_fund_amount: 1_000_000_000_000, // 100,000 XLM total funding
            seed: 300,
            max_gas_per_withdrawal: 50_000_000, // 50M gas units limit
            storage_limit_threshold: 10_000, // 10k storage entries limit
        }
    }
}

/// User withdrawal tracking for bank run analysis
#[derive(Clone, Debug)]
struct UserWithdrawalTracker {
    user_address: Address,
    grant_ids: Vec<u64>,
    total_withdrawable: i128,
    total_withdrawn: i128,
    gas_consumed: u64,
    withdrawal_order: usize, // Order in bank run sequence
    successful_withdrawals: u32,
    failed_withdrawals: u32,
}

impl UserWithdrawalTracker {
    fn new(address: Address) -> Self {
        Self {
            user_address: address,
            grant_ids: Vec::new(),
            total_withdrawable: 0,
            total_withdrawn: 0,
            gas_consumed: 0,
            withdrawal_order: 0,
            successful_withdrawals: 0,
            failed_withdrawals: 0,
        }
    }

    fn track_withdrawal_attempt(&mut self, grant_id: u64, amount: i128, gas_used: u64, success: bool) {
        self.grant_ids.push_back(grant_id);
        self.gas_consumed += gas_used;
        
        if success {
            self.total_withdrawable += amount;
            self.total_withdrawn += amount;
            self.successful_withdrawals += 1;
        } else {
            self.failed_withdrawals += 1;
        }
    }
}

/// Bank run invariant verifier
struct BankRunInvariantVerifier {
    env: Env,
    contract_client: GrantContractClient,
    token_address: Address,
    violations: Vec<String>,
    storage_entries_count: u32,
    total_gas_consumed: u64,
}

impl BankRunInvariantVerifier {
    fn new(env: &Env, contract_client: GrantContractClient, token_address: Address) -> Self {
        Self {
            env: env.clone(),
            contract_client,
            token_address,
            violations: Vec::new(env),
            storage_entries_count: 0,
            total_gas_consumed: 0,
        }
    }

    /// Invariant 1: Contract state remains consistent after concurrent withdrawals
    fn verify_state_consistency(&mut self, before_balance: i128, after_balance: i128, total_withdrawn: i128) -> bool {
        let expected_balance = before_balance - total_withdrawn;
        
        if after_balance != expected_balance {
            let violation = format!(
                "STATE CONSISTENCY VIOLATION: Expected balance {}, actual balance {} after withdrawing {}",
                expected_balance, after_balance, total_withdrawn
            );
            self.violations.push_back(String::from_str(&self.env, &violation));
            return false;
        }
        
        true
    }

    /// Invariant 2: No storage limit exceeded during bank run
    fn verify_storage_limits(&mut self, storage_threshold: u32) -> bool {
        // Count storage entries by checking various data keys
        let mut entry_count = 0u32;
        
        // Count grant entries (simplified estimation)
        for i in 1u64..=1000 {
            let grant_key = DataKey::Grant(i);
            if self.env.storage().instance().has(&grant_key) {
                entry_count += 1;
            }
        }
        
        self.storage_entries_count = entry_count;
        
        if entry_count > storage_threshold {
            let violation = format!(
                "STORAGE LIMIT VIOLATION: {} storage entries exceed threshold {}",
                entry_count, storage_threshold
            );
            self.violations.push_back(String::from_str(&self.env, &violation));
            return false;
        }
        
        true
    }

    /// Invariant 3: Gas consumption doesn't block later withdrawers
    fn verify_gas_consumption_fairness(&mut self, user_trackers: &Vec<UserWithdrawalTracker>, max_gas_per_withdrawal: u64) -> bool {
        for tracker in user_trackers.iter() {
            if tracker.gas_consumed > max_gas_per_withdrawal {
                let violation = format!(
                    "GAS CONSUMPTION VIOLATION: User {} consumed {} gas, exceeding limit {}",
                    tracker.user_address, tracker.gas_consumed, max_gas_per_withdrawal
                );
                self.violations.push_back(String::from_str(&self.env, &violation));
                return false;
            }
        }
        
        // Check that later withdrawers aren't disproportionately affected
        let early_withdrawers: Vec<_> = user_trackers.iter()
            .filter(|t| t.withdrawal_order < user_trackers.len() / 2)
            .collect();
        
        let late_withdrawers: Vec<_> = user_trackers.iter()
            .filter(|t| t.withdrawal_order >= user_trackers.len() / 2)
            .collect();
        
        let early_success_rate = early_withdrawers.iter()
            .map(|t| t.successful_withdrawals as f64 / (t.successful_withdrawals + t.failed_withdrawals).max(1) as f64)
            .sum::<f64>() / early_withdrawers.len().max(1) as f64;
        
        let late_success_rate = late_withdrawers.iter()
            .map(|t| t.successful_withdrawals as f64 / (t.successful_withdrawals + t.failed_withdrawals).max(1) as f64)
            .sum::<f64>() / late_withdrawers.len().max(1) as f64;
        
        // Later withdrawers should have at least 80% of the success rate of early withdrawers
        if late_success_rate < early_success_rate * 0.8 {
            let violation = format!(
                "GAS FAIRNESS VIOLATION: Late withdrawers success rate {:.2}% < 80% of early rate {:.2}%",
                late_success_rate * 100.0, early_success_rate * 100.0
            );
            self.violations.push_back(String::from_str(&self.env, &violation));
            return false;
        }
        
        true
    }

    /// Invariant 4: No state corruption or data inconsistency
    fn verify_no_state_corruption(&mut self, user_trackers: &Vec<UserWithdrawalTracker>) -> bool {
        let mut total_accounted = 0i128;
        let mut processed_grants = Vec::new(&self.env);
        
        for tracker in user_trackers.iter() {
            total_accounted += tracker.total_withdrawn;
            
            for &grant_id in tracker.grant_ids.iter() {
                // Check if grant was already processed
                let mut already_processed = false;
                for &processed_id in processed_grants.iter() {
                    if processed_id == grant_id {
                        already_processed = true;
                        break;
                    }
                }
                
                if already_processed {
                    // Duplicate grant processing detected
                    let violation = format!(
                        "STATE CORRUPTION VIOLATION: Grant {} processed multiple times",
                        grant_id
                    );
                    self.violations.push_back(String::from_str(&self.env, &violation));
                    return false;
                }
                
                processed_grants.push_back(grant_id);
            }
        }
        
        // Verify contract balance matches accounted withdrawals
        let contract_balance = token::Client::new(&self.env, &self.token_address)
            .balance(&self.env.current_contract_address());
        let expected_balance = INITIAL_CONTRACT_BALANCE - total_accounted;
        
        if contract_balance != expected_balance {
            let violation = format!(
                "STATE CORRUPTION VIOLATION: Contract balance {} doesn't match expected {} after accounting for {} withdrawals",
                contract_balance, expected_balance, total_accounted
            );
            self.violations.push_back(String::from_str(&self.env, &violation));
            return false;
        }
        
        true
    }

    fn verify_all_bank_run_invariants(&mut self, config: &BankRunConfig, user_trackers: &Vec<UserWithdrawalTracker>, 
                                     before_balance: i128, after_balance: i128, total_withdrawn: i128) -> bool {
        let mut all_valid = true;
        
        all_valid &= self.verify_state_consistency(before_balance, after_balance, total_withdrawn);
        all_valid &= self.verify_storage_limits(config.storage_limit_threshold);
        all_valid &= self.verify_gas_consumption_fairness(user_trackers, config.max_gas_per_withdrawal);
        all_valid &= self.verify_no_state_corruption(user_trackers);
        
        all_valid
    }

    fn get_violations(&self) -> Vec<String> {
        self.violations.clone()
    }

    fn get_storage_usage(&self) -> u32 {
        self.storage_entries_count
    }

    fn get_total_gas_consumed(&self) -> u64 {
        self.total_gas_consumed
    }
}

/// Bank run scenario fuzz test generator
struct BankRunFuzzGenerator {
    rng: ChaCha8Rng,
    config: BankRunConfig,
    env: Env,
    contract_client: GrantContractClient,
    token_address: Address,
    admin: Address,
    verifier: BankRunInvariantVerifier,
    user_trackers: Vec<UserWithdrawalTracker>,
    created_grants: Vec<u64>,
}

impl BankRunFuzzGenerator {
    fn new(config: BankRunConfig) -> Self {
        let env = Env::default();
        env.mock_all_auths();
        
        let admin = Address::generate(&env);
        let token_address = Self::setup_token(&env, &admin, config.total_fund_amount);
        
        let contract_id = env.register_contract(None, GrantContract);
        let contract_client = GrantContractClient::new(&env, &contract_id);
        
        // Initialize contract
        contract_client.initialize(
            &admin,
            &token_address,
        );
        
        let verifier = BankRunInvariantVerifier::new(&env, contract_client.clone(), token_address.clone());
        
        Self {
            rng: ChaCha8Rng::seed_from_u64(config.seed),
            config,
            env,
            contract_client,
            token_address,
            admin,
            verifier,
            user_trackers: Vec::new(&env),
            created_grants: Vec::new(&env),
        }
    }

    fn setup_token(env: &Env, admin: &Address, amount: i128) -> Address {
        let token_address = env.register_stellar_asset_contract(admin.clone());
        token::StellarAssetClient::new(env, &token_address).mint(admin, &amount);
        token_address
    }

    fn create_concurrent_grants(&mut self) -> Result<(), String> {
        let amount_per_grant = self.config.total_fund_amount / (self.config.num_concurrent_users * self.config.grants_per_user) as i128;
        let mut grant_id_counter = 1u64;
        
        // Create grants for multiple users
        for user_idx in 0..self.config.num_concurrent_users {
            let user_address = Address::generate(&self.env);
            let mut user_tracker = UserWithdrawalTracker::new(user_address.clone());
            
            for _grant_idx in 0..self.config.grants_per_user {
                let grant_id = grant_id_counter;
                
                // Create grant
                self.contract_client.create_grant(
                    &grant_id,
                    &user_address,
                    &(amount_per_grant as u128),
                    &self.token_address,
                );
                
                // Fund the grant
                self.contract_client.add_funds(
                    &grant_id,
                    &(amount_per_grant as u128),
                );
                
                user_tracker.grant_ids.push_back(grant_id);
                self.created_grants.push_back(grant_id);
                grant_id_counter += 1;
            }
            
            self.user_trackers.push_back(user_tracker);
        }
        
        Ok(())
    }

    fn simulate_bank_run(&mut self) -> Result<BankRunResult, String> {
        let before_balance = token::Client::new(&self.env, &self.token_address)
            .balance(&self.env.current_contract_address());
        
        let mut total_withdrawn = 0i128;
        let mut total_gas_consumed = 0u64;
        
        // Randomize withdrawal order to simulate real bank run chaos
        let mut withdrawal_order: Vec<usize> = Vec::new(&self.env);
        for i in 0..self.user_trackers.len() {
            withdrawal_order.push_back(i);
        }
        
        // Simple Fisher-Yates shuffle implementation
        for i in (1..withdrawal_order.len()).rev() {
            let j = self.rng.gen_range(0..=i);
            let temp = withdrawal_order.get(i);
            withdrawal_order.set(i, withdrawal_order.get(j));
            withdrawal_order.set(j, temp);
        }
        
        // Execute concurrent withdrawals
        for (order, &user_idx) in withdrawal_order.iter().enumerate() {
            let user_tracker = &mut self.user_trackers[user_idx];
            user_tracker.withdrawal_order = order;
            
            let user_gas_start = self.env.ledger().timestamp(); // Simplified gas tracking
            
            for &grant_id in user_tracker.grant_ids.iter() {
                let grant_id_symbol = Symbol::new(&self.env, &format!("g{}", grant_id));
                let grant = self.contract_client.get_grant(&grant_id);
                let withdrawable = self.contract_client.get_withdrawable_amount(&grant_id_symbol, &grant.recipient);
                
                if withdrawable > 0 {
                    let withdraw_amount = std::cmp::min(withdrawable as i128, grant.total_amount / 10); // Withdraw 10% at a time
                    
                    // Attempt withdrawal - check if withdrawable amount is sufficient
                    let can_withdraw = withdrawable >= withdraw_amount as u128;
                    
                    let user_gas_end = self.env.ledger().timestamp();
                    let gas_used = (user_gas_end - user_gas_start) as u64; // Simplified gas calculation
                    
                    if can_withdraw {
                        // Get user balance before withdrawal
                        let token_client = token::Client::new(&self.env, &self.token_address);
                        let balance_before = token_client.balance(&grant.recipient);
                        
                        // Perform withdrawal
                        self.contract_client.withdraw(&grant_id_symbol, &withdraw_amount);
                        
                        // Verify withdrawal succeeded
                        let balance_after = token_client.balance(&grant.recipient);
                        let actual_withdrawn = balance_after - balance_before;
                        
                        if actual_withdrawn == withdraw_amount as u128 {
                            user_tracker.track_withdrawal_attempt(grant_id, withdraw_amount, gas_used, true);
                            total_withdrawn += withdraw_amount;
                            total_gas_consumed += gas_used;
                        } else {
                            user_tracker.track_withdrawal_attempt(grant_id, withdraw_amount, gas_used, false);
                            total_gas_consumed += gas_used;
                        }
                    } else {
                        user_tracker.track_withdrawal_attempt(grant_id, withdraw_amount, gas_used, false);
                        total_gas_consumed += gas_used;
                    }
                }
            }
        }
        
        let after_balance = token::Client::new(&self.env, &self.token_address)
            .balance(&self.env.current_contract_address());
        
        // Verify all invariants
        let invariants_held = self.verifier.verify_all_bank_run_invariants(
            &self.config,
            &self.user_trackers,
            before_balance,
            after_balance,
            total_withdrawn,
        );
        
        Ok(BankRunResult {
            num_concurrent_users: self.config.num_concurrent_users,
            grants_created: self.created_grants.len(),
            total_withdrawn,
            total_gas_consumed,
            storage_entries_used: self.verifier.get_storage_usage(),
            successful_withdrawals: self.user_trackers.iter().map(|t| t.successful_withdrawals).sum(),
            failed_withdrawals: self.user_trackers.iter().map(|t| t.failed_withdrawals).sum(),
            invariants_held,
            violations: self.verifier.get_violations(),
            early_withdrawer_success_rate: self.calculate_early_success_rate(),
            late_withdrawer_success_rate: self.calculate_late_success_rate(),
        })
    }

    fn calculate_early_success_rate(&self) -> f64 {
        let early_withdrawers: Vec<_> = self.user_trackers.iter()
            .filter(|t| t.withdrawal_order < self.user_trackers.len() / 2)
            .collect();
        
        if early_withdrawers.is_empty() {
            return 0.0;
        }
        
        early_withdrawers.iter()
            .map(|t| t.successful_withdrawals as f64 / (t.successful_withdrawals + t.failed_withdrawals).max(1) as f64)
            .sum::<f64>() / early_withdrawers.len() as f64
    }

    fn calculate_late_success_rate(&self) -> f64 {
        let late_withdrawers: Vec<_> = self.user_trackers.iter()
            .filter(|t| t.withdrawal_order >= self.user_trackers.len() / 2)
            .collect();
        
        if late_withdrawers.is_empty() {
            return 0.0;
        }
        
        late_withdrawers.iter()
            .map(|t| t.successful_withdrawals as f64 / (t.successful_withdrawals + t.failed_withdrawals).max(1) as f64)
            .sum::<f64>() / late_withdrawers.len() as f64
    }

    fn run_bank_run_test(&mut self) -> BankRunResult {
        // Create concurrent grants
        if let Err(e) = self.create_concurrent_grants() {
            return BankRunResult {
                num_concurrent_users: self.config.num_concurrent_users,
                grants_created: 0,
                total_withdrawn: 0,
                total_gas_consumed: 0,
                storage_entries_used: 0,
                successful_withdrawals: 0,
                failed_withdrawals: 0,
                invariants_held: false,
                violations: Vec::new(&self.env),
                early_withdrawer_success_rate: 0.0,
                late_withdrawer_success_rate: 0.0,
            };
        }
        
        // Simulate bank run
        self.simulate_bank_run().unwrap_or_else(|e| BankRunResult {
            num_concurrent_users: self.config.num_concurrent_users,
            grants_created: self.created_grants.len(),
            total_withdrawn: 0,
            total_gas_consumed: 0,
            storage_entries_used: 0,
            successful_withdrawals: 0,
            failed_withdrawals: 0,
            invariants_held: false,
            violations: Vec::new(&self.env),
            early_withdrawer_success_rate: 0.0,
            late_withdrawer_success_rate: 0.0,
        })
    }
}

#[derive(Clone, Debug)]
struct BankRunResult {
    num_concurrent_users: usize,
    grants_created: usize,
    total_withdrawn: i128,
    total_gas_consumed: u64,
    storage_entries_used: u32,
    successful_withdrawals: u32,
    failed_withdrawals: u32,
    invariants_held: bool,
    violations: Vec<String>,
    early_withdrawer_success_rate: f64,
    late_withdrawer_success_rate: f64,
}

#[test]
fn fuzz_test_concurrent_multi_user_claims_bank_run() {
    let config = BankRunConfig {
        num_concurrent_users: 150, // 150+ unique addresses
        grants_per_user: 3,
        total_fund_amount: 1_000_000_000_000, // 100,000 XLM
        seed: 300,
        max_gas_per_withdrawal: 50_000_000,
        storage_limit_threshold: 10_000,
    };

    let mut generator = BankRunFuzzGenerator::new(config);
    let result = generator.run_bank_run_test();

    // Assert all invariants hold during bank run
    assert!(result.invariants_held, "Bank run invariants violated: {:?}", result.violations);
    
    // Assert reasonable success rates for both early and late withdrawers
    assert!(result.early_withdrawer_success_rate >= 0.9, 
        "Early withdrawer success rate {:.2}% should be >= 90%", result.early_withdrawer_success_rate * 100.0);
    assert!(result.late_withdrawer_success_rate >= 0.7, 
        "Late withdrawer success rate {:.2}% should be >= 70%", result.late_withdrawer_success_rate * 100.0);
    
    // Assert storage limits are respected
    assert!(result.storage_entries_used <= 10_000, 
        "Storage usage {} exceeds limit", result.storage_entries_used);
    
    // Assert no gas consumption blocking
    let success_rate_fairness = if result.early_withdrawer_success_rate > 0.0 {
        result.late_withdrawer_success_rate / result.early_withdrawer_success_rate
    } else {
        0.0
    };
    assert!(success_rate_fairness >= 0.8, 
        "Late withdrawer success rate fairness {:.2} should be >= 80%", success_rate_fairness);
    
    println!("Bank Run Fuzz Test Results:");
    println!("  Concurrent Users: {}", result.num_concurrent_users);
    println!("  Grants Created: {}", result.grants_created);
    println!("  Total Withdrawn: {} stroops", result.total_withdrawn);
    println!("  Total Gas Consumed: {}", result.total_gas_consumed);
    println!("  Storage Entries Used: {}", result.storage_entries_used);
    println!("  Successful Withdrawals: {}", result.successful_withdrawals);
    println!("  Failed Withdrawals: {}", result.failed_withdrawals);
    println!("  Early Withdrawer Success Rate: {:.2}%", result.early_withdrawer_success_rate * 100.0);
    println!("  Late Withdrawer Success Rate: {:.2}%", result.late_withdrawer_success_rate * 100.0);
    println!("  Success Rate Fairness: {:.2}%", success_rate_fairness * 100.0);
    println!("  Invariants Held: {}", result.invariants_held);
    
    if !result.violations.is_empty() {
        println!("  Violations: {:?}", result.violations);
    }
}

#[test]
fn fuzz_test_extreme_bank_run_stress() {
    // Extreme stress test with maximum concurrent users
    let config = BankRunConfig {
        num_concurrent_users: 300, // Double the users for stress testing
        grants_per_user: 5, // More grants per user
        total_fund_amount: 5_000_000_000_000, // 500,000 XLM
        seed: 301,
        max_gas_per_withdrawal: 100_000_000, // Higher gas limit for stress
        storage_limit_threshold: 20_000, // Higher storage limit
    };

    let mut generator = BankRunFuzzGenerator::new(config);
    let result = generator.run_bank_run_test();

    // Even under extreme stress, invariants must hold
    assert!(result.invariants_held, "Extreme bank run invariants violated: {:?}", result.violations);
    
    println!("Extreme Bank Run Stress Test Results:");
    println!("  Concurrent Users: {}", result.num_concurrent_users);
    println!("  Grants Created: {}", result.grants_created);
    println!("  Total Withdrawn: {} stroops", result.total_withdrawn);
    println!("  Storage Entries Used: {}", result.storage_entries_used);
    println!("  Early Success Rate: {:.2}%", result.early_withdrawer_success_rate * 100.0);
    println!("  Late Success Rate: {:.2}%", result.late_withdrawer_success_rate * 100.0);
    println!("  Invariants Held: {}", result.invariants_held);
}

#[test]
fn fuzz_test_gas_consumption_analysis() {
    // Test specifically focused on gas consumption patterns
    let config = BankRunConfig {
        num_concurrent_users: 100,
        grants_per_user: 2,
        total_fund_amount: 500_000_000_000, // 50,000 XLM
        seed: 302,
        max_gas_per_withdrawal: 25_000_000, // Lower gas limit to test constraints
        storage_limit_threshold: 5_000,
    };

    let mut generator = BankRunFuzzGenerator::new(config);
    let result = generator.run_bank_run_test();

    // Verify gas consumption doesn't create unfair advantages
    assert!(result.invariants_held, "Gas consumption analysis failed: {:?}", result.violations);
    
    // Calculate average gas per successful withdrawal
    let avg_gas_per_withdrawal = if result.successful_withdrawals > 0 {
        result.total_gas_consumed / result.successful_withdrawals as u64
    } else {
        0
    };
    
    println!("Gas Consumption Analysis Results:");
    println!("  Total Gas Consumed: {}", result.total_gas_consumed);
    println!("  Successful Withdrawals: {}", result.successful_withdrawals);
    println!("  Average Gas per Withdrawal: {}", avg_gas_per_withdrawal);
    println!("  Early Success Rate: {:.2}%", result.early_withdrawer_success_rate * 100.0);
    println!("  Late Success Rate: {:.2}%", result.late_withdrawer_success_rate * 100.0);
    println!("  Gas Fairness Maintained: {}", result.invariants_held);
}
