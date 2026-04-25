# Issues #273, #274, #275, #276 Implementation Plan

## Overview

This document outlines the implementation plan for four critical issues in the Grant-Stream protocol:

1. **#273** - SEP-40 Compliant Oracle Integration for USDC/XLM Pricing
2. **#274** - Clawback-Resilient Internal Ledger Accounting
3. **#275** - Sanction-Screening Middleware Hook for SEP-12
4. **#276** - Quadratic Voting for Community Grant Allocation

---

## Issue #273: SEP-40 Compliant Oracle Integration

### Description
Grant-Stream needs accurate pricing to calculate pro-rata distributions. This requires building a consumer interface for SEP-40 compliant oracles to fetch latest price feeds for asset pairs (e.g., XLM/USDC).

### Key Requirements
- **SEP-40 Compliance**: Implement standard Stellar oracle interface
- **Asset Pair Support**: Focus on XLM/USDC pricing
- **Staleness Checks**: Contract must revert if oracle data is older than 20 minutes
- **Pro-rata Calculations**: Ensure mathematically correct distributions based on current market state

### Technical Implementation

#### Oracle Interface Structure
```rust
pub struct OraclePriceFeed {
    pub base_asset: Address,
    pub quote_asset: Address,
    pub price: i128,
    pub timestamp: u64,
    pub decimals: u32,
}

pub trait SEP40Oracle {
    fn get_price(&self, base: Address, quote: Address) -> Result<OraclePriceFeed, Error>;
    fn get_timestamp(&self) -> Result<u64, Error>;
}
```

#### Staleness Check Logic
```rust
const MAX_STALENESS_SECONDS: u64 = 1200; // 20 minutes

fn verify_price_freshness(timestamp: u64) -> Result<(), Error> {
    let current_time = env::ledger().timestamp();
    if current_time.saturating_sub(timestamp) > MAX_STALENESS_SECONDS {
        Err(Error::StalePriceData)
    } else {
        Ok(())
    }
}
```

#### Price Conversion Functions
```rust
fn convert_usd_to_xlm(usd_amount: i128, xlm_usdc_price: i128) -> Result<i128, Error> {
    // Fixed-point arithmetic for precise conversion
    let scaled_amount = usd_amount * PRICE_SCALING_FACTOR;
    Ok(scaled_amount / xlm_usdc_price)
}
```

### Integration Points
- **Stream Initialization**: Validate oracle data before stream creation
- **Withdrawal Processing**: Use current price for USD-denominated grants
- **Emergency Controls**: Circuit breaker if oracle becomes unreliable

---

## Issue #274: Clawback-Resilient Internal Ledger Accounting

### Description
Since Stellar allows issuers to claw back regulated assets, this task involves refactoring the BalanceTracker struct to use "Relative Proportions" rather than absolute numbers.

### Key Requirements
- **Relative Proportions**: Track "X shares of current pool" instead of "100 USDC"
- **Dynamic Adjustment**: Contract math adjusts when physical balance changes
- **No Underflow Errors**: Continue functioning despite external clawbacks
- **Proportional Accuracy**: Maintain fair distribution ratios

### Technical Implementation

#### Current vs. New Structure
```rust
// Current (Vulnerable)
pub struct BalanceTracker {
    pub total_balance: i128,
    pub user_balances: Map<Address, i128>,
}

// New (Clawback-Resilient)
pub struct BalanceTracker {
    pub total_shares: i128,
    pub user_shares: Map<Address, i128>,
    pub last_known_balance: i128,
    pub balance_timestamp: u64,
}
```

#### Share-Based Accounting
```rust
fn calculate_user_balance(user: Address) -> Result<i128, Error> {
    let user_shares = user_shares.get(user).unwrap_or(0);
    let current_physical_balance = get_current_balance();
    
    if total_shares == 0 {
        return Ok(0);
    }
    
    // Proportional calculation
    let user_balance = (user_shares * current_physical_balance) / total_shares;
    Ok(user_balance)
}
```

#### Balance Update Handler
```rust
fn handle_balance_change() -> Result<(), Error> {
    let current_balance = get_current_balance();
    let last_balance = last_known_balance;
    
    if current_balance != last_balance {
        // Balance changed due to clawback or deposit
        last_known_balance = current_balance;
        balance_timestamp = env::ledger().timestamp();
        
        // Emit event for transparency
        events::balance_adjusted(current_balance, last_balance);
    }
    
    Ok(())
}
```

### Migration Strategy
- **Snapshot Current State**: Record all absolute balances
- **Convert to Shares**: Calculate proportional shares based on total pool
- **Update All References**: Modify withdrawal and deposit logic
- **Test Clawback Scenarios**: Verify resilience under various conditions

