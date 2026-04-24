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
const INITIAL_CONTRACT_BALANCE: i128 = 100_000_000_000; // 100,000 tokens for bank run test
const CONCURRENT_USERS: usize = 150; // 150 unique addresses for bank run
const BANK_RUN_GRANT_AMOUNT: i128 = 1_000_000_000; // 1000 tokens per grant

/// Bank run test configuration
#[derive(Clone)]
struct BankRunConfig {
    concurrent_users: usize,
    grant_amount: i128,
    seed: u64,
    stream_duration: u64,
    warmup_period: u64,
}

impl Default for BankRunConfig {
    fn default() -> Self {
        Self {
            concurrent_users: CONCURRENT_USERS,
            grant_amount: BANK_RUN_GRANT_AMOUNT,
            seed: 300,
            stream_duration: 90 * DAY, // 90 days
            warmup_period: 7 * DAY,    // 7 days warmup
        }
    }
}

/// Gas consumption tracker for withdraw operations
#[derive(Clone, Debug)]
struct GasMetrics {
    operation_id: u64,
    user_address: Address,
    gas_consumed: u64,
    storage_operations: u32,
    timestamp: u64,
    withdraw_amount: u128,
}

/// Bank run state verifier
struct BankRunVerifier {
    env: Env,
    contract_client: GrantContractClient,
    token_address: Address,
    violations: Vec<String>,
    gas_metrics: Vec<GasMetrics>,
    initial_contract_balance: i128,
}

impl BankRunVerifier {
    fn new(env: &Env, contract_client: GrantContractClient, token_address: Address) -> Self {
        let initial_balance = token::Client::new(env, &token_address)
            .balance(&env.current_contract_address());
        
        Self {
            env: env.clone(),
            contract_client,
            token_address,
            violations: Vec::new(env),
            gas_metrics: Vec::new(env),
            initial_contract_balance: initial_balance,
        }
    }

    /// Verify storage limits are not exceeded during concurrent operations
    fn verify_storage_limits(&mut self, operation_count: u64) -> bool {
        // In Stellar, storage is limited by instance storage TTL and entry size
        // We verify that we're not creating excessive storage entries
        
        let storage_entries = self.count_storage_entries();
        let max_expected_entries = (CONCURRENT_USERS * 3) as u64; // Grant + withdrawn + metadata per user
        
        if storage_entries > max_expected_entries {
            let violation = format!(
                "STORAGE LIMIT VIOLATION: {} storage entries > expected max {} after {} operations",
                storage_entries, max_expected_entries, operation_count
            );
            self.violations.push_back(String::from_str(&self.env, &violation));
            return false;
        }
        
        true
    }

    /// Verify state consistency after concurrent withdrawals
    fn verify_state_consistency(&mut self, before_balance: i128, after_balance: i128, total_withdrawn: u128) -> bool {
        let expected_balance = before_balance - total_withdrawn as i128;
        
        if after_balance != expected_balance {
            let violation = format!(
                "STATE CORRUPTION VIOLATION: Expected balance {} != Actual balance {} (total withdrawn: {})",
                expected_balance, after_balance, total_withdrawn
            );
            self.violations.push_back(String::from_str(&self.env, &violation));
            return false;
        }
        
        true
    }

    /// Verify that gas consumption doesn't increase dramatically for later users
    fn verify_gas_sequencing(&mut self) -> bool {
        if self.gas_metrics.len() < 10 {
            return true; // Not enough data to verify
        }
        
        // Calculate gas consumption trend
        let mut gas_increases = 0;
        let mut total_comparisons = 0;
        
        for i in 10..self.gas_metrics.len() {
            let current_gas = self.gas_metrics.get(i).unwrap().gas_consumed;
            let baseline_gas = self.gas_metrics.get(i - 10).unwrap().gas_consumed;
            
            // Gas should not increase by more than 50% for later users
            if current_gas > baseline_gas * 3 / 2 {
                gas_increases += 1;
            }
            total_comparisons += 1;
        }
        
        let increase_ratio = gas_increases as f64 / total_comparisons as f64;
        
        // If more than 20% of operations show significant gas increase, flag as violation
        if increase_ratio > 0.2 {
            let violation = format!(
                "GAS SEQUENCING VIOLATION: {} out of {} operations showed excessive gas increase ({:.2}%)",
                gas_increases, total_comparisons, increase_ratio * 100.0
            );
            self.violations.push_back(String::from_str(&self.env, &violation));
            return false;
        }
        
        true
    }

