# Delegated Clawback Authority for Sub-DAOs

## Overview

The Delegated Clawback Authority system enables large DAOs to establish hierarchical permission structures where Sub-DAOs (working groups) can manage specific grants within their domain of expertise, while the main DAO retains ultimate veto power.

## Problem Statement

**Current Challenge:**
- Large DAOs have specialized working groups (Marketing, Engineering, Operations, etc.)
- Centralized grant management creates bottlenecks
- Department experts lack direct control over relevant grants
- Main DAO members may lack domain-specific expertise for timely decisions

**Solution:**
- Hierarchical permission system with specialized oversight
- Sub-DAOs can pause/clawback grants within their jurisdiction
- Main DAO retains veto power over all Sub-DAO actions
- Comprehensive audit trail and accountability

## Architecture

### Permission Levels

```rust
pub enum PermissionLevel {
    None,      // No permissions
    Pause,     // Can pause/resume grants
    Clawback,  // Can pause/resume and cancel grants
    Full,      // All permissions including rate changes
}
```

### Key Components

1. **Sub-DAO Authority Contract** (`sub_dao_authority.rs`)
   - Permission management and validation
   - Action logging and audit trail
   - Veto system implementation

2. **Enhanced Grant Contract** (`lib.rs`)
   - Integrated Sub-DAO authorization checks
   - Delegated pause/resume/cancel functions
   - Event emission for transparency

3. **Hierarchical Permission Model**
   - Main DAO admin controls Sub-DAO permissions
   - Sub-DAOs manage assigned grants
   - Main DAO can veto any Sub-DAO action

## Implementation Details

### 1. Sub-DAO Permission Structure

```rust
pub struct SubDaoPermission {
    pub sub_dao_address: Address,
    pub department: String,              // e.g., "Marketing", "Engineering"
    pub permission_level: PermissionLevel,
    pub status: SubDaoStatus,           // Active, Suspended, Revoked
    pub granted_at: u64,
    pub granted_by: Address,            // Main DAO admin
    pub expires_at: Option<u64>,        // Optional expiration
    pub max_grant_amount: i128,         // Maximum total amount they can manage
    pub managed_grants: Vec<u64>,       // List of grant IDs they manage
}
```

### 2. Action Logging and Veto System

```rust
pub struct ActionLog {
    pub action_id: u64,
    pub sub_dao_address: Address,
    pub action_type: String,            // "pause", "resume", "cancel"
    pub grant_id: u64,
    pub action_data: String,            // Reason/details
    pub executed_at: u64,
    pub vetoed: bool,
    pub veto_id: Option<u64>,
}

pub struct VetoRecord {
    pub veto_id: u64,
    pub sub_dao_address: Address,
    pub action_type: String,
    pub grant_id: u64,
    pub veto_reason: String,
    pub vetoed_by: Address,             // Main DAO admin
    pub vetoed_at: u64,
    pub original_action_timestamp: u64,
}
```

### 3. Enhanced Grant Functions

**Delegated Pause Function:**
```rust
pub fn pause_stream(env: Env, caller: Address, grant_id: u64, reason: String) -> Result<u64, Error>
```

**Delegated Resume Function:**
```rust
pub fn resume_stream(env: Env, caller: Address, grant_id: u64, reason: String) -> Result<u64, Error>
```

**Delegated Clawback Function:**
```rust
pub fn cancel_grant(env: Env, caller: Address, grant_id: u64, reason: String) -> Result<u64, Error>
```

## Usage Examples

### Setting Up Sub-DAO Authority

```rust
// 1. Initialize Sub-DAO Authority contract
SubDaoAuthority::initialize(env, main_dao_admin_address)?;

// 2. Set up integration with Grant Contract
GrantContract::set_sub_dao_authority_contract(
    env,
    grant_contract_admin,
    sub_dao_authority_contract_address,
)?;
```

### Creating Sub-DAOs with Permissions

