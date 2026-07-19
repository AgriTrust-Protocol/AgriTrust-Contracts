use soroban_sdk::{contracttype, Bytes, BytesN};

// ── Constants ──────────────────────────────────────────────────────────────────

/// Soft storage-access budget. resolve_provenance() fails with
/// StorageBudgetExceeded before touching storage once this threshold is
/// reached. Leaves a 40-entry margin below Soroban's ~160 combined limit.
pub const STORAGE_BUDGET: u32 = 120;

/// Warning threshold: emitted as an event but execution continues.
pub const STORAGE_WARN_THRESHOLD: u32 = 100;

/// Maximum number of hops supported in a single resolve_provenance() call.
/// At 2 storage accesses per hop (prefetch read + output write) plus 2 for
/// the resolution result, 10 hops = 22 accesses — well within STORAGE_BUDGET.
pub const MAX_HOPS: u32 = 10;

/// Fixed-point precision for Score values: 1e7.
pub const SCORE_PRECISION: i128 = 10_000_000;

// ── Score ─────────────────────────────────────────────────────────────────────

/// Provenance quality score with 1e7 fixed-point precision.
/// Range: 0 to SCORE_PRECISION (0.0 to 1.0).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Score {
    /// Raw value; divide by SCORE_PRECISION for a float equivalent.
    pub raw: i128,
}

impl Score {
    pub const ZERO: Score = Score { raw: 0 };
    pub const MAX: Score = Score {
        raw: SCORE_PRECISION,
    };

    pub fn is_valid(&self) -> bool {
        self.raw >= 0 && self.raw <= SCORE_PRECISION
    }
}

// ── HopState ──────────────────────────────────────────────────────────────────

/// Compressed per-hop state stored as a single storage entry.
///
/// Combines what would otherwise be separate Metadata + AuditEntry entries
/// (2 reads) into one (1 read), halving the per-hop storage access cost.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HopState {
    /// Index of this hop in the chain (0-based).
    pub index: u32,
    /// Credential identifier (hash) for this hop's issuer.
    pub credential_id: BytesN<32>,
    /// Signature proof bytes (compact encoding).
    pub signature: BytesN<64>,
    /// Compressed metadata + audit log: policy reference hash + timestamp.
    pub policy_ref: BytesN<32>,
    /// Unix timestamp when this hop was recorded.
    pub recorded_at: u64,
    /// Aggregated provenance score at this hop.
    pub score: Score,
    /// True if the credential was verified at record time.
    pub credential_verified: bool,
}

// ── ProvenanceAccessSet ───────────────────────────────────────────────────────

/// Set of storage keys to prefetch for a full provenance chain.
///
/// Built before touching storage so all hop keys are known up-front,
/// enabling a single sequential prefetch pass instead of N on-demand reads.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProvenanceAccessSet {
    /// Sorted list of HopState storage keys to read (one per hop).
    pub hop_keys: soroban_sdk::Vec<Bytes>,
    /// Number of hops planned.
    pub hop_count: u32,
    /// Estimated storage access count for this set: hop_count reads +
    /// hop_count writes (output) + 2 for the final result entry.
    pub estimated_accesses: u32,
}

impl ProvenanceAccessSet {
    /// Compute estimated access count: 1 read/hop + 1 write/hop + 2 final.
    pub fn estimated_accesses(hop_count: u32) -> u32 {
        hop_count.saturating_mul(2).saturating_add(2)
    }
}

// ── StorageBudget ─────────────────────────────────────────────────────────────

/// Runtime storage access counter. Passed mutably through resolve_provenance()
/// and checked before every storage operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageBudget {
    pub used: u32,
    pub limit: u32,
    pub warn_threshold: u32,
    /// Detailed tracking for different types of storage operations
    pub reads: u32,
    pub writes: u32,
}

impl StorageBudget {
    pub fn new() -> Self {
        StorageBudget {
            used: 0,
            limit: STORAGE_BUDGET,
            warn_threshold: STORAGE_WARN_THRESHOLD,
            reads: 0,
            writes: 0,
        }
    }

    /// Increment counter by `n` and return whether the warn threshold is
    /// newly crossed (caller should emit a warning event).
    pub fn charge(&mut self, n: u32) -> bool {
        let before = self.used;
        self.used = self.used.saturating_add(n);
        before < self.warn_threshold && self.used >= self.warn_threshold
    }

    /// Charge for a read operation and return whether the warn threshold is
    /// newly crossed.
    pub fn charge_read(&mut self) -> bool {
        self.reads = self.reads.saturating_add(1);
        self.charge(1)
    }

    /// Charge for a write operation and return whether the warn threshold is
    /// newly crossed.
    pub fn charge_write(&mut self) -> bool {
        self.writes = self.writes.saturating_add(1);
        self.charge(1)
    }

    /// Returns true if adding `n` would exceed the hard limit.
    pub fn would_exceed(&self, n: u32) -> bool {
        self.used.saturating_add(n) > self.limit
    }

    /// Returns true if a read operation would exceed the hard limit.
    pub fn would_exceed_read(&self) -> bool {
        self.would_exceed(1)
    }

    /// Returns true if a write operation would exceed the hard limit.
    pub fn would_exceed_write(&self) -> bool {
        self.would_exceed(1)
    }
}

// ── ProvenanceResult ─────────────────────────────────────────────────────────

/// Output of a successful resolve_provenance() call.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProvenanceResult {
    /// Number of hops successfully resolved.
    pub hops_resolved: u32,
    /// Final aggregated score across all hops.
    pub final_score: Score,
    /// Total storage accesses consumed.
    pub storage_accesses_used: u32,
    /// Ledger sequence when resolution completed.
    pub resolved_at_ledger: u32,
}
