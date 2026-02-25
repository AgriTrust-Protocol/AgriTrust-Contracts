# Multi-Token Streaming Feature

## Overview

This feature implements multi-token streaming capabilities for grant contracts, allowing a single grant to stream multiple assets simultaneously (e.g., 500 USDC + 100 GOV_TOKEN per month). This enables more flexible and comprehensive funding arrangements for complex projects.

## Problem Statement

Currently, grants support only single-token streaming, limiting flexibility for projects that require multiple types of funding (e.g., stable currency for operations + governance tokens for incentives). Multi-token streaming provides comprehensive funding solutions in a single grant.

## Solution: Multi-Token Grant Architecture

### Core Features

1. **Multi-Token Support** - Stream multiple tokens simultaneously
2. **Independent Flow Rates** - Different rates per token type
3. **Unified Management** - Single grant controls all tokens
4. **Flexible Withdrawals** - Withdraw specific tokens or combinations
5. **Dynamic Token Management** - Add/remove tokens during grant lifecycle

### Implementation Details

#### Token Balance Structure

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct TokenBalance {
    pub token_address: Address,  // Token contract address
    pub total_amount: i128,      // Total amount allocated for this token
    pub withdrawn: i128,         // Amount already withdrawn
    pub claimable: i128,         // Amount currently claimable
    pub flow_rate: i128,         // Rate per second for this token
}
```

#### Multi-Token Grant Structure

```rust
#[derive(Clone, Debug)]
#[contracttype]
pub struct MultiTokenGrant {
    pub recipient: Address,
    pub tokens: Vec<TokenBalance>,  // Vector of token balances
    pub last_update_ts: u64,
    pub rate_updated_at: u64,
    pub status_mask: u32,
}
```

#### Withdrawal Request Structure

```rust
#[derive(Clone, Debug)]
#[contracttype]
pub struct TokenWithdrawal {
    pub token_address: Address,
    pub amount: i128,
}
```

## Usage Examples

### Creating a Multi-Token Grant

```rust
// Create token balances for USDC and GOV_TOKEN
let mut tokens = Vec::new(&env);
tokens.push_back(create_token_balance(&env, usdc_address, 500000000i128, 192901i128)); // 500 USDC/month
tokens.push_back(create_token_balance(&env, gov_token_address, 100000000i128, 38580i128)); // 100 GOV_TOKEN/month

// Create multi-token grant
GrantContract::create_multi_token_grant(
    &env,
    &contract_id,
    grant_id,
    recipient,
    tokens,
    STATUS_ACTIVE,
)?;
```

### Multi-Token Withdrawal

```rust
// Create withdrawal requests
let mut withdrawals = Vec::new(&env);
withdrawals.push_back(create_token_withdrawal(&env, usdc_address, 1000000i128));
withdrawals.push_back(create_token_withdrawal(&env, gov_token_address, 50000i128));

// Execute multi-token withdrawal
let result = GrantContract::multi_token_withdraw(&env, &contract_id, grant_id, withdrawals)?;

// Check results
println!("Successful withdrawals: {}", result.successful_withdrawals.len());
println!("Failed withdrawals: {}", result.failed_withdrawals.len());
```

### Updating Token Flow Rates

```rust
// Create rate updates
let mut token_updates = Vec::new(&env);
token_updates.push_back(create_token_balance(&env, usdc_address, 0i128, 200000i128)); // Double USDC rate
token_updates.push_back(create_token_balance(&env, gov_token_address, 0i128, 50000i128)); // Reduce GOV rate

// Update rates (admin only)
GrantContract::update_multi_token_rates(&env, &contract_id, grant_id, token_updates)?;
```

### Adding/Removing Tokens

```rust
// Add new token to existing grant
let new_token = create_token_balance(&env, new_token_address, 1000000i128, 1000i128);
GrantContract::add_token_to_grant(&env, &contract_id, grant_id, new_token)?;