```rust
// Engineering Sub-DAO with full permissions
SubDaoAuthority::grant_sub_dao_permissions(
    env,
    main_dao_admin,
    engineering_dao_address,
    "Engineering",
    PermissionLevel::Full,
    5_000_000, // $5M max grant amount
    None, // No expiration
)?;

// Marketing Sub-DAO with pause-only permissions
SubDaoAuthority::grant_sub_dao_permissions(
    env,
    main_dao_admin,
    marketing_dao_address,
    "Marketing",
    PermissionLevel::Pause,
    2_000_000, // $2M max grant amount
    Some(expiration_timestamp),
)?;
```

### Assigning Grants to Sub-DAOs

```rust
// Assign engineering grants to Engineering Sub-DAO
SubDaoAuthority::assign_grant_to_sub_dao(
    env,
    main_dao_admin,
    engineering_dao_address,
    101, // grant_id
)?;

SubDaoAuthority::assign_grant_to_sub_dao(
    env,
    main_dao_admin,
    engineering_dao_address,
    102, // grant_id
)?;

SubDaoAuthority::assign_grant_to_sub_dao(
    env,
    main_dao_admin,
    marketing_dao_address,
    201, // grant_id
)?;
```

### Sub-DAO Actions

```rust
// Engineering Sub-DAO pauses a grant for review
let action_id = GrantContract::pause_stream(
    env,
    engineering_dao_address,
    101,
    "Code review identified security issues".to_string(),
)?;

// Marketing Sub-DAO resumes a grant after issues resolved
let action_id = GrantContract::resume_stream(
    env,
    marketing_dao_address,
    201,
    "Campaign compliance verified".to_string(),
)?;

// Engineering Sub-DAO clawbacks a failing project
let action_id = GrantContract::cancel_grant(
    env,
    engineering_dao_address,
    102,
    "Project failed to meet technical milestones".to_string(),
)?;
```

### Main DAO Veto Power

```rust
// Main DAO vetoes a Sub-DAO action
let veto_id = SubDaoAuthority::veto_sub_dao_action(
    env,
    main_dao_admin,
    action_id, // Action to veto
    "Project is actually meeting milestones - pause not justified".to_string(),
)?;

// Check veto details
let veto_record = SubDaoAuthority::get_veto_record(env, veto_id)?;
```

## Event Emissions

The system emits comprehensive events for transparency:

```rust
// Permission management
("permission_granted", sub_dao_address, department, permission_level, max_amount)
("permission_revoked", sub_dao_address, reason)
("subdao_suspended", sub_dao_address, reason)
("subdao_unsuspended", sub_dao_address)

// Grant assignment
("grant_assigned", sub_dao_address, grant_id)

// Delegated actions
("delegated_pause", sub_dao_address, grant_id, action_id, reason)
("delegated_resume", sub_dao_address, grant_id, action_id, reason)
("delegated_clawback", sub_dao_address, grant_id, action_id, reason)

// Admin actions
("admin_pause", grant_id, admin_address, reason)
("admin_resume", grant_id, admin_address, reason)
("admin_cancel", grant_id, admin_address, reason)

// Veto actions
("action_vetoed", sub_dao_address, action_id, veto_id, veto_reason)
```

## Security Features

### 1. Hierarchical Authorization
- Main DAO admin controls all Sub-DAO permissions
- Sub-DAOs can only act on assigned grants
- Permission levels enforce capability boundaries

### 2. Expiration and Limits
- Optional permission expiration dates
- Maximum grant amount limits per Sub-DAO
- Department-based organization for clear boundaries

### 3. Comprehensive Audit Trail
- Every action logged with timestamp and reason
- Veto records maintain decision history
- Full traceability of all grant management decisions

### 4. Veto Power
- Main DAO can override any Sub-DAO action
- Veto reasons recorded for transparency
- Prevents abuse while enabling specialized oversight

### 5. Status Management
- Sub-DAOs can be suspended (temporary) or revoked (permanent)
- Granular control over Sub-DAO access
- Emergency response capabilities

## Error Handling