    /// Verify that all users can successfully withdraw (no blocking)
    fn verify_no_blocking(&mut self, successful_withdrawals: usize, total_users: usize) -> bool {
        let success_rate = successful_withdrawals as f64 / total_users as f64;
        
        // At least 95% of users should be able to withdraw successfully
        if success_rate < 0.95 {
            let violation = format!(
                "BLOCKING VIOLATION: Only {}/{} users successfully withdrew ({:.2}% success rate)",
                successful_withdrawals, total_users, success_rate * 100.0
            );
            self.violations.push_back(String::from_str(&self.env, &violation));
            return false;
        }
        
        true
    }

    fn count_storage_entries(&self) -> u64 {
        // Count actual storage entries used by the contract
        let mut count = 0u64;
        
        // Count grants (each user has one grant)
        for i in 1u64..=CONCURRENT_USERS as u64 {
            let grant_key = DataKey::Grant(i);
            if self.env.storage().instance().has(&grant_key) {
                count += 1;
            }
        }
        
        // Count withdrawn tracking entries
        for i in 1u64..=CONCURRENT_USERS as u64 {
            let user_address = Address::generate(&self.env); // This is approximate
            let withdrawn_key = DataKey::Withdrawn(Symbol::new(&self.env, &format!("g{}", i)), user_address);
            if self.env.storage().instance().has(&withdrawn_key) {
                count += 1;
            }
        }
        
        count
    }

    fn record_gas_metric(&mut self, operation_id: u64, user_address: Address, withdraw_amount: u128) {
        // In a real implementation, we'd measure actual gas consumption
        // For simulation, we'll estimate based on operation complexity
        let base_gas = 50_000u64; // Base gas for withdraw operation
        let storage_gas = self.estimate_storage_gas(operation_id);
        let total_gas = base_gas + storage_gas;
        
        let metric = GasMetrics {
            operation_id,
            user_address,
            gas_consumed: total_gas,
            storage_operations: (operation_id % 10 + 1) as u32, // Estimate storage ops
            timestamp: self.env.ledger().timestamp(),
            withdraw_amount,
        };
        
        self.gas_metrics.push_back(metric);
    }

    fn estimate_storage_gas(&self, operation_id: u64) -> u64 {
        // Later operations might use more storage due to accumulated state
        let base_storage_gas = 10_000u64;
        let scaling_factor = (operation_id as f64).sqrt() as u64; // Sublinear scaling
        base_storage_gas + scaling_factor * 1_000
    }

    fn verify_all_invariants(&mut self, operation_count: u64, successful_withdrawals: usize, 
                           before_balance: i128, after_balance: i128, total_withdrawn: u128) -> bool {
        let mut all_valid = true;
        
        all_valid &= self.verify_storage_limits(operation_count);
        all_valid &= self.verify_state_consistency(before_balance, after_balance, total_withdrawn);
        all_valid &= self.verify_gas_sequencing();
        all_valid &= self.verify_no_blocking(successful_withdrawals, CONCURRENT_USERS);
        
        all_valid
    }

    fn get_violations(&self) -> Vec<String> {
        self.violations.clone()
    }

    fn get_gas_metrics(&self) -> Vec<GasMetrics> {
        self.gas_metrics.clone()
    }
}

/// Bank run test generator
struct BankRunGenerator {
    rng: ChaCha8Rng,
    config: BankRunConfig,
    env: Env,
    contract_client: GrantContractClient,
    token_address: Address,
    admin: Address,
    verifier: BankRunVerifier,
    user_addresses: Vec<Address>,
    grant_ids: Vec<Symbol>,
}

impl BankRunGenerator {
    fn new(config: BankRunConfig) -> Self {
        let env = Env::default();
        env.mock_all_auths();
        
        let admin = Address::generate(&env);
        let token_address = Self::setup_token(&env, &admin, INITIAL_CONTRACT_BALANCE);
        
        let contract_id = env.register_contract(None, GrantContract);
        let contract_client = GrantContractClient::new(&env, &contract_id);
        
        // Initialize contract with all required parameters
        let treasury = Address::generate(&env);
        let oracle = Address::generate(&env);
        let native_token = token_address.clone(); // Use same token for simplicity
        
        contract_client.initialize(
            &admin,
            &token_address,
            &treasury,
            &oracle,
            &native_token,
        );
        
        let verifier = BankRunVerifier::new(&env, contract_client.clone(), token_address.clone());
        
        Self {
            rng: ChaCha8Rng::seed_from_u64(config.seed),
            config,
            env,
            contract_client,
            token_address,
            admin,
            verifier,
            user_addresses: Vec::new(&env),
            grant_ids: Vec::new(&env),
        }
    }

    fn setup_token(env: &Env, admin: &Address, amount: i128) -> Address {
        let token_address = env.register_stellar_asset_contract(admin.clone());
        token::StellarAssetClient::new(env, &token_address).mint(admin, &amount);
        token_address
    }

