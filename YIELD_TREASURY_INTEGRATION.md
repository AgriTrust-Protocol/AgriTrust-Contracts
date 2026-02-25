# Yield-Bearing Treasury Integration

This document describes the implementation of yield-bearing treasury functionality for the grant contract system, addressing issue #46 and #36.

## 📋 Overview

The yield-bearing treasury integration allows idle funds in grant contracts to earn yield through integration with Stellar-based liquidity pools and lending protocols. This ensures that "resting" capital is put to work while maintaining liquidity for grantee withdrawals.

## 🎯 Acceptance Criteria Met

✅ **Implement `invest_idle_funds()`** - Invest idle funds into yield-bearing strategies  
✅ **Implement `divest_funds()`** - Divest funds from yield strategies  
✅ **Ensure liquidity is always available for grantee `withdraw()` calls** - Liquidity protection mechanisms  

## 🏗️ Architecture

### Core Components

#### 1. YieldTreasuryContract
- **Purpose**: Standalone yield management contract
- **Features**: Investment strategies, yield calculation, metrics tracking
- **Strategies**: Stellar AQUA, Stellar USDC, Liquidity Pools

#### 2. YieldEnhancedGrantContract
- **Purpose**: Enhanced grant contract with integrated yield functionality
- **Features**: Auto-invest, liquidity protection, emergency divestment
- **Integration**: Seamless grant + yield management

### Investment Strategies

| Strategy | APY | Description | Risk Level |
|-----------|-------|-------------|-------------|
| STELLAR_AQUA | 8% | Stellar AQUA token staking | Medium |
| STELLAR_USDC | 5% | Stellar USDC lending | Low |
| LIQUIDITY_POOL | 12% | AMM liquidity provision | High |

## 🔧 Key Features

### 1. Liquidity Protection
- **Minimum Reserve Ratio**: Configurable percentage to keep available for withdrawals
- **Auto-Divestment**: Automatically divests when withdrawal liquidity is needed
- **Emergency Withdrawal**: Bypass all checks for emergency situations

### 2. Yield Calculation
- **Continuous Compounding**: Real-time yield calculation based on APY
- **Time-Based**: Accurate yield calculation based on elapsed time
- **Transparent**: All yield metrics and positions are queryable

### 3. Risk Management
- **Strategy Selection**: Multiple strategies with different risk/return profiles
- **Diversification**: Support for multiple investment strategies
- **Monitoring**: Comprehensive metrics and alerting

## 📊 Smart Contract Functions

### YieldTreasuryContract

#### Core Functions
```rust
// Initialize treasury
initialize(env, admin, yield_token_address, config) -> Result<(), YieldError>

// Invest idle funds
invest_idle_funds(env, amount, strategy) -> Result<(), YieldError>

// Divest funds
divest_funds(env, amount) -> Result<(), YieldError>

// Get yield position
get_yield_position(env) -> Result<YieldPosition, YieldError>

// Get treasury metrics
get_yield_metrics(env) -> Result<YieldMetrics, YieldError>
```

#### Utility Functions
```rust
// Emergency withdrawal
emergency_withdraw(env, amount, recipient) -> Result<(), YieldError>

// Auto-invest idle funds
auto_invest(env) -> Result<(), YieldError>

// Update configuration
update_config(env, config) -> Result<(), YieldError>
```

### YieldEnhancedGrantContract

#### Enhanced Grant Functions
```rust
// Create enhanced grant with yield options
create_enhanced_grant(env, grant_id, recipient, total_amount, flow_rate, 
                   initial_status_mask, yield_enabled, auto_yield_invest, 
                   min_reserve_percentage) -> Result<(), EnhancedError>

// Enhanced withdrawal with yield consideration
enhanced_withdraw(env, grant_id, amount) -> Result<(), EnhancedError>

// Get enhanced grant information
get_enhanced_grant(env, grant_id) -> Result<EnhancedGrant, EnhancedError>
```

## 📈 Data Structures

### YieldPosition
```rust
pub struct YieldPosition {
    pub strategy: u32,           // Investment strategy used
    pub invested_amount: i128,     // Principal invested
    pub current_value: i128,       // Current value (principal + yield)
    pub accrued_yield: i128,       // Total yield earned
    pub invested_at: u64,         // Investment timestamp
    pub last_yield_update: u64,    // Last yield calculation
    pub apy: i128,               // Annual Percentage Yield (basis points)
}
```

