# Concurrent Bank Run Fuzz Test

## Overview

This fuzz test simulates a "Bank Run" scenario where 100+ unique addresses attempt to call `withdraw()` in the same ledger sequence. The test verifies that the contract correctly sequences these transactions without hitting storage limits or causing state corruption.

## Key Features

### 1. Concurrent User Simulation
- **150 unique addresses** by default (configurable)
- Each user has their own grant with withdrawable funds
- All users attempt withdrawal in rapid sequence

### 2. Storage Limit Verification
- Monitors storage entry creation during concurrent operations
- Ensures storage doesn't exceed expected limits (grants + withdrawn tracking + metadata)
- Detects storage bloat that could indicate state corruption

### 3. State Corruption Detection
- Verifies contract balance consistency before/after withdrawals
- Ensures total withdrawn amounts match balance changes
- Detects any arithmetic errors or state inconsistencies

### 4. Gas Consumption Sequencing
- Tracks gas consumption patterns across concurrent operations
- Ensures later withdrawers aren't blocked by excessive gas consumption
- Detects non-linear gas scaling that could indicate performance issues

### 5. Non-Blocking Verification
- Ensures at least 95% of users can successfully withdraw
- Detects scenarios where early transactions block later ones
- Verifies fair access to contract functions

## Test Configuration

```rust
pub struct BankRunConfig {
    pub concurrent_users: usize,    // Number of unique addresses (default: 150)
    pub grant_amount: i128,         // Amount per grant (default: 1000 tokens)
    pub seed: u64,                  // Random seed for reproducibility
    pub stream_duration: u64,       // Grant duration (default: 90 days)
    pub warmup_period: u64,         // Warmup before withdrawals (default: 7 days)
}
```

## Invariant Checks

### Storage Limits
- **Expected**: `concurrent_users * 3` storage entries maximum
- **Violation**: Storage entries exceed expected maximum
- **Purpose**: Detect storage bloat and limit exhaustion

### State Consistency
- **Expected**: `final_balance = initial_balance - total_withdrawn`
- **Violation**: Balance arithmetic doesn't match withdrawals
- **Purpose**: Detect state corruption and arithmetic errors

### Gas Sequencing
- **Expected**: Gas consumption increases sublinearly
- **Violation**: >20% of operations show >50% gas increase
- **Purpose**: Ensure fair gas pricing and prevent blocking

### Non-Blocking
- **Expected**: ≥95% success rate for withdrawals
- **Violation**: Success rate below 95%
- **Purpose**: Ensure fair access and prevent transaction blocking

## Test Scenarios

### 1. Basic Bank Run (`fuzz_test_concurrent_bank_run_basic`)
- 100 concurrent users
- 50 tokens per grant
- 60-day duration, 3-day warmup
- Validates all invariants under normal conditions

### 2. Stress Test (`fuzz_test_concurrent_bank_run_stress`)
- 200 concurrent users (maximum stress)
- 100 tokens per grant
- 120-day duration, 7-day warmup
- Tests invariants under maximum load

### 3. Edge Cases (`fuzz_test_concurrent_bank_run_edge_cases`)
- 150 concurrent users
- 1 token per grant (minimal amounts)
- 7-day duration, 1-hour warmup
- Tests boundary conditions and minimal values

## Gas Metrics

The test tracks detailed gas consumption metrics:

```rust
pub struct GasMetrics {
    pub operation_id: u64,
    pub user_address: Address,
    pub gas_consumed: u64,
    pub storage_operations: u32,
    pub timestamp: u64,
    pub withdraw_amount: u128,
}
```

### Gas Analysis
- **Total Gas**: Sum of all operation gas consumption
- **Average Gas**: Mean gas per operation
- **Max/Min Gas**: Highest and lowest gas consumption
- **Gas Range**: Scaling factor (max/min)
- **Gas Scaling**: How gas consumption changes over sequence

## Implementation Details

### Contract Initialization
```rust
contract_client.initialize(
    &admin,
    &token_address,      // Grant token
    &treasury,          // Treasury address
    &oracle,            // Oracle address
    &native_token,      // Native token
);
```

### Grant Creation
```rust
let grantees = Map::new(&env);
grantees.set(user_address.clone(), 10000); // 100% share

contract_client.create_grant(&grant_id, &admin, &grantees);
contract_client.configure_stream(&grant_id, &stream_start, &stream_duration);
```

### Withdrawal Process
```rust
let withdrawable = contract_client.get_withdrawable_amount(&grant_id, &user_address);
if withdrawable > 0 {
    // Attempt withdrawal and track metrics
    let actual_withdrawn = try_withdraw(user_address, grant_id, withdrawable);
    // Record gas metrics and verify invariants
}
```

## Expected Results

### Successful Test
- All invariants pass (no violations)
- ≥95% withdrawal success rate
- Gas scaling ≤3x (even under stress)
- Storage entries within expected limits
- Balance arithmetic correct

### Failure Indicators
- **Storage violations**: Storage bloat or limit exhaustion
- **State corruption**: Balance inconsistencies
- **Gas blocking**: Excessive gas scaling for later users
- **Access blocking**: Low withdrawal success rates

## Running the Tests

```bash
# Run all bank run tests
cargo test fuzz_test_concurrent_bank_run

# Run specific test
cargo test fuzz_test_concurrent_bank_run_basic -- --nocapture

# Run stress test
cargo test fuzz_test_concurrent_bank_run_stress -- --nocapture
```

## Integration with CI

These tests should be integrated into the CI pipeline to ensure:
1. Contract handles concurrent operations correctly
2. Gas consumption remains reasonable under load
3. Storage usage stays within limits
4. State corruption is prevented

## Troubleshooting

### Common Issues
1. **Compilation errors**: Check contract API matches current implementation
2. **Invariant violations**: Review contract logic for arithmetic errors
3. **Gas scaling issues**: Optimize contract storage operations
4. **Storage limit hits**: Review data structures for efficiency

### Debug Tips
- Use `--nocapture` to see detailed test output
- Check gas metrics for unusual patterns
- Verify grant setup matches contract expectations
- Review invariant violation messages for specific issues

## Future Enhancements

1. **Real gas measurement**: Integrate actual Stellar gas metering
2. **Network simulation**: Test with realistic network conditions
3. **Variable timing**: Randomize withdrawal timing patterns
4. **Cross-contract testing**: Test with multiple interacting contracts
5. **Long-running tests**: Extended duration bank run scenarios
