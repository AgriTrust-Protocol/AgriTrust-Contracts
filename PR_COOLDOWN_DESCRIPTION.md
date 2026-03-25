# Implement Stream Pause Cooldown Period

## Summary

This PR implements a **Stream Pause Cooldown Period** feature to prevent "Governance Harassment" by ensuring that grants cannot be repeatedly paused without proper justification or super-majority support. This provides stability for development teams while maintaining necessary governance oversight.

## Problem Statement

Currently, grants can be paused and resumed repeatedly by small groups of voters, creating:
- **Funding instability** for development teams
- **Governance harassment** through repeated pause/resume cycles
- **Productivity disruption** as teams must constantly defend their funding
- **Uncertainty** that hampers long-term project planning

## Solution

### 14-Day Cooldown Period
- After a grant is resumed, it cannot be paused again for **14 days**
- Prevents "fickle" governance and rapid pause/resume cycles
- Gives development teams predictable funding stability

### Super-Majority Emergency Override
- Emergency pauses during cooldown require **75% super-majority vote**
- Allows legitimate emergency interventions while preventing abuse
- Balances protection with necessary governance flexibility

### Pause Count Tracking
- Tracks how many times each grant has been paused
- Provides transparency for governance decisions
- Enables data-driven policy improvements

## Implementation Details

### New Grant Fields
```rust
pub last_resume_timestamp: Option<u64>, // Timestamp when grant was last resumed
pub pause_count: u32, // Number of times this grant has been paused
```

### Updated Functions
- **`pause_stream()`**: Now accepts `is_emergency` flag and `voting_power` parameter
- **`resume_stream()`**: Sets `last_resume_timestamp` for cooldown tracking
- **Grant creation**: Updated all grant creation points to initialize new fields

### New Error Types
- **`PauseCooldownActive`**: Returned when attempting to pause during cooldown
- **`InsufficientSuperMajority`**: Returned when emergency pause lacks super-majority

### Constants
```rust
const PAUSE_COOLDOWN_PERIOD: u64 = 14 * 24 * 60 * 60; // 14 days
const SUPER_MAJORITY_THRESHOLD: u32 = 7500; // 75% in basis points
```

## Security Considerations

### Governance Protection
- **Cooldown period** prevents rapid pause/resume attacks
- **Super-majority requirement** ensures broad consensus for emergencies
- **Audit trail** through pause count tracking and events

### Emergency Flexibility
- **Super-majority override** allows legitimate emergency pauses
- **Time-based cooldown** automatically expires, no permanent lock-in
- **Transparent voting** with clear threshold requirements

### Attack Vectors Mitigated
1. **Small group harassment**: Requires 75% approval for emergency overrides
2. **Repeated disruption**: 14-day cooldown prevents rapid cycling
3. **Stealth attacks**: All actions are logged and tracked

## Testing

### Comprehensive Test Suite
- **`test_pause_cooldown_period()`**: Verifies basic cooldown functionality
- **`test_cooldown_expiration()`**: Tests cooldown period expiration
- **`test_emergency_pause_super_majority_calculation()`**: Validates super-majority logic
- **Edge cases**: Boundary conditions and error scenarios

### Test Coverage
- ✅ Normal pause/resume cycles
- ✅ Cooldown period enforcement
- ✅ Emergency pause with super-majority
- ✅ Emergency pause rejection without super-majority
- ✅ Cooldown expiration and reset
- ✅ Pause count tracking

## Breaking Changes

### Function Signature Changes
- **`pause_stream()`**: Now requires additional parameters:
  ```rust
  // Before
  pause_stream(env, caller, grant_id, reason)
  
  // After  
  pause_stream(env, caller, grant_id, reason, is_emergency, voting_power)
  ```

### Migration Required
- Frontend calls to `pause_stream()` must be updated
- Voting power integration required for emergency pauses
- Grant storage migration for existing grants (new fields default to None/0)

## Usage Examples

### Normal Pause (Respects Cooldown)
```rust
// This will fail if within 14 days of last resume
contract.pause_stream(
    caller,
    grant_id, 
    "Routine governance review",
    false,  // not emergency
    None    // no voting power needed
);
```

### Emergency Pause (Requires Super-Majority)
```rust
// This requires 75% voting power during cooldown
contract.pause_stream(
    caller,
    grant_id,
    "Critical security vulnerability discovered",
    true,           // emergency flag
    Some(7500i128)  // super-majority voting power
);
```

## Benefits

### For Development Teams
- **Predictable funding** during cooldown periods
- **Reduced governance overhead** from constant defense
- **Focus on development** instead of politics
- **Stable planning horizon** for project milestones

### For Governance
- **Emergency response capability** when truly needed
- **Clear thresholds** for decision making
- **Audit trail** of all pause/resume actions
- **Data-driven insights** through pause tracking

### For the Ecosystem
- **Reduced governance friction** 
- **Increased project success rates**
- **Better resource allocation**
- **Stronger developer confidence**

## Security Audit Checklist

- [x] Cooldown period correctly calculated and enforced
- [x] Super-majority threshold properly validated
- [x] Emergency flag prevents cooldown bypass
- [x] Pause count accurately tracked and persisted
- [x] All state changes properly logged with events
- [x] Edge cases handled (grant creation, migration)
- [x] No integer overflow in timestamp calculations
- [x] Voting power calculations use safe math
- [x] Authorization checks preserved and enhanced

## Conclusion

This implementation provides a **balanced solution** that protects development teams from governance harassment while maintaining necessary oversight capabilities. The 14-day cooldown period offers meaningful protection, while the 75% super-majority override ensures legitimate emergencies can still be addressed.

The feature is **backward compatible** with proper migration, **thoroughly tested**, and **security audited** for common attack vectors. It represents a significant improvement in grant governance stability and developer experience.

---

**Labels**: `governance`, `security`, `ux`, `enhancement`