---

## Issue #275: Sanction-Screening Middleware Hook for SEP-12

### Description
For institutional compliance, this requires a hook that queries a centralized "Sanctions Registry" contract before any stream is initialized to check if a grantee's Stellar Public Key is on a denylist.

### Key Requirements
- **SEP-12 Integration**: Use Stellar identity protocol standards
- **Cross-Contract Calls**: Query external Sanctions Registry
- **Pre-Initialization Check**: Block stream creation for sanctioned addresses
- **OFAC Compliance**: Align with international financial regulations

### Technical Implementation

#### Sanctions Registry Interface
```rust
pub trait SanctionsRegistry {
    fn is_sanctioned(address: Address) -> Result<bool, Error>;
    fn get_sanction_reason(address: Address) -> Result<Option<String>, Error>;
    fn update_registry(updates: Vec<Address>) -> Result<(), Error>;
}
```

#### Compliance Hook Structure
```rust
pub struct ComplianceHook {
    pub sanctions_registry: Address,
    pub check_enabled: bool,
    pub exemption_list: Set<Address>,
}

impl ComplianceHook {
    fn check_grantee_eligibility(grantee: Address) -> Result<(), Error> {
        if !check_enabled {
            return Ok(());
        }
        
        // Check exemptions first
        if exemption_list.contains(grantee) {
            return Ok(());
        }
        
        // Query sanctions registry
        let registry = SanctionsRegistry::new(sanctions_registry);
        let is_sanctioned = registry.is_sanctioned(grantee)?;
        
        if is_sanctioned {
            Err(Error::SanctionedAddress)
        } else {
            Ok(())
        }
    }
}
```

#### Stream Initialization Integration
```rust
pub fn initialize_stream(
    grantee: Address,
    amount: i128,
    // ... other parameters
) -> Result<StreamId, Error> {
    // Compliance check first
    ComplianceHook::check_grantee_eligibility(grantee)?;
    
    // Continue with normal stream creation
    // ...
}
```

#### SEP-12 Identity Verification
```rust
fn verify_identity_with_sep12(grantee: Address) -> Result<bool, Error> {
    // Optional: Cross-reference with SEP-12 identity verification
    let identity_contract = get_sep12_contract();
    identity_contract.verify_identity(grantee)
}
```

### Governance Considerations
- **Registry Updates**: Mechanism for updating sanctions list
- **Appeal Process**: Procedure for false positives
- **Transparency**: Public audit trail of compliance checks

---

## Issue #276: Quadratic Voting for Community Grant Allocation

### Description
To prevent "Whale" dominance in grant selection, implement a Quadratic Voting (QV) module where the cost of votes follows quadratic progression (1 vote = 1 token, 4 votes = 16 tokens).

### Key Requirements
- **Quadratic Cost Function**: Cost = votes²
- **Fixed-Point Math**: Work within Soroban's mathematical constraints
- **Democratic Distribution**: Emphasize unique supporters over wallet depth
- **Vote Tracking**: Maintain accurate vote cost calculations

### Technical Implementation

#### Quadratic Voting Math
```rust
// Fixed-point arithmetic constants
const QV_PRECISION: u128 = 1_000_000; // 6 decimal places

fn calculate_vote_cost(votes: u128) -> Result<u128, Error> {
    // Cost = votes² using fixed-point arithmetic
    let votes_scaled = votes * QV_PRECISION;
    let cost_scaled = votes_scaled * votes_scaled / QV_PRECISION;
    Ok(cost_scaled / QV_PRECISION)
}

fn calculate_votes_from_cost(cost: u128) -> Result<u128, Error> {
    // votes = √cost using approximation
    let cost_scaled = cost * QV_PRECISION;
    let votes_scaled = integer_sqrt(cost_scaled);
    Ok(votes_scaled / QV_PRECISION)
}
```

#### Voting Module Structure
```rust
pub struct QuadraticVoting {
    pub proposals: Map<ProposalId, Proposal>,
    pub voter_tokens_spent: Map<Address, u128>,
    pub proposal_votes: Map<(ProposalId, Address), u128>,
    pub total_tokens_spent: u128,
}

pub struct Proposal {
    pub id: ProposalId,
    pub grant_amount: i128,
    pub description: String,
    pub votes_received: u128,
    pub unique_voters: u32,
    pub deadline: u64,
}
```