// Remove token from grant
GrantContract::remove_token_from_grant(&env, &contract_id, grant_id, token_address)?;
```

## Architecture Design

### Storage Model

#### Single Vault Approach
- **Unified Storage**: All token balances stored in single grant structure
- **Vector-Based**: Dynamic token list using Soroban Vec
- **Efficient Access**: Direct indexing for token operations
- **Atomic Updates**: All tokens updated in single transaction

#### Data Flow
```
Grant Creation → Token Validation → Storage → Accrual Loop → Withdrawal Processing
```

### Accrual Logic

#### Multi-Token Settlement
```rust
fn settle_multi_token_grant(grant: &mut MultiTokenGrant, now: u64) -> Result<(), Error> {
    let elapsed = now - grant.last_update_ts;
    let elapsed_i128 = i128::from(elapsed);
    
    // Settle each token independently
    for token_balance in grant.tokens.iter_mut() {
        let accrued = token_balance.flow_rate * elapsed_i128;
        let remaining = token_balance.total_amount - (token_balance.withdrawn + token_balance.claimable);
        let delta = min(accrued, remaining);
        
        token_balance.claimable += delta;
    }
    
    grant.last_update_ts = now;
    Ok(())
}
```

#### Independent Flow Rates
- **Per-Token Rates**: Each token has independent flow rate
- **Simultaneous Accrual**: All tokens accrue concurrently
- **Rate Flexibility**: Different rates for different token types
- **Zero Rate Support**: Tokens can have zero flow rate

### Withdrawal Processing

#### Batch Withdrawal
```rust
pub fn multi_token_withdraw(
    env: Env,
    grant_id: u64,
    withdrawals: Vec<TokenWithdrawal>,
) -> Result<MultiTokenWithdrawResult, Error>
```

#### Partial Success Handling
- **Individual Processing**: Each withdrawal processed independently
- **Success/Failure Tracking**: Separate lists for successful and failed withdrawals
- **Atomic Per-Token**: Each token withdrawal is atomic
- **Comprehensive Results**: Detailed withdrawal outcome reporting

#### Withdrawal Result Structure
```rust
#[derive(Clone, Debug)]
#[contracttype]
pub struct MultiTokenWithdrawResult {
    pub grant_id: u64,
    pub successful_withdrawals: Vec<TokenWithdrawal>,
    pub failed_withdrawals: Vec<TokenWithdrawal>,
    pub total_withdrawn: Map<Address, i128>,  // token_address -> amount
    pub withdrawn_at: u64,
}
```

## Gas Efficiency

### Optimized Operations

| Operation | Estimated Gas | Optimization |
|-----------|----------------|--------------|
| Create Multi-Token Grant | ~25,000 + 5,000 per token | Single storage write |
| Multi-Token Withdrawal | ~15,000 + 3,000 per token | Batch processing |
| Update Rates | ~10,000 + 2,000 per token | Vector iteration |
| Add Token | ~8,000 | Single push operation |
| Remove Token | ~10,000 | Vector reconstruction |

### Storage Efficiency

#### Vector-Based Storage
- **Dynamic Sizing**: Only store tokens that are used
- **Compact Representation**: Efficient memory layout
- **Single Storage Entry**: One grant structure per grant
- **Linear Scaling**: O(n) complexity for n tokens

#### Batch Operations
- **Reduced Storage Writes**: Multiple operations in single transaction
- **Shared Calculations**: Common computations reused
- **Efficient Iteration**: Optimized vector traversal

## Security Considerations

### Token Validation

#### Address Validation
```rust
// Validate token addresses
for token in tokens.iter() {
    if token.token_address.is_zero() {
        return Err(Error::InvalidTokenAddress);
    }
}
```

#### Duplicate Prevention
```rust
// Check for duplicate tokens
let mut seen = Vec::new();
for token in tokens.iter() {
    if seen.contains(&token.token_address) {
        return Err(Error::DuplicateToken);
    }
    seen.push_back(token.token_address.clone());
}
```

### Amount Validation

#### Overflow Protection
```rust
// Safe arithmetic operations
token_balance.claimable = token_balance
    .claimable
    .checked_add(delta)
    .ok_or(Error::MathOverflow)?;