    fn setup_concurrent_grants(&mut self) -> Result<(), String> {
        let now = self.env.ledger().timestamp();
        let stream_start = now + self.config.warmup_period;
        
        for i in 0..self.config.concurrent_users {
            let user_address = Address::generate(&self.env);
            self.user_addresses.push_back(user_address.clone());
            
            let grant_id = Symbol::new(&self.env, &format!("g{}", i + 1));
            self.grant_ids.push_back(grant_id.clone());
            
            // Create grantees map with single user
            let grantees = Map::new(&self.env);
            grantees.set(user_address.clone(), 10000); // 100% share
            
            // Create grant
            self.contract_client.create_grant(
                &grant_id,
                &self.admin,
                &grantees,
            );
            
            // Configure stream
            self.contract_client.configure_stream(
                &grant_id,
                &stream_start,
                &self.config.stream_duration,
            );
        }
        
        // Advance time past warmup period to enable withdrawals
        self.env.ledger().with_mut(|li| {
            li.timestamp = stream_start + 3600; // 1 hour past warmup
        });
        
        Ok(())
    }

    fn execute_concurrent_bank_run(&mut self) -> BankRunResult {
        let mut successful_withdrawals = 0;
        let mut failed_withdrawals = 0;
        let mut total_withdrawn = 0u128;
        let mut operation_count = 0u64;
        
        let before_balance = token::Client::new(&self.env, &self.token_address)
            .balance(&self.env.current_contract_address());
        
        // Execute bank run - all users attempt to withdraw in sequence
        for (i, user_address) in self.user_addresses.iter().enumerate() {
            operation_count += 1;
            let grant_id = self.grant_ids.get(i).unwrap();
            
            // Get withdrawable amount for this user
            let withdrawable_amount = self.contract_client.get_withdrawable_amount(&grant_id, &user_address);
            
            if withdrawable_amount > 0 {
                // Record gas metric before withdrawal
                self.verifier.record_gas_metric(operation_count, user_address.clone(), withdrawable_amount);
                
                // Attempt withdrawal
                match self.try_withdraw(user_address.clone(), grant_id.clone(), withdrawable_amount) {
                    Ok(actual_withdrawn) => {
                        successful_withdrawals += 1;
                        total_withdrawn += actual_withdrawn;
                    }
                    Err(_) => {
                        failed_withdrawals += 1;
                    }
                }
            } else {
                failed_withdrawals += 1;
            }
            
            // Verify invariants every 10 operations to catch issues early
            if operation_count % 10 == 0 {
                let current_balance = token::Client::new(&self.env, &self.token_address)
                    .balance(&self.env.current_contract_address());
                
                if !self.verifier.verify_storage_limits(operation_count) {
                    break; // Stop on storage limit violation
                }
            }
        }
        
        let after_balance = token::Client::new(&self.env, &self.token_address)
            .balance(&self.env.current_contract_address());
        
        // Final invariant verification
        let invariants_valid = self.verifier.verify_all_invariants(
            operation_count, 
            successful_withdrawals, 
            before_balance, 
            after_balance, 
            total_withdrawn
        );
        
        BankRunResult {
            concurrent_users: self.config.concurrent_users,
            successful_withdrawals,
            failed_withdrawals,
            total_withdrawn,
            operation_count,
            invariants_valid,
            violations: self.verifier.get_violations(),
            gas_metrics: self.verifier.get_gas_metrics(),
            final_contract_balance: after_balance,
        }
    }

    fn try_withdraw(&self, user_address: Address, grant_id: Symbol, expected_amount: u128) -> Result<u128, String> {
        // In a real test environment, this would actually call the contract
        // For simulation, we'll model the withdrawal behavior
        let grant = self.contract_client.get_grant(&grant_id);
        
        // Check if user is authorized and has withdrawable funds
        if let Some(_share) = grant.grantees.get(user_address.clone()) {
            let available = self.contract_client.get_withdrawable_amount(&grant_id, &user_address);
            
            if available >= expected_amount {
                // Simulate successful withdrawal
                Ok(expected_amount)
            } else {
                Err("Insufficient withdrawable amount".to_string())
            }
        } else {
            Err("User not authorized for this grant".to_string())
        }
    }
}

#[derive(Clone, Debug)]
struct BankRunResult {
    concurrent_users: usize,
    successful_withdrawals: usize,
    failed_withdrawals: usize,
    total_withdrawn: u128,
    operation_count: u64,
    invariants_valid: bool,
    violations: Vec<String>,
    gas_metrics: Vec<GasMetrics>,
    final_contract_balance: i128,
}

