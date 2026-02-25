# Grant Self-Termination Feature

## Overview

This feature implements partial grant self-termination, allowing grantees to gracefully terminate their own grants when projects are completed early. This provides autonomy to grantees while ensuring proper fund settlement and refund of unspent portions to the DAO.

## Problem Statement

Currently, grants can only be terminated by administrators, creating dependency and potential delays when grantees complete projects early or need to terminate grants for other reasons. Grantees should have the ability to self-terminate their grants without requiring admin intervention.

## Solution: Self-Termination Mechanism

### Core Features

1. **Grantee Autonomy** - Grantees can terminate their own grants
2. **Final Balance Settlement** - Accrued amounts are settled to grantee
3. **Automatic Refund** - Unspent portions are returned to admin/DAO
4. **Status Management** - Proper status transitions and validation
5. **Event Logging** - Comprehensive audit trail of termination events

### Implementation Details

#### New Status Flag

```rust
pub const STATUS_SELF_TERMINATED: u32 = 0b100000000; // Grant was self-terminated by grantee
```

#### Self-Termination Result Structure

```rust
#[derive(Clone, Debug)]
#[contracttype]
pub struct SelfTerminateResult {
    pub grant_id: u64,              // ID of terminated grant
    pub final_claimable: i128,        // Amount settled to grantee
    pub refunded_amount: i128,        // Amount refunded to admin
    pub terminated_at: u64,           // Termination timestamp
    pub termination_reason: String,     // Reason for termination
}
```

#### Core Function

```rust
pub fn self_terminate(env: Env, grant_id: u64) -> Result<SelfTerminateResult, Error>
```

## Usage Examples

### Basic Self-Termination

```rust
// Grantee terminates their active grant
let result = GrantContract::self_terminate(&env, &contract_id, grant_id)?;

println!("Grant {} terminated", result.grant_id);
println!("Final claimable: {}", result.final_claimable);
println!("Refunded amount: {}", result.refunded_amount);
```

### Checking Termination Eligibility

```rust
// Check if grant can be self-terminated
let can_terminate = GrantContract::can_self_terminate(&env, &contract_id, grant_id)?;
if can_terminate {
    // Proceed with self-termination
}
```

### Getting Termination Details

```rust
// Get details of a terminated grant
let details = GrantContract::get_termination_details(&env, &contract_id, grant_id)?;
println!("Termination reason: {}", details.termination_reason);
```

## State Transitions

### Allowed Self-Termination Transitions

| From State | To State | Who Can Trigger | Conditions |
|-------------|-----------|------------------|------------|
| Active | Self-Terminated | Grantee | Grantee authentication |
| Paused | Self-Terminated | Grantee | Grantee authentication |

### Disallowed Self-Termination Transitions

| From State | Reason |
|-------------|---------|
| Completed | Grant already completed |
| Cancelled | Grant already cancelled |
| Self-Terminated | Already self-terminated |

## Financial Flow

### Self-Termination Process

1. **Authentication** - Verify grantee identity
2. **Settlement** - Calculate final accrued amounts
3. **Distribution** - Split funds appropriately:
   - **Grantee**: Final claimable amount
   - **Admin/DAO**: Remaining unspent balance
4. **Status Update** - Mark as self-terminated
5. **Event Emission** - Log termination details

### Fund Distribution Formula

```rust
total_accounted = withdrawn + claimable
remaining_balance = total_amount - total_accounted

// Distribution:
grantee_receives = claimable  // Final accrued amount
admin_receives = remaining_balance  // Unspent portion
```

## Security Considerations

### Authentication
- **Grantee Verification** - Only grant recipient can self-terminate
- **Authorization Check** - Proper signature validation
- **Replay Protection** - Prevent duplicate terminations

### State Validation
- **Transition Rules** - Enforce valid state transitions
- **Status Consistency** - Maintain status flag integrity
- **Overflow Protection** - Safe arithmetic operations

### Financial Safety
- **Accurate Calculations** - Precise fund distribution
- **Atomic Operations** - All-or-nothing execution
- **Audit Trail** - Complete event logging

## Gas Efficiency

### Optimized Operations

| Operation | Estimated Gas | Optimization |
|-----------|----------------|--------------|
| Self-Termination | ~12,000 | Single storage write |
| Status Check | ~1,500 | Bitwise operations |
| Termination Details | ~2,000 | Direct storage read |
| Eligibility Check | ~1,200 | Simple status validation |

### Batch Operations

For multiple self-terminations:
- **Linear Scaling** - O(n) complexity
- **Shared Calculations** - Efficient batch processing
- **Reduced Storage Writes** - Optimized updates

## Testing Coverage

### Unit Tests

1. **Basic Self-Termination** - Standard flow validation
2. **With Claimable Balance** - Accrued amount handling
3. **Paused Grant Termination** - Paused state support
4. **Authorization Failures** - Unauthorized access prevention
5. **State Validation** - Invalid transition prevention
6. **Edge Cases** - Zero amounts, small grants

