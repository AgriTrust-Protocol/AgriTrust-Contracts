# Security Model & Threat Map

This document defines the protocol security assumptions, trust boundaries, and key failure scenarios for the grant streaming contracts in this repository.

## Scope

- `contracts/grant_contracts` (core admin-managed streaming grant logic)
- Related operational assumptions for deployment on Stellar/Soroban

## Security Objectives

- Prevent unauthorized fund movement.
- Restrict privileged state transitions to authorized roles.
- Keep grant accounting bounded (`withdrawn + claimable <= total_amount`).
- Ensure availability degrades safely during infrastructure/network incidents.

## Trust Assumptions

1. Admin key security
- The configured Admin address can execute privileged operations (for example: create/cancel grants, pause/resume in optimized flows, rate updates, emergency reassignment, token rescue).
- Security assumes the Admin signer(s) are not compromised.

2. Correct role configuration
- Admin, Treasury, and optional Oracle addresses are set correctly at initialization.
- No immutable governance layer is enforced on-chain in this repository; safe operation depends on operational governance off-chain.

3. Stellar/Soroban liveness and finality
- Transactions eventually execute and finalize on Stellar.
- Contract behavior assumes ledger timestamps advance and are monotonic.

4. Token contract behavior
- Referenced token contracts follow expected transfer/balance semantics.
- Fee-on-transfer or non-standard token behavior may affect net outcomes unless explicitly handled by specific modules.

5. Off-chain operators and monitoring
- Teams monitor events, alerts, and privileged activity.
- Incident response exists for signer compromise and abnormal on-chain behavior.

## Privileged Role Model

- Admin: highest privilege ("god-mode" relative to normal grantees).
- Recipient/Grantee: can withdraw own claimable funds and call recipient-scoped actions.
- Oracle (when enabled): can apply KPI multiplier logic.
- Public callers: may trigger specific permissionless flows like inactivity slashing (as implemented).

## God-Mode Admin Key Threats and Mitigations

### Threat 1: Admin key compromise

Risk:
- Attacker can perform any admin-authorized action, including fund redirection patterns exposed by admin-only functions.

Mitigations:
- Use Stellar multi-sig with thresholded signers (no single hot key control).
- Store signing keys in HSM/MPC; avoid raw key export.
- Use separate keys by environment (dev/test/prod) and by function where possible.
- Enforce operational timelocks/approval workflows off-chain for sensitive admin actions.
- Real-time alerting on admin calls and unexpected destination addresses.
- Maintain a tested key-rotation and emergency admin replacement runbook.

### Threat 2: Malicious or mistaken admin action

Risk:
- Valid signatures can still execute harmful or incorrect state transitions.

Mitigations:
- Two-person review for privileged transactions.
- Preflight simulation and deterministic transaction review before submission.
- Change-management policy with explicit rollback/containment procedures.
- Publicly documented governance policy for emergency powers and limits.

### Threat 3: Admin key loss/unavailability

Risk:
- Inability to perform necessary operations (grant creation/cancellation, emergency actions).

Mitigations:
- Redundant signer set with threshold design to tolerate signer loss.
- Periodic disaster-recovery drills for signer replacement.
- Offline recovery materials with strict access controls.

## Stellar Downtime / Network Incident Model

### What happens during downtime or severe degradation

- New transactions may be delayed or not accepted until the network recovers.
- No contract state changes occur while transactions are not executing.
- Time-dependent grant accrual settles on the next successful state-changing call using ledger time; users may observe delayed but catch-up settlement behavior.

### Risks

- Withdrawal and admin operations become temporarily unavailable.
- Operational queues/backlogs form after recovery.
- Market and UX risk from delayed fund access.

### Mitigations

- Communicate degraded mode clearly to users during outages.
- Maintain operational playbooks for post-outage reconciliation.
- Use monitoring on ledger health, RPC health, and transaction inclusion latency.
- After recovery, process critical operations in priority order (user withdrawals, safety actions, routine admin actions).

## Threat Map

| Threat | Actor | Impact | Existing Control | Recommended Hardening |
|---|---|---|---|---|
| Admin key compromise | External attacker | Full admin action surface abuse | `require_auth` on admin-gated functions | Multi-sig threshold, HSM/MPC, real-time alerting, emergency rotation |
| Malicious admin | Insider | Harmful but authorized changes | Role-gated calls | Multi-party governance, approvals, auditable runbooks |
| Oracle compromise (if enabled) | External/insider | Manipulated KPI-based rate updates | Oracle-specific auth | Isolated oracle keys, bounded multipliers, monitoring |
| Stellar downtime | Infrastructure/network | Temporary loss of write availability | Safe no-write during outage | Incident comms, backlog processing playbook, health monitoring |
| Token integration risk | External dependency | Unexpected transfer/accounting behavior | Contract-side checks vary by module | Token allowlist + integration tests per token type |
| Monitoring failure | Ops gap | Delayed detection of attacks/mistakes | On-chain events available | 24/7 alerting + response SLOs |

## Residual Risk

Even with hardening, the Admin role remains a high-trust component. The protocol should be treated as "admin-governed" unless governance controls are cryptographically constrained on-chain.

## Operational Minimum Baseline

- Threshold multi-sig Admin on Stellar.
- Hardware/MPC-backed signer custody.
- Incident response runbook for compromised/lost signers.
- Continuous monitoring of privileged calls and treasury movements.
- Periodic security review of admin and oracle permissions.