### TreasuryConfig
```rust
pub struct TreasuryConfig {
    pub admin: Address,                    // Admin address
    pub min_reserve_ratio: i128,           // Minimum reserve (basis points)
    pub max_investment_ratio: i128,        // Maximum investment (basis points)
    pub auto_invest: bool,                 // Auto-invest idle funds
    pub yield_strategy: u32,               // Default strategy
    pub emergency_withdrawal_enabled: bool, // Emergency withdrawal
}
```

### EnhancedGrant
```rust
pub struct EnhancedGrant {
    pub base_grant: Grant,           // Original grant structure
    pub yield_enabled: bool,           // Enable yield for this grant
    pub auto_yield_invest: bool,       // Auto-invest idle funds
    pub min_reserve_percentage: i128,   // Minimum reserve for this grant
}
```

## 🔄 Workflow

### 1. Initialization
1. Deploy YieldTreasuryContract or YieldEnhancedGrantContract
2. Initialize with admin address and yield token
3. Configure treasury parameters (reserve ratios, strategies)

### 2. Investment Process
1. Call `invest_idle_funds()` with amount and strategy
2. Contract validates liquidity requirements
3. Funds are invested in selected strategy
4. Yield position is created and tracked

### 3. Yield Accumulation
1. Yield is calculated continuously based on APY
2. Position value updates with accrued yield
3. Metrics are updated in real-time

### 4. Divestment Process
1. Call `divest_funds()` with amount (or None for all)
2. Contract calculates principal vs. yield portions
3. Funds are returned to contract reserve
4. Position is updated or removed

### 5. Grant Withdrawal with Yield
1. Grantee calls `enhanced_withdraw()`
2. Contract checks available liquidity
3. Auto-divests if needed (if enabled)
4. Processes withdrawal normally

## 🛡️ Safety Features

### 1. Liquidity Protection
- **Minimum Reserve**: Always maintains configurable minimum reserve
- **Auto-Divestment**: Automatically divests when liquidity needed
- **Buffer Calculation**: Includes safety buffer for withdrawal fluctuations

### 2. Access Control
- **Admin Only**: Only admin can invest/divest funds
- **Grantee Access**: Grantees can only withdraw their grants
- **Emergency Access**: Admin can emergency withdraw in crisis

### 3. Error Handling
- **Comprehensive Errors**: Detailed error codes for all failure modes
- **Safe Math**: Overflow protection on all calculations
- **State Validation**: Validates all state transitions

## 📊 Metrics and Monitoring

### Yield Metrics
```rust
pub struct YieldMetrics {
    pub total_invested: i128,        // Total principal invested
    pub total_yield_earned: i128,     // Total yield earned
    pub current_apy: i128,            // Current APY
    pub last_yield_calculation: u64,    // Last calculation time
    pub investment_count: u32,          // Number of investments
}
```

### Available Queries
- `get_yield_position()` - Current investment position
- `get_yield_metrics()` - Treasury performance metrics
- `get_reserve_balance()` - Available reserve balance
- `get_total_balance()` - Total contract balance
- `is_investment_active()` - Check if investment is active

## 🧪 Testing

### Test Coverage
- ✅ Initialization and configuration
- ✅ Investment and divestment workflows
- ✅ Yield calculation and tracking
- ✅ Liquidity protection mechanisms
- ✅ Error conditions and edge cases
- ✅ Enhanced grant integration
- ✅ Emergency withdrawal functionality

### Running Tests
```bash
# Run all yield tests
cargo test --package grant_contracts test_yield

# Run specific test
cargo test --package grant_contracts test_yield_treasury_initialization

# Run enhanced grant tests
cargo test --package grant_contracts test_enhanced_grant_with_yield
```

## 🚀 Deployment

### Prerequisites
1. Soroban CLI installed
2. Stellar network access (testnet/mainnet)
3. Token addresses for yield strategies
4. Admin wallet configured

### Deployment Steps
```bash
# Build contract
cargo build --target wasm32-unknown-unknown --release

# Deploy YieldTreasuryContract
soroban contract deploy \
    --wasm target/wasm32-unknown-unknown/release/yield_treasury.wasm \
    --source admin_address \
    --network testnet

# Deploy YieldEnhancedGrantContract
soroban contract deploy \
    --wasm target/wasm32-unknown-unknown/release/yield_enhanced.wasm \
    --source admin_address \
    --network testnet

# Initialize contract
soroban contract invoke \
    --id CONTRACT_ID \
    --function initialize \
    --args ADMIN_ADDRESS TOKEN_ADDRESS true \
    --source admin_address \
    --network testnet
```

