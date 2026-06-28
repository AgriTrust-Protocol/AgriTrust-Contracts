# Optimistic Concurrency Control — 2PC Protocol Specification

## Overview

This module implements an optimistic concurrency control (OCC) system for batch state transitions on the AgriTrust protocol. It uses a two-phase commit (2PC) protocol with per-batch sequence counters as a linearization point to prevent race conditions in concurrent optimistic transactions.

## Problem Statement

The original implementation had a race condition:
1. Two `begin_optimistic()` calls start from version N concurrently
2. Both produce `PendingMutation` entries
3. First `commit_optimistic()` succeeds (version N→N+1)
4. Second `commit_optimistic()` detects version mismatch but only deletes the pending entry — no compensation is logged, breaking auditability

## Resolution: Two-Phase Commit with Compensation

### Protocol Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                    TWO-PHASE COMMIT (2PC)                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  1. BEGIN                                                       │
│     ├─ Assign next seq_no (atomic counter per batch)            │
│     ├─ Capture previous state values                            │
│     ├─ Store PendingMutation{Pending, version, seq_no}         │
│     └─ Return seq_no                                            │
│                                                                 │
│  2. COMMIT                                                      │
│     Phase 1: Linearization Check                                │
│     ├─ Verify seq_no == current_seq_no + 1                      │
│     ├─ If not: apply compensation, return false                 │
│     └─ If yes: set ReadyToCommit flag                          │
│                                                                 │
│     Phase 2: Apply State                                        │
│     ├─ Write state updates to storage                           │
│     ├─ Increment version and seq_no                             │
│     └─ Mark Committed                                           │
│                                                                 │
│  3. ROLLBACK                                                    │
│     ├─ Restore previous values (compensating action)            │
│     ├─ Log CompensationEntry                                    │
│     └─ Mark RolledBack                                          │
│                                                                 │
│  4. EXPIRE                                                      │
│     ├─ Anyone can call after expires_at                         │
│     ├─ Only valid for Pending/ReadyToCommit mutations           │
│     └─ Mark Expired                                            │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Linearization Guarantees

### Sequence Counter (Per-Batch FIFO)

Each batch maintains an atomic sequence counter. The key invariant:

```
commit_optimistic(seq_no) succeeds ⟺ seq_no == current_seq_no + 1
```

This ensures:
- **Total ordering**: Mutations commit in strict FIFO order per batch
- **No gaps**: Every committed seq_no is consecutive
- **Single writer**: Only one caller can commit at any time per batch

### Linearizability Proof

For any execution, there exists a sequential execution that produces the same final state:

1. Each `commit_optimistic()` that succeeds has a linearization point at the moment it reads `current_seq_no`
2. The seq_no assignment in `begin_optimistic()` determines the linearization order
3. Failed commits (out-of-order) are compensated, preserving the sequential illusion

## Compensating Transaction Pattern

When a version conflict is detected:

```
┌──────────────────────────────────────────────┐
│         COMPENSATION FLOW                     │
├──────────────────────────────────────────────┤
│                                              │
│  Conflict Detected (seq_no != expected)      │
│       │                                      │
│       ▼                                      │
│  Create CompensationEntry:                   │
│    - original_mutation_id                    │
│    - compensation_state (prev_values)        │
│    - reason: "out_of_order"                 │
│    - timestamp                               │
│       │                                      │
│       ▼                                      │
│  Mark original mutation: RolledBack         │
│       │                                      │
│       ▼                                      │
│  Emit "compensated" event                   │
│                                              │
│  Invariant: ∀ rollback ∃ compensation       │
│                                              │
└──────────────────────────────────────────────┘
```

### Compensation Reasons

| Reason | Description |
|--------|-------------|
| `out_of_order` | seq_no mismatch — another mutation committed first |
| `explicit_rb` | Explicit rollback by authorized party |
| `expired` | Mutation timed out before commit |
| `rb_module` | Rollback via rollback module |

## State Machine

```
                    ┌──────────┐
                    │  START   │
                    └────┬─────┘
                         │ begin_optimistic()
                         ▼
                    ┌──────────┐
            ┌──────│ PENDING  │◄──────────┐
            │      └────┬─────┘           │
            │           │                 │
            │ commit    │ rollback        │ rollback_mutation()
            │ (Phase 1) │                 │
            │           ▼                 │
            │      ┌──────────┐          │
            │      │READY_2PC │──────────┤
            │      └────┬─────┘          │
            │           │                 │
            │ commit    │ expire (timeout)│
            │ (Phase 2) │                 │
            │           ▼                 ▼
            │      ┌──────────┐    ┌──────────┐
            └─────►│COMMITTED │    │ ROLLED   │
                   └──────────┘    │  BACK    │
                                   └──────────┘
                         │
                         │ expire (timeout)
                         ▼
                    ┌──────────┐
                    │ EXPIRED  │
                    └──────────┘
```

## Parameters

| Parameter | Value | Description |
|-----------|-------|-------------|
| `OPTIMISTIC_LOCK_TIMEOUT` | 10 | Ledger closes before mutation expires (~50s) |
| `MAX_CONCURRENT_MUTATIONS` | 5 | Max pending mutations per batch |

## Storage Layout

| Key | Type | Description |
|-----|------|-------------|
| `Version` | `StateVersion` | Current version and seq_no |
| `PendingMap(batch_id)` | `Vec<PendingMutation>` | All pending mutations for a batch |
| `CompensationMap(mutation_id)` | `Vec<CompensationEntry>` | Compensation chain |
| `BatchCounter(batch_id)` | `u64` | Next seq_no for batch |
| `ReadyToCommit(mutation_id)` | `bool` | Phase 1 flag |
| `StateValue(key)` | `Bytes` | Actual state values |

## Security Properties

1. **No unauthorized state changes**: Only committed mutations modify state
2. **Auditability**: Every rollback has a corresponding compensation entry
3. **Bounded concurrency**: Max 5 pending mutations per batch
4. **Timeout safety**: Stale mutations expire after ~50s
5. **Linearization**: Per-batch FIFO ordering prevents out-of-order commits

## Events

| Event | Topics | Data |
|-------|--------|------|
| `begin_opt` | `(batch_id, mutation_id)` | `seq_no` |
| `committed` | `(batch_id, mutation_id)` | `(version, seq_no)` |
| `compensated` | `(batch_id, mutation_id)` | `current_seq_no` |
| `rolled_back` | `(batch_id, mutation_id)` | `timestamp` |
| `expired` | `(batch_id, mutation_id)` | `timestamp` |

## Implementation Notes

- Uses Soroban persistent storage (survives contract restarts)
- All arithmetic uses safe patterns (no overflow)
- Events are published for off-chain indexing
- Property-based tests verify linearizability with 20 concurrent mutations
