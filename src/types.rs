use soroban_sdk::{contracterror, contracttype, Address, BytesN, String, Symbol};

/// Storage namespaces for the AgriTrust execution engine.
///
/// Every namespace is derived from a cryptographic commitment so that the
/// canonical location of an object is a function of its content identity:
///   * [`DataKey::Asset`] -> keyed by the asset twin UUID / hash.
///   * [`DataKey::DataContract`] -> keyed by the agreement identifier.
///   * [`DataKey::Policy`] -> keyed by the ODRL rule digest.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Singleton pointer to the protocol administrator.
    Admin,
    /// `:Asset` twin anchored by its physical batch commitment.
    Asset(BytesN<32>),
    /// `:DataContract` usage agreement anchored by its agreement id.
    DataContract(BytesN<32>),
    /// `:ODRLPolicy` usage policy anchored by its SHA-256 rule digest.
    Policy(BytesN<32>),
}

/// The digital twin of a physical agricultural batch.
///
/// This record is the on-ledger anchor binding a `:Process` provenance graph
/// (committed via `provenance_merkle_root`) to a real-world commodity batch.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetTwinRecord {
    /// Hash matching the physical batch (e.g. a coffee, cocoa or soya lot).
    pub asset_id: BytesN<32>,
    /// Ledger identity of the farmer / cooperative that controls this batch.
    ///
    /// Sovereignty anchor: this address is the only principal permitted to
    /// grant or revoke third-party access to `asset_id`.
    pub originator: Address,
    /// Decentralized identifier of the originator (e.g. `did:key:...`,
    /// `did:ion:...`) resolvable to a verifiable credential subject.
    pub originator_did: String,
    /// Commodity classification (e.g. `COFFEE`, `SOYA`, `CATTLE`).
    pub commodity_type: Symbol,
    /// Cryptographic commitment (Merkle root) over the supply chain
    /// `:Process` graph covering the batch lifecycle.
    pub provenance_merkle_root: BytesN<32>,
    /// Ledger timestamp at which the twin was anchored.
    pub registered_at: u64,
}

/// A decentralized data usage agreement (`:DataContract`).
///
/// Grants a named consumer access to an asset twin's compliance records for a
/// bounded number of ledgers, under machine-readable ODRL terms. The grant is
/// fully revocable by the farmer at any point (Farmer Data Sovereignty).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataContractRecord {
    /// Unique agreement identifier.
    pub contract_id: BytesN<32>,
    /// The `:Asset` twin whose compliance records are licensed.
    pub asset_id: BytesN<32>,
    /// Entity requesting access to compliance records.
    pub data_consumer: Address,
    /// SHA-256 digest of the machine-readable ODRL usage terms.
    pub odrl_policy_hash: BytesN<32>,
    /// Escrow payment required from the consumer for the access window.
    pub access_fee: i128,
    /// Last ledger sequence at which the grant remains enforceable.
    pub valid_until_ledger: u32,
    /// Set by the farmer to terminate the grant unilaterally.
    pub is_revoked: bool,
}

/// An anchored ODRL usage policy (`:ODRLPolicy`).
///
/// Anchoring the digest on-ledger is what makes an agreement enforceable: a
/// `:DataContract` may only reference a policy that has been committed here,
/// so consumers can verify the referenced rule set against an immutable,
/// publicly auditable registration rather than an opaque off-ledger hash.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ODRLPolicyRecord {
    /// SHA-256 digest of the canonical ODRL permission/constraint set.
    pub policy_hash: BytesN<32>,
    /// Resolvable location of the ODRL offer / permission document.
    pub policy_uri: String,
    /// Issuer that committed the policy to the ledger.
    pub issuer: Address,
    /// Ledger timestamp at which the policy was anchored.
    pub registered_at: u64,
}

/// Failures surfaced by the AgriTrust execution engine.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    /// The protocol administrator has already been established.
    AlreadyInitialized = 1,
    /// The caller is not the sovereign originator of the referenced asset.
    Unauthorized = 2,
    /// A referenced ledger object (asset twin or data contract) does not exist.
    AssetNotFound = 3,
    /// The agreement's validity window has elapsed.
    ContractExpired = 4,
    /// The referenced ODRL policy is not anchored, or terms are not satisfied.
    PolicyViolation = 5,
    /// The farmer has revoked this access grant.
    AccessRevoked = 6,
}