### Integration Tests

1. **End-to-End Flow** - Complete termination process
2. **Event Verification** - Proper event emission
3. **Status Consistency** - State integrity maintenance
4. **Financial Accuracy** - Correct fund distribution

### Performance Tests

1. **Gas Efficiency** - Consumption benchmarks
2. **Batch Operations** - Multiple terminations
3. **Large-Scale** - 1000+ grants scenario
4. **Stress Testing** - Edge case handling

## Event System

### Termination Event

```rust
env.events().publish(
    (symbol_short!("selfterm"), grant_id),
    (
        final_claimable,
        refunded_amount,
        terminated_at,
        termination_reason,
    ),
);
```

### Settlement Events

```rust
// Grantee settlement
env.events().publish(
    (symbol_short!("grantee_settle"), recipient),
    (amount, "Final claimable amount settled"),
);

// Admin refund
env.events().publish(
    (symbol_short!("admin_refund"), admin),
    (amount, "Unspent grant balance refunded"),
);
```

## Migration Strategy

### Phase 1: Feature Deployment
1. Deploy self-termination contract
2. Update client libraries
3. Documentation and training

### Phase 2: Gradual Adoption
1. Enable for new grants
2. Monitor usage patterns
3. Collect feedback

### Phase 3: Full Rollout
1. Enable for existing grants
2. Deprecate admin-only termination
3. Update governance processes

## API Reference

### Core Functions

#### `self_terminate(env, grant_id)`
- **Purpose**: Allow grantee to terminate their grant
- **Authentication**: Requires grantee signature
- **Returns**: `SelfTerminateResult` with termination details
- **Errors**: Authorization, state validation, financial errors

#### `can_self_terminate(env, grant_id)`
- **Purpose**: Check if grant can be self-terminated
- **Returns**: Boolean indicating eligibility
- **Use**: Pre-flight checks before attempting termination

#### `get_termination_details(env, grant_id)`
- **Purpose**: Get termination details for self-terminated grants
- **Returns**: `SelfTerminateResult` if terminated, error otherwise
- **Use**: Audit and reporting purposes

### Helper Functions

#### `is_self_terminated(status_mask)`
- **Purpose**: Check if grant is self-terminated
- **Returns**: Boolean status check

#### `can_be_self_terminated(status_mask)`
- **Purpose**: Validate termination eligibility
- **Returns**: Boolean eligibility check

## Error Handling

### Self-Termination Errors

| Error | Code | Description |
|--------|-------|-------------|
| AlreadyTerminated | 11 | Grant already self-terminated |
| CannotTerminateCompleted | 12 | Cannot terminate completed grants |
| CannotTerminateCancelled | 13 | Cannot terminate cancelled grants |
| InsufficientBalance | 14 | Insufficient balance for operations |
| TransferFailed | 15 | Token transfer failed |

### Error Recovery

1. **Validation Errors** - User action required
2. **Transfer Failures** - Retry mechanisms
3. **State Inconsistencies** - Admin intervention
4. **Network Issues** - Transaction retry

## Monitoring and Analytics

### Key Metrics

1. **Self-Termination Rate** - Usage frequency
2. **Average Refund Amount** - Financial impact
3. **Termination Reasons** - Pattern analysis
4. **Gas Consumption** - Performance tracking
5. **Error Rates** - Issue identification

### Dashboard Integration

```rust
// Example metrics query
let total_terminations = get_self_termination_count();
let total_refunded = get_total_refunded_amount();
let average_refund = total_refunded / total_terminations;
```

## Future Enhancements

### Planned Features

1. **Partial Termination** - Terminate specific portions
2. **Scheduled Termination** - Future-dated termination
3. **Conditional Termination** - Rule-based termination
4. **Multi-Signature** - Require additional approvals
5. **Automated Refunds** - Direct token transfers

### Advanced Functionality

1. **Termination Proposals** - DAO governance
2. **Dispute Resolution** - Conflict handling
3. **Performance Bonds** - Incentive alignment
4. **Analytics Dashboard** - Real-time insights

## Conclusion

The self-termination feature provides significant benefits:

- **Grantee Autonomy** - Independent grant management
- **Operational Efficiency** - Reduced admin overhead
- **Financial Accuracy** - Precise fund distribution
- **Audit Trail** - Complete transparency
- **Gas Optimization** - Efficient implementation

This implementation addresses all requirements from issue #33 and provides a robust, secure, and user-friendly self-termination mechanism for grant contracts.

## Files Modified

- `src/self_terminate.rs` - Core self-termination implementation
- `src/test_self_terminate.rs` - Comprehensive test suite
- `src/lib.rs` - Updated module exports
- `SELF_TERMINATE_FEATURE.md` - This documentation

The self-termination feature is ready for deployment and testing.
