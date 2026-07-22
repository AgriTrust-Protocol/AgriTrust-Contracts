use soroban_sdk::contracttype;

#[derive(Clone)]
#[contracttype]
pub enum GrantStatus {
    Active,
    Completed,
    Cancelled,
}

#[derive(Clone)]
#[contracttype]
pub struct Grant {
    pub status: GrantStatus,
    pub remaining_balance: i128,
    pub withdrawable_balance: i128,
}

/// Immutable record written to persistent storage after a grant is archived.
/// Keyed by `(grant_id, archive_version)`, so competing archivals of the
/// same grant each produce a distinct tombstone rather than silently
/// overwriting each other.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct GrantTombstone {
    /// Snapshot of the grant at the moment it was archived.
    pub original_grant: Grant,
    /// Ledger sequence when the archive occurred.
    pub archived_at_ledger: u32,
    /// Monotonically increasing version for this grant's tombstone lineage.
    pub archive_version: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum GrantError {
    GrantNotFound = 1,
    InvalidStatus = 2,
    NonZeroBalance = 3,
    /// Returned when a concurrent `archive_grant` is in progress for the
    /// same `grant_id` (detected by an extant `ArchiveLock`).
    ConcurrentArchiveInProgress = 4,
}
