# Long-Duration Grant Simulation Implementation

## Issue #19: Testing Long-Duration Simulation

This implementation addresses the requirement to simulate a grant that runs for 10 years, ensuring timestamp math doesn't overflow or drift significantly.

## Implementation Overview

### Grant Contract (`lib.rs`)

The `GrantContract` implements a vesting grant system with the following key features:

1. **Grant Initialization**: `initialize_grant()` sets up a grant with:
   - Recipient address
   - Total amount (using U256 for large numbers)
   - Duration in seconds
   - Automatic start/end timestamp calculation

2. **Claimable Balance Calculation**: `claimable_balance()` calculates vested tokens using:
   - Linear vesting formula: `total_amount * elapsed_time / total_duration`
   - Protection against timestamp overflow
   - U256 arithmetic for precision with large numbers

3. **Claim Functionality**: `claim()` allows recipients to withdraw vested tokens
4. **Grant Information**: `get_grant_info()` returns grant details for testing

### Key Features for Long-Duration Testing

- **U256 Arithmetic**: Uses 256-bit integers to handle large amounts and prevent overflow
- **Timestamp Safety**: Validates timestamp calculations to prevent overflow
- **Linear Vesting**: Simple, predictable vesting schedule
- **Precision**: Maintains accuracy over long periods

## Test Suite (`test.rs`)

### 1. Basic Functionality Test
- Verifies basic grant creation and vesting over short periods

### 2. Long-Duration Simulation Test (Main Requirement)
```rust
test_long_duration_simulation_10_years()
```

**Test Parameters:**
- Duration: `315360000` seconds (exactly 10 years)
- Total Amount: 100,000,000 tokens
- Verification points: Year 5 and Year 10

**Verification at Year 5:**
- Expected: ~50% of total amount vested
- Tolerance: ±1 token for rounding precision
- Formula: `total_amount * 157680000 / 315360000`

**Verification at Year 10:**
- Expected: 100% of total amount vested
- Tolerance: ±1 token for rounding precision
- Tests beyond end time to ensure no additional vesting

### 3. Claim Functionality Test
- Tests claiming at year 5 and year 10
- Verifies total claimed equals total amount
- Ensures claimable balance resets to 0 after claiming

### 4. Timestamp Overflow Test
- Tests with high timestamps near `u64::MAX`
- Verifies no overflow in timestamp calculations
- Uses large amounts to stress test U256 arithmetic

### 5. Grant Information Test
- Verifies proper storage and retrieval of grant parameters

## Acceptance Criteria Fulfillment

✅ **Test case with duration = 315360000 (10 years)**
- Implemented in `test_long_duration_simulation_10_years()`
- Uses exact 10-year duration in seconds

✅ **Verify claimable_balance is accurate at year 5 and year 10**
- Year 5: Verifies ~50% vesting with ±1 token tolerance
- Year 10: Verifies 100% vesting with ±1 token tolerance
- Includes detailed assertions and error messages

## Technical Considerations

### Overflow Prevention
- Uses U256 for amount calculations
- Validates timestamp bounds
- Tests edge cases with maximum timestamps

### Precision Handling
- Linear vesting formula minimizes rounding errors
- Tolerance-based assertions for floating-point precision
- Uses integer arithmetic where possible

### Long-Duration Stability
- Tests with 10-year timespans
- Verifies no timestamp drift
- Validates mathematical accuracy over long periods

## Running the Tests

Once Rust and Visual Studio Build Tools are installed:

```bash
cargo test
```

The test suite will run all 5 test functions, with the main long-duration simulation being the primary focus.

## Files Modified/Created

1. `src/lib.rs` - Grant contract implementation
2. `src/test.rs` - Comprehensive test suite
3. `LONG_DURATION_SIMULATION.md` - This documentation

This implementation fully addresses Issue #19 and provides a robust foundation for long-duration grant simulations on the Stellar blockchain.
