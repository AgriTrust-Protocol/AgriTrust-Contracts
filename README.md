## Deployed Contract

- **Network:** Stellar Testnet
- **Contract ID:** CD6OGC46OFCV52IJQKEDVKLX5ASA3ZMSTHAAZQIPDSJV6VZ3KUJDEP4D

## Grant State Flowchart

```mermaid
stateDiagram-v2
    [*] --> Proposed : create_grant()
    
    Proposed --> Active : activate_grant()
    Proposed --> Cancelled : cancel_grant()
    
    Active --> Paused : pause_grant()
    Active --> Completed : All milestones approved
    
    Paused --> Active : resume_grant()
    Paused --> Cancelled : cancel_grant()
    
    Completed --> [*]
    Cancelled --> [*]
    
    note right of Proposed : Admin can create grant<br/>Admin can cancel
    note right of Active : Admin can pause<br/>Milestones can be approved
    note right of Paused : Admin can resume<br/>Admin can cancel
    note right of Completed : All funds released<br/>No further actions
    note right of Cancelled : Grant terminated<br/>No further actions
```

## Grant Lifecycle State Diagram

```mermaid
stateDiagram-v2
    [*] --> Proposed : create_grant()
    Proposed --> Active : activate_grant()
    Proposed --> Cancelled : cancel_grant()
    Active --> Cliff : enter_cliff()
    Cliff --> Streaming : start_stream()
    Streaming --> Paused/Slashed : pause_or_slash()
    Paused/Slashed --> Streaming : resume_stream()
    Streaming --> Completed : finish_stream()
    Streaming --> Cancelled : cancel_during_stream()

    note right of Proposed : Admin can create\nAdmin can cancel
    note right of Active : Admin can activate\nMay enter cliff
    note right of Cliff : System moves to streaming
    note right of Streaming : Admin / Oracle actions
    note right of Paused/Slashed : Admin/Oracle
    note right of Completed : Grants done
    note right of Cancelled : Grant terminated


## State Transitions and Permissions

| From State | To State | Trigger | Who Can Trigger |
|------------|-----------|-----------|------------------|
| Proposed | Active | `activate_grant()` | Admin |
| Proposed | Cancelled | `cancel_grant()` | Admin |
| Active | Paused | `pause_grant()` | Admin |
| Active | Completed | All milestones approved | Admin (via milestone approvals) |
| Paused | Active | `resume_grant()` | Admin |
| Paused | Cancelled | `cancel_grant()` | Admin |

## Grant Features

### Token Support
- **Multi-token support**: Grants can be created with any SAC token (USDC, XLM, AQUA, etc.)
- **Transfer fee handling**: Contract detects and handles tokens with transfer fees
- **Balance tracking**: Contract maintains accurate balance tracking for all token types

### Security Features
- **Checks-Effects-Interactions pattern**: All state updates happen before external calls
- **Access control**: Only authorized users can perform specific actions
- **Overflow protection**: Safe arithmetic operations throughout the contract

### Grant Management
- **Milestone-based releases**: Funds released when milestones are approved
- **Pause/Resume functionality**: Grants can be paused for extended periods
- **Long duration support**: Tested with pause durations up to 100 years

## Troubleshooting

If you encounter generic error codes (e.g., `Error(7)`) during interaction, please refer to the [Error Codes Mapping](ERRORS.md) for human-readable explanations.