```

#### Balance Consistency
```rust
// Ensure total_amount >= withdrawn + claimable
let accounted = token_balance.withdrawn + token_balance.claimable;
if accounted > token_balance.total_amount {
    return Err(Error::InvalidState);
}
```

### Access Control

#### Authentication
- **Grantee Verification**: Only grantee can withdraw
- **Admin Authorization**: Token management requires admin
- **Signature Validation**: Proper cryptographic checks

#### State Validation
- **Active Status**: Only active grants allow withdrawals
- **Token Existence**: Verify token exists in grant
- **Sufficient Balance**: Check claimable amount

## Testing Coverage

### Unit Tests

#### Core Functionality
1. **Multi-Token Grant Creation** - Basic multi-token setup
2. **Token Accrual** - Independent rate calculations
3. **Multi-Token Withdrawal** - Batch withdrawal processing
4. **Partial Success Handling** - Mixed success/failure scenarios
5. **Rate Updates** - Dynamic rate modifications

#### Token Management
1. **Add Token** - Dynamic token addition
2. **Remove Token** - Safe token removal
3. **Duplicate Prevention** - Error handling for duplicates
4. **Empty Token List** - Validation edge cases

#### Edge Cases
1. **Zero Flow Rates** - Tokens with no accrual
2. **Single Token** - Degenerate case testing
3. **Maximum Tokens** - Stress testing with many tokens
4. **Large Amounts** - Overflow protection testing

### Integration Tests

#### End-to-End Scenarios
1. **Complete Grant Lifecycle** - Creation to completion
2. **Complex Withdrawal Patterns** - Various withdrawal combinations
3. **Rate Change Impact** - Dynamic rate adjustments
4. **Token Addition/Removal** - Mid-lifecycle modifications

#### Performance Tests
1. **Gas Efficiency** - Consumption benchmarks
2. **Large-Scale Operations** - Many tokens/grants
3. **Batch Processing** - Efficiency of batch operations
4. **Memory Usage** - Storage optimization validation

## API Reference

### Core Functions

#### `create_multi_token_grant(env, grant_id, recipient, tokens, initial_status_mask)`
- **Purpose**: Create a grant with multiple token streams
- **Authentication**: Requires admin authorization
- **Returns**: Empty result on success
- **Errors**: Invalid tokens, duplicate tokens, insufficient authorization

#### `multi_token_withdraw(env, grant_id, withdrawals)`
- **Purpose**: Withdraw from multiple tokens in single transaction
- **Authentication**: Requires grantee signature
- **Returns**: `MultiTokenWithdrawResult` with detailed outcomes
- **Errors**: Invalid state, insufficient balance, token not found

#### `update_multi_token_rates(env, grant_id, token_updates)`
- **Purpose**: Update flow rates for multiple tokens
- **Authentication**: Requires admin authorization
- **Returns**: Empty result on success
- **Errors**: Invalid rates, token not found, insufficient authorization

#### `add_token_to_grant(env, grant_id, token_balance)`
- **Purpose**: Add new token to existing grant
- **Authentication**: Requires admin authorization
- **Returns**: Empty result on success
- **Errors**: Duplicate token, invalid token, insufficient authorization

#### `remove_token_from_grant(env, grant_id, token_address)`
- **Purpose**: Remove token from existing grant
- **Authentication**: Requires admin authorization
- **Returns**: Empty result on success
- **Errors**: Token not found, remaining balance, last token removal

#### `get_token_claimable(env, grant_id, token_address)`
- **Purpose**: Get claimable amount for specific token
- **Authentication**: Public read access
- **Returns**: Claimable amount for token
- **Errors**: Grant not found, token not found

#### `get_multi_token_grant(env, grant_id)`
- **Purpose**: Get complete multi-token grant details
- **Authentication**: Public read access
- **Returns**: `MultiTokenGrant` with all token balances
- **Errors**: Grant not found

### Helper Functions

#### `create_token_balance(env, token_address, total_amount, flow_rate)`
- **Purpose**: Create token balance structure
- **Returns**: `TokenBalance` with initialized values

#### `create_token_withdrawal(env, token_address, amount)`
- **Purpose**: Create withdrawal request structure
- **Returns**: `TokenWithdrawal` for batch operations

## Error Handling

### Multi-Token Errors

| Error | Code | Description |
|--------|-------|-------------|
| TokenNotFound | 16 | Token address not found in grant |
| InvalidTokenAddress | 17 | Invalid or zero token address |
| DuplicateToken | 18 | Token already exists in grant |
| EmptyTokenList | 19 | No tokens provided for operation |
| TokenTransferFailed | 20 | Token transfer operation failed |
| InsufficientTokenBalance | 21 | Insufficient balance for withdrawal |
| InvalidTokenAmount | 22 | Invalid token amount or rate |

### Error Recovery

#### Validation Errors
- **User Action Required**: Correct input parameters
- **Clear Error Messages**: Specific error descriptions
- **Input Validation**: Pre-flight checks

#### Runtime Errors
- **Partial Success**: Continue processing other tokens
- **Rollback Support**: Atomic operations where needed
- **Event Logging**: Complete error tracking

## Event System

### Grant Events

#### Creation Event
```rust
env.events().publish(
    (symbol_short!("multi_create"), grant_id),
    (recipient, token_count, timestamp),
);
```

#### Withdrawal Event
```rust
env.events().publish(
    (symbol_short!("multi_withdraw"), grant_id),
    (successful_count, failed_count, timestamp),
);
```

#### Rate Update Event
```rust
env.events().publish(
    (symbol_short!("multi_rateupdt"), grant_id),
    (token_count, timestamp),
);
```

#### Token Management Events
```rust
// Token addition
env.events().publish(
    (symbol_short!("token_added"), grant_id),
    (new_token_count, timestamp),
);