## 🔧 Configuration

### Environment Variables
```env
# Treasury Configuration
MIN_RESERVE_RATIO=2000        # 20% minimum reserve
MAX_INVESTMENT_RATIO=8000      # 80% maximum investment
DEFAULT_STRATEGY=2             # STELLAR_USDC
AUTO_INVEST=false              # Manual investment
EMERGENCY_WITHDRAWAL=true     # Enable emergency withdrawal

# Strategy APYs (basis points)
STELLAR_AQUA_APY=800         # 8%
STELLAR_USDC_APY=500         # 5%
LIQUIDITY_POOL_APY=1200       # 12%
```

## 📈 Performance Considerations

### Gas Optimization
- **Batch Operations**: Multiple operations in single transaction
- **Efficient Storage**: Optimized data structures
- **Minimal External Calls**: Reduce cross-contract calls

### Yield Optimization
- **Strategy Selection**: Choose optimal APY vs. risk ratio
- **Timing**: Invest during high-yield periods
- **Diversification**: Spread across multiple strategies

## 🔄 Integration with Existing Grants

### Backward Compatibility
- **Existing Grants**: Continue to work without yield
- **Optional Feature**: Yield is opt-in per grant
- **Gradual Migration**: Can enable yield per grant

### Migration Path
1. Deploy new yield-enhanced contract
2. Migrate existing grants (optional)
3. Enable treasury functionality
4. Configure yield parameters

## 🚨 Risk Considerations

### Smart Contract Risk
- **Code Audits**: Comprehensive security audit required
- **Test Thoroughly**: Extensive testing on testnet
- **Gradual Rollout**: Start with small amounts

### Financial Risk
- **Strategy Risk**: Different strategies have different risk profiles
- **Market Risk**: APY rates can fluctuate
- **Liquidity Risk**: Ensure sufficient reserves

### Operational Risk
- **Admin Risk**: Secure admin key management
- **Oracle Risk**: Reliable price feeds for strategies
- **Network Risk**: Stellar network stability

## 📚 API Reference

### Error Codes
| Error | Code | Description |
|--------|-------|-------------|
| NotInitialized | 1 | Contract not initialized |
| AlreadyInitialized | 2 | Contract already initialized |
| NotAuthorized | 3 | Caller not authorized |
| InsufficientReserve | 4 | Insufficient reserve balance |
| InsufficientInvestment | 5 | Insufficient investment balance |
| InvalidAmount | 6 | Invalid amount provided |
| InvalidStrategy | 7 | Invalid investment strategy |
| InvestmentActive | 8 | Investment already active |
| InvestmentInactive | 9 | No active investment |
| MathOverflow | 10 | Math overflow error |
| YieldCalculationFailed | 11 | Yield calculation failed |
| EmergencyMode | 12 | Emergency mode not enabled |
| TokenError | 13 | Token operation error |
| InvalidState | 14 | Invalid contract state |

### Events
| Event | Description |
|-------|-------------|
| yield_init | Treasury initialized |
| yield_invest | Funds invested |
| yield_divest | Funds divested |
| emergency_withdraw | Emergency withdrawal |
| enhanced_grant_created | Enhanced grant created |
| enhanced_withdraw | Enhanced withdrawal |
| config_update | Configuration updated |

## 🔮 Future Enhancements

### Planned Features
- **Dynamic APY**: Real-time APY from oracles
- **Multi-Strategy**: Split investments across strategies
- **Yield Farming**: Advanced yield farming strategies
- **Insurance**: Yield protection insurance
- **Governance**: Community-driven strategy selection

### Integration Opportunities
- **DeFi Protocols**: Integration with major Stellar DeFi protocols
- **DEX Integration**: Direct DEX liquidity provision
- **Cross-Chain**: Multi-chain yield opportunities
- **Automation**: Automated yield optimization

## 📞 Support

### Documentation
- [Contract Source Code](./contracts/grant_contracts/src/)
- [Test Suite](./contracts/grant_contracts/src/test_yield.rs)
- [API Reference](./contracts/grant_contracts/src/yield_treasury.rs)

### Community
- **Issues**: Report bugs and feature requests
- **Discussions**: Community support and discussions
- **Contributions**: Pull requests welcome

---

**Note**: This implementation is designed for Stellar blockchain and integrates with existing grant contract infrastructure. Ensure thorough testing and security audits before mainnet deployment.