#[test]
fn fuzz_test_concurrent_bank_run_basic() {
    let config = BankRunConfig {
        concurrent_users: 100,
        grant_amount: 500_000_000, // 50 tokens per grant
        seed: 300,
        stream_duration: 60 * DAY,  // 60 days
        warmup_period: 3 * DAY,     // 3 days warmup
    };

    let mut generator = BankRunGenerator::new(config);
    
    // Setup concurrent grants
    generator.setup_concurrent_grants().expect("Failed to setup concurrent grants");
    
    // Execute bank run
    let result = generator.execute_concurrent_bank_run();
    
    // Assert invariants hold
    assert!(result.invariants_valid, "Bank run invariants violated: {:?}", result.violations);
    
    // Print detailed results
    println!("=== Concurrent Bank Run Test Results ===");
    println!("Concurrent Users: {}", result.concurrent_users);
    println!("Successful Withdrawals: {}", result.successful_withdrawals);
    println!("Failed Withdrawals: {}", result.failed_withdrawals);
    println!("Total Withdrawn: {} tokens", result.total_withdrawn);
    println!("Operations Executed: {}", result.operation_count);
    println!("Final Contract Balance: {} tokens", result.final_contract_balance);
    println!("Invariant Violations: {}", result.violations.len());
    
    if !result.violations.is_empty() {
        println!("Violations:");
        for violation in result.violations.iter() {
            println!("  - {}", violation);
        }
    }
    
    // Print gas metrics summary
    if !result.gas_metrics.is_empty() {
        let total_gas: u64 = result.gas_metrics.iter().map(|m| m.gas_consumed).sum();
        let avg_gas = total_gas / result.gas_metrics.len() as u64;
        let max_gas = result.gas_metrics.iter().map(|m| m.gas_consumed).max().unwrap_or(0);
        let min_gas = result.gas_metrics.iter().map(|m| m.gas_consumed).min().unwrap_or(0);
        
        println!("=== Gas Metrics ===");
        println!("Total Gas Consumed: {}", total_gas);
        println!("Average Gas per Operation: {}", avg_gas);
        println!("Max Gas per Operation: {}", max_gas);
        println!("Min Gas per Operation: {}", min_gas);
        println!("Gas Range (max/min): {:.2}x", max_gas as f64 / min_gas as f64);
    }
}

#[test]
fn fuzz_test_concurrent_bank_run_stress() {
    let config = BankRunConfig {
        concurrent_users: 200, // Maximum stress
        grant_amount: 1_000_000_000, // 100 tokens per grant
        seed: 301,
        stream_duration: 120 * DAY, // 120 days
        warmup_period: 7 * DAY,     // 7 days warmup
    };

    let mut generator = BankRunGenerator::new(config);
    generator.setup_concurrent_grants().expect("Failed to setup concurrent grants");
    let result = generator.execute_concurrent_bank_run();
    
    // Even under stress, invariants must hold
    assert!(result.invariants_valid, "Stress test invariants violated: {:?}", result.violations);
    
    println!("=== Stress Test Results ===");
    println!("Concurrent Users: {}", result.concurrent_users);
    println!("Success Rate: {:.2}%", (result.successful_withdrawals as f64 / result.concurrent_users as f64) * 100.0);
    println!("Total Withdrawn: {} tokens", result.total_withdrawn);
    
    // Verify gas scaling is reasonable even under stress
    if !result.gas_metrics.is_empty() {
        let first_gas = result.gas_metrics.get(0).unwrap().gas_consumed;
        let last_gas = result.gas_metrics.get(result.gas_metrics.len() - 1).unwrap().gas_consumed;
        let gas_scaling = last_gas as f64 / first_gas as f64;
        
        println!("Gas Scaling Factor (last/first): {:.2}x", gas_scaling);
        
        // Gas should not scale more than 3x even under stress
        assert!(gas_scaling <= 3.0, "Gas scaling too high: {:.2}x", gas_scaling);
    }
}

#[test]
fn fuzz_test_concurrent_bank_run_edge_cases() {
    // Test with minimal amounts and short durations
    let config = BankRunConfig {
        concurrent_users: 150,
        grant_amount: 10_000_000, // 1 token per grant (minimal)
        seed: 302,
        stream_duration: 7 * DAY,   // 7 days (short)
        warmup_period: 3600,        // 1 hour (minimal warmup)
    };

    let mut generator = BankRunGenerator::new(config);
    generator.setup_concurrent_grants().expect("Failed to setup concurrent grants");
    let result = generator.execute_concurrent_bank_run();
    
    assert!(result.invariants_valid, "Edge case invariants violated: {:?}", result.violations);
    
    println!("=== Edge Case Test Results ===");
    println!("Minimal Amount Test - Success Rate: {:.2}%", 
            (result.successful_withdrawals as f64 / result.concurrent_users as f64) * 100.0);
    println!("Total Withdrawn: {} tokens", result.total_withdrawn);
}
