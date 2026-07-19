use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// Returned when the storage access counter would exceed STORAGE_BUDGET
    /// before completing the call. Fail-fast — no partial state is written.
    StorageBudgetExceeded = 1,

    /// The hop chain supplied to resolve_provenance() exceeds MAX_HOPS.
    ChainTooLong = 2,

    /// A HopState entry expected in storage was not found.
    HopNotFound = 3,

    /// The credential referenced by a HopState failed signature verification.
    InvalidHopSignature = 4,

    /// The credential referenced by a HopState is expired or not yet valid.
    InvalidHopCredential = 5,

    /// A Score value outside [0, SCORE_PRECISION] was encountered.
    InvalidScore = 6,

    /// resolve_provenance() was called with zero hops.
    EmptyChain = 7,

    /// Emitted when STORAGE_WARN_THRESHOLD is crossed (non-fatal; used in
    /// events, not as a return error).
    StorageAccessWarning = 8,

    NotInitialized = 9,
    Unauthorized = 10,
    InvalidQuantity = 11,
    TokenNotFound = 12,
    TokenBurned = 13,
    SoulboundTransfer = 14,
    UnknownCertificateType = 15,
    CertificateExpired = 16,
    CertificateNotFound = 17,
    UnknownComplianceStandard = 18,
    EventNotFound = 19,
    IncompatibleBatch = 20,
}