```rust
pub enum SubDaoError {
    NotInitialized = 2001,
    NotAuthorized = 2003,
    SubDaoNotFound = 2004,
    InsufficientPermissions = 2005,
    SubDaoSuspended = 2006,
    GrantNotManaged = 2007,
    ExceededMaxAmount = 2008,
    PermissionExpired = 2012,
    MainDaoVeto = 2013,
    // ... more errors
}
```

## Department Organization

Sub-DAOs are organized by departments for clear jurisdiction:

```rust
// Engineering Department
- Sub-DAO A: Backend Infrastructure (Full permissions)
- Sub-DAO B: Frontend Development (Clawback permissions)
- Sub-DAO C: DevOps (Pause permissions)

// Marketing Department  
- Sub-DAO D: Social Media (Pause permissions)
- Sub-DAO E: Content Creation (Clawback permissions)

// Operations Department
- Sub-DAO F: Community Management (Full permissions)
- Sub-DAO G: Event Planning (Pause permissions)
```

## Testing

Comprehensive test suite covering:

- Permission management and validation
- Grant assignment and management
- Delegated actions (pause/resume/clawback)
- Main DAO veto functionality
- Department organization
- Error conditions and edge cases
- Permission expiration
- Suspension and revocation

Run tests with:
```bash
cargo test --package grant_contracts --lib test_sub_dao_authority
```

## Deployment Steps

1. **Deploy Sub-DAO Authority Contract**
   - Initialize with main DAO admin
   - Set up department structures

2. **Update Grant Contract**
   - Set Sub-DAO authority contract address
   - Enable delegated functionality

3. **Create Sub-DAOs**
   - Grant appropriate permissions
   - Set department assignments
   - Configure limits and expirations

4. **Assign Grants**
   - Allocate grants to appropriate Sub-DAOs
   - Verify jurisdiction boundaries

5. **Test Integration**
   - Validate delegated actions
   - Test veto functionality
   - Verify audit trail

## Benefits

### 1. Specialized Oversight
- Domain experts manage relevant grants
- Faster decision-making for technical issues
- Better understanding of project progress

### 2. Scalable Governance
- Main DAO focuses on strategic decisions
- Sub-DAOs handle operational grant management
- Clear delegation of authority

### 3. Accountability and Transparency
- Comprehensive audit trail
- Veto power prevents abuse
- Public record of all decisions

### 4. Flexibility
- Granular permission levels
- Department-based organization
- Temporary suspensions for investigations

### 5. Risk Management
- Main DAO retains ultimate control
- Permission limits prevent overreach
- Expiration dates for time-bound authority

## Use Cases

### 1. Engineering DAO
- **Sub-DAOs**: Backend, Frontend, DevOps, Security
- **Permissions**: Full technical control
- **Oversight**: Code quality, security reviews, technical milestones

### 2. Marketing DAO  
- **Sub-DAOs**: Social Media, Content, Events, Analytics
- **Permissions**: Pause and resume campaigns
- **Oversight**: Brand compliance, campaign performance

### 3. Investment DAO
- **Sub-DAOs**: Due Diligence, Portfolio Management, Risk Assessment
- **Permissions**: Full investment authority within limits
- **Oversight**: Investment criteria, risk management

### 4. Community DAO
- **Sub-DAOs**: Moderation, Events, Support, Content
- **Permissions**: Community management authority
- **Oversight**: Community guidelines, engagement metrics

## Future Enhancements

1. **Cross-Department Collaboration**: Allow Sub-DAOs to collaborate on grants
2. **Dynamic Permission Adjustment**: Automatic permission scaling based on performance
3. **Reputation System**: Sub-DAO reputation affecting permission levels
4. **Multi-Sig Requirements**: Require multiple Sub-DAO approvals for large actions
5. **Automated Monitoring**: AI-powered anomaly detection in Sub-DAO actions

## Conclusion

The Delegated Clawback Authority system provides a robust, scalable solution for large DAOs to maintain effective governance while enabling specialized oversight. By combining hierarchical permissions with veto power and comprehensive audit trails, it creates the perfect balance between autonomy and control.

This implementation addresses the core challenges of large-scale DAO governance, enabling domain experts to make timely decisions while maintaining overall accountability and strategic oversight by the main DAO.
