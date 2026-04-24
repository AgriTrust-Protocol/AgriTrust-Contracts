# Fuzz-Test: Concurrent Multi-User Claims (Bank Run Scenario)

## Issue #300

This PR implements a comprehensive fuzz test suite for concurrent multi-user claims that simulates a "Bank Run" scenario where 100+ unique addresses attempt to call `withdraw()` in the same ledger sequence.

## Summary

### 🎯 **Problem Solved**
The grant streaming contract needs to handle high-concurrency scenarios without hitting storage limits, causing state corruption, or creating unfair gas consumption patterns that block later withdrawers.

### 🔧 **Solution Implemented**
Added a robust bank run fuzz testing framework with:

#### **Core Components**
- **`BankRunConfig`** - Configurable parameters for concurrent users (150+), grants per user, funding amounts, gas limits, and storage thresholds
- **`UserWithdrawalTracker`** - Individual user behavior tracking including gas consumption and success rates
- **`BankRunInvariantVerifier`** - Comprehensive invariant checking system

#### **Four Critical Invariants**
1. **State Consistency** - Contract balance remains consistent after concurrent withdrawals
2. **Storage Limits** - No storage limit exceeded during bank run operations (≤10,000 entries)
3. **Gas Consumption Fairness** - Later withdrawers aren't blocked by earlier gas consumption (≥80% fairness threshold)
4. **No State Corruption** - Prevents duplicate processing and ensures balance integrity

#### **Three Test Scenarios**
1. **Standard Bank Run** - 150 concurrent users, 3 grants each
2. **Extreme Stress Test** - 300 concurrent users, 5 grants each with higher limits
3. **Gas Consumption Analysis** - Focused gas pattern analysis with tighter constraints

### 📊 **Key Features**
- **Randomized withdrawal order** using Fisher-Yates shuffle to simulate real chaos
- **Detailed tracking** of early vs. late withdrawer success rates
- **Gas consumption monitoring** per user with fairness validation
- **Storage entry counting** and threshold enforcement
- **Balance verification** and state corruption detection

### ✅ **Test Assertions**
- All invariants hold during bank run scenarios
- Early withdrawers achieve ≥90% success rate
- Late withdrawers achieve ≥70% success rate  
- Storage limits are respected (≤10,000 entries)
- Gas fairness is maintained (≥80% relative success rate)
- No state corruption or balance inconsistencies

## 🧪 **Testing**

### New Test Functions
```rust
fuzz_test_concurrent_multi_user_claims_bank_run()
fuzz_test_extreme_bank_run_stress()
fuzz_test_gas_consumption_analysis()
```

### How to Run
```bash
cargo test fuzz_test_concurrent_multi_user_claims_bank_run --lib
cargo test fuzz_test_extreme_bank_run_stress --lib
cargo test fuzz_test_gas_consumption_analysis --lib
```

## 🔒 **Security Benefits**

1. **Concurrency Safety** - Validates contract behavior under extreme load
2. **Gas Fairness** - Ensures no user is disadvantaged by transaction ordering
3. **Storage Efficiency** - Prevents storage exhaustion attacks
4. **State Integrity** - Guarantees no corruption during concurrent operations
5. **Economic Security** - Protects against bank run manipulation

## 📈 **Performance Metrics**

The tests track and validate:
- **Concurrent Users**: Up to 300+ unique addresses
- **Total Operations**: 450-1500+ withdrawal attempts
- **Gas Consumption**: Per-user and aggregate tracking
- **Storage Usage**: Entry counting and threshold monitoring
- **Success Rates**: Early vs. late withdrawer comparison
- **State Consistency**: Balance and integrity verification

## 🛠 **Technical Implementation**

### No-Std Compatibility
- Uses Soroban SDK types instead of standard library
- Custom HashSet implementation using Soroban Vec
- Fisher-Yates shuffle for randomization

### Proper Integration
- Correct function signatures for `withdraw()` and `get_withdrawable_amount()`
- Symbol-based grant ID handling
- Proper error handling and invariant checking

## 📝 **Files Changed**

- `contracts/grant_contracts/src/test_fuzz_invariants.rs` - Added bank run fuzz test suite (628 lines)

## ✨ **Impact**

This implementation provides:
- **Robust testing** for high-concurrency scenarios
- **Security guarantees** for bank run situations  
- **Performance validation** under stress conditions
- **Economic fairness** for all users regardless of transaction order
- **Storage safety** preventing DoS attacks

The contract now has comprehensive testing to ensure it can handle real-world bank run scenarios while maintaining security, fairness, and performance guarantees.