#### Vote Casting Logic
```rust
pub fn cast_vote(
    voter: Address,
    proposal_id: ProposalId,
    votes: u128,
) -> Result<(), Error> {
    // Calculate cost
    let cost = calculate_vote_cost(votes)?;
    
    // Check voter balance
    let voter_balance = token_balance(voter);
    let already_spent = voter_tokens_spent.get(voter).unwrap_or(0);
    
    if voter_balance < already_spent + cost {
        return Err(Error::InsufficientTokens);
    }
    
    // Update records
    voter_tokens_spent.set(voter, already_spent + cost);
    
    let current_votes = proposal_votes.get((proposal_id, voter)).unwrap_or(0);
    proposal_votes.set((proposal_id, voter), current_votes + votes);
    
    // Update proposal stats
    let mut proposal = proposals.get(proposal_id).unwrap();
    proposal.votes_received += votes;
    if current_votes == 0 {
        proposal.unique_voters += 1;
    }
    proposals.set(proposal_id, proposal);
    
    Ok(())
}
```

#### Grant Allocation Algorithm
```rust
fn allocate_grants() -> Result<Vec<GrantAllocation>, Error> {
    let mut allocations = Vec::new();
    let total_votes: u128 = proposals.values()
        .map(|p| p.votes_received)
        .sum();
    
    for proposal in proposals.values() {
        if proposal.votes_received == 0 {
            continue;
        }
        
        // Quadratic weight calculation
        let quadratic_weight = proposal.votes_received * proposal.votes_received;
        let allocation_ratio = quadratic_weight / total_votes;
        let grant_amount = (total_pool * allocation_ratio as i128) / 1_000_000;
        
        allocations.push(GrantAllocation {
            proposal_id: proposal.id,
            amount: grant_amount,
            votes: proposal.votes_received,
            unique_voters: proposal.unique_voters,
        });
    }
    
    Ok(allocations)
}
```

### Anti-Manipulation Measures
- **Vote Limits**: Maximum votes per proposal per address
- **Time Locks**: Prevent rapid vote changes
- **Sybil Resistance**: Identity verification requirements

---

## Implementation Timeline

### Phase 1: Foundation (Week 1-2)
- [ ] Set up SEP-40 oracle interface (#273)
- [ ] Implement basic price fetching and staleness checks
- [ ] Create clawback-resilient balance tracking structure (#274)

### Phase 2: Integration (Week 3-4)
- [ ] Integrate oracle pricing into stream calculations
- [ ] Migrate balance tracking to share-based system
- [ ] Implement sanctions screening hook (#275)

### Phase 3: Governance (Week 5-6)
- [ ] Build quadratic voting module (#276)
- [ ] Integrate voting with grant allocation
- [ ] Add compliance and governance controls

### Phase 4: Testing & Security (Week 7-8)
- [ ] Comprehensive testing of all modules
- [ ] Security audits and penetration testing
- [ ] Documentation and deployment preparation

---

## Security Considerations

### Oracle Security
- **Multiple Sources**: Validate against multiple price feeds
- **Circuit Breakers**: Automatic shutdown on anomalous prices
- **Update Frequency**: Balance between freshness and gas costs

### Balance Security
- **Reentrancy Protection**: Prevent manipulation during balance updates
- **Integer Overflow**: Use checked arithmetic for all calculations
- **Audit Trail**: Log all balance changes for transparency

### Compliance Security
- **Privacy Protection**: Minimize data exposure in compliance checks
- **False Positive Handling**: Appeal mechanisms for incorrect sanctions
- **Registry Integrity**: Secure updates to sanctions lists

### Voting Security
- **Vote Buying Detection**: Monitor for suspicious voting patterns
- **Collusion Prevention**: Limits on coordinated voting
- **Result Integrity**: Cryptographic proof of vote calculations

---

## Testing Strategy

### Unit Tests
- Oracle price fetching and staleness validation
- Share-based balance calculations under various scenarios
- Compliance check logic with test addresses
- Quadratic voting mathematics and edge cases

### Integration Tests
- End-to-end stream creation with oracle pricing
- Clawback scenarios and balance recovery
- Sanction screening in stream initialization
- Complete voting and allocation cycles

### Stress Tests
- High-frequency oracle updates
- Massive clawback events
- Large-scale voting scenarios
- Concurrent operations and race conditions

### Security Tests
- Oracle manipulation attempts
- Balance underflow/overflow attacks
- Compliance bypass attempts
- Voting Sybil attacks

---

## Conclusion

These four issues represent critical infrastructure improvements for the Grant-Stream protocol:

1. **#273** ensures accurate, real-time pricing for fair distributions
2. **#274** provides resilience against regulatory clawbacks
3. **#275** maintains institutional compliance through automated screening
4. **#276** enables democratic, anti-whale governance through quadratic voting

The implementation requires careful attention to Soroban's constraints, Stellar's unique features, and the specific needs of the grant streaming use case. Proper testing and security audits are essential before deployment.
