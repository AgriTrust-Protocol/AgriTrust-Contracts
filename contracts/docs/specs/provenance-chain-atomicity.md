# Provenance Chain Atomicity & Multi-Hop Verification Specification

## Overview
This specification details the cryptographic architecture and concurrency controls that guarantee atomic, unbroken multi-hop Merkle chains across agricultural batch certifications (Farm → Processor → Distributor → Retailer).

## Threat Model & Race Condition Analysis
Prior implementations computed inter-hop links using a read-check-write pattern against a standalone `current_chain_tip` key. Under high-throughput batching where multiple supply chain participants submitted certification hops within identical ledger windows, race conditions emerged:
1. Two signers (e.g., Farm and Processor) concurrently read tip $H_{t}$.
2. Both computed next tip hashes $H_{t+1}^{(A)}$ and $H_{t+1}^{(B)}$.
3. The second transaction clobbered the first, resulting in orphan hops and broken provenance verification.

## Formal Architecture & Invariants

### 1. Append-Only State Container
Instead of a mutable singleton tip key, provenance chains are maintained as an atomic, append-only `Vec<Hop>` encapsulated in `BatchProvenanceChain`:
```rust
pub struct BatchProvenanceChain {
    pub batch_id: BytesN<32>,
    pub hops: Vec<Hop>,
    pub tip_hash: BytesN<32>,
    pub is_finalized: bool,
}
```

### 2. Sequence Number Guard
Every hop submission enforces strict positional determinism:
$$\text{require}(\text{hop.sequence} == \text{chain.hops.len()})$$
Any transaction executing out of order or attempting to insert into an already advanced tip fails immediately with `Error::SequenceMismatch`.

### 3. Cryptographic Merkle Node Chaining
Hop digests are deterministically linked:
$$H_0 = 0^{32} \quad (\text{Genesis})$$
$$H_k = \text{SHA256}(H_{k-1} \parallel M_k)$$
where $M_k$ is the stage certification metadata hash. Mismatched predecessor hashes or modified payloads trigger `Error::ChainBroken`.

### 4. Mutual Exclusion & Deadlock Prevention
Mutations acquire a batch-level mutual exclusion lock (`ProvenanceLock`). To prevent permanent lockouts from aborted transactions:
$$\text{elapsed\_ledgers} = \text{current\_ledger} - \text{locked\_at\_ledger}$$
If $\text{elapsed\_ledgers} > 2$, the lock is deemed expired and automatically recoverable.

## Verification Complexity
- **Storage Access Complexity**: $O(1)$ read and $O(1)$ write per hop appended.
- **Verification Complexity**: $O(N)$ sequential verification from Genesis to Tip over bounded stages ($N \le 32$).