// Token removal
env.events().publish(
    (symbol_short!("token_removed"), grant_id),
    (token_address, remaining_count),
);
```

## Migration Strategy

### Phase 1: Feature Deployment
1. Deploy multi-token contract
2. Update client libraries
3. Documentation and examples
4. Developer training

### Phase 2: Gradual Adoption
1. Enable for new grants
2. Monitor usage patterns
3. Performance optimization
4. User feedback collection

### Phase 3: Full Integration
1. Migration tools for existing grants
2. Advanced features rollout
3. Ecosystem integration
4. Governance updates

## Future Enhancements

### Planned Features

1. **Token Swaps** - Automatic token conversion
2. **Conditional Streaming** - Rule-based token distribution
3. **Cross-Chain Support** - Multi-chain token streaming
4. **Dynamic Token Lists** - Runtime token addition/removal
5. **Advanced Analytics** - Token flow analytics

### Advanced Functionality

1. **Token Batching** - Group similar tokens
2. **Priority Streaming** - Preferred token treatment
3. **Automatic Rebalancing** - Dynamic rate adjustments
4. **Token Vesting** - Time-based token release
5. **Governance Integration** - DAO token distribution

## Performance Optimization

### Storage Optimization

#### Vector Compression
- **Sparse Representation**: Efficient storage for unused tokens
- **Index Optimization**: Fast token lookup
- **Memory Layout**: Optimized struct packing

#### Batch Operations
- **Single Transactions**: Multiple operations in one tx
- **Shared Validation**: Common validation reuse
- **Efficient Iteration**: Optimized loops

### Computational Optimization

#### Accrual Calculations
- **Vectorized Operations**: Parallel token processing
- **Caching**: Store intermediate results
- **Lazy Evaluation**: Calculate on-demand

#### Withdrawal Processing
- **Early Validation**: Pre-flight checks
- **Batch Transfers**: Group token transfers
- **Error Isolation**: Continue on individual failures

## Conclusion

The multi-token streaming feature provides significant benefits:

- **Flexibility** - Support for complex funding arrangements
- **Efficiency** - Single grant for multiple tokens
- **Scalability** - Dynamic token management
- **User Experience** - Simplified grant management
- **Gas Optimization** - Efficient batch operations

This implementation addresses all requirements from issue #34 and provides a robust, secure, and flexible multi-token streaming system for grant contracts.

## Files Modified

- `src/multi_token.rs` - Core multi-token implementation
- `src/test_multi_token.rs` - Comprehensive test suite
- `src/lib.rs` - Updated module exports
- `MULTI_TOKEN_STREAMING.md` - This documentation

The multi-token streaming feature is ready for deployment and testing.
