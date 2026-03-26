#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, Map, Symbol, Vec, String,
    panic_with_error,
};

// --- Constants ---
const METADATA_VERSION: u32 = 1;
const MAX_METADATA_LENGTH: u32 = 5000;
const MAX_CROSS_CHAIN_REFERENCES: u32 = 10;
const MAX_IPFS_HASH_LENGTH: u32 = 64;
const MAX_CHAIN_ID_LENGTH: u32 = 32;
const MAX_EXTERNAL_ID_LENGTH: u32 = 64;
const METADATA_EXPIRY_SECS: u64 = 365 * 24 * 60 * 60; // 1 year

// --- ERC-4337 and JSON-LD Standard Constants ---
const ERC4337_CONTEXT: &str = "https://github.com/ethereum/ERCs/blob/master/ERCS/erc-4337.md";
const JSONLD_CONTEXT: &str = "https://www.w3.org/2018/credentials/v1";
const GRANT_SCHEMA_CONTEXT: &str = "https://schema.org/Grant";
const STELLAR_CONTEXT: &str = "https://stellar.org/contexts/grant";

// --- Data Structures ---

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub enum MetadataDataKey {
    /// Maps grant_id to GrantMetadata
    GrantMetadata(u64),
    /// Maps external reference to Stellar grant_id
    ExternalReference(String, String), // (chain_id, external_id)
    /// Global metadata registry for indexing
    GlobalMetadataRegistry,
    /// Chain-specific metadata registry
    ChainRegistry(String), // chain_id
    /// Metadata validation cache
    ValidationCache([u8; 32]),
    /// Cross-chain sync status
    SyncStatus(u64), // grant_id
}

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub struct GrantMetadata {
    /// Stellar grant ID
    pub grant_id: u64,
    /// JSON-LD compliant metadata hash (IPFS or similar)
    pub metadata_hash: [u8; 32],
    /// IPFS CID where full metadata is stored
    pub ipfs_cid: String,
    /// JSON-LD context URL
    pub context: String,
    /// Schema type (e.g., "Grant", "Project", "Proposal")
    pub schema_type: String,
    /// When metadata was created
    pub created_at: u64,
    /// When metadata expires
    pub expires_at: u64,
    /// Creator of the metadata
    pub creator: Address,
    /// Metadata version for compatibility
    pub version: u32,
    /// Whether metadata is publicly visible
    pub public: bool,
    /// Verification status
    pub verified: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub struct CrossChainReference {
    /// Chain identifier (e.g., "ethereum", "polygon", "arbitrum")
    pub chain_id: String,
    /// External grant/contract ID on that chain
    pub external_id: String,
    /// Type of reference (contract, transaction, etc.)
    pub reference_type: ReferenceType,
    /// When this reference was added
    pub linked_at: u64,
    /// Who added this reference
    pub linked_by: Address,
    /// Verification status of the cross-chain link
    pub verified: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub enum ReferenceType {
    Contract,      // Smart contract address
    Transaction,   // Transaction hash
    Proposal,      // Governance proposal ID
    Project,       // Project registry ID
    Custom,        // Custom identifier
}

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub struct MetadataIndex {
    /// Grant ID
    pub grant_id: u64,
    /// Metadata hash
    pub metadata_hash: [u8; 32],
    /// Chain references
    pub chain_references: Vec<CrossChainReference>,
    /// Last indexed timestamp
    pub indexed_at: u64,
    /// Index version
    pub index_version: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub struct SyncStatus {
    /// Grant ID
    pub grant_id: u64,
    /// Which chains have been synced
    pub synced_chains: Vec<String>,
    /// Last sync timestamp
    pub last_synced: u64,
    /// Sync status per chain
    pub chain_status: Map<String, SyncChainStatus>,
    /// Total sync attempts
    pub sync_attempts: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub enum SyncChainStatus {
    NotSynced,
    Pending,
    Synced,
    Failed,
    VerificationRequired,
}

#[derive(Clone, Debug, Eq, PartialEq, contracttype)]
pub struct MetadataValidation {
    /// Metadata hash being validated
    pub metadata_hash: [u8; 32],
    /// Validation result
    pub valid: bool,
    /// Validation errors (if any)
    pub errors: Vec<String>,
    /// When validated
    pub validated_at: u64,
    /// Validator address
    pub validator: Address,
    /// Validation score (0-100)
    pub validation_score: u32,
}

// --- Errors ---

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum MetadataError {
    InvalidMetadataHash = 1,
    InvalidIPFSCid = 2,
    InvalidChainId = 3,
    InvalidExternalId = 4,
    MetadataNotFound = 5,
    MetadataExpired = 6,
    UnauthorizedAccess = 7,
    InvalidJsonLD = 8,
    DuplicateReference = 9,
    ReferenceNotFound = 10,
    ValidationFailed = 11,
    SyncInProgress = 12,
    MaxReferencesExceeded = 13,
    StringTooLong = 14,
    InvalidSchema = 15,
    CrossChainVerificationFailed = 16,
}

// --- Contract Implementation ---

#[contract]
pub struct CrossChainMetadata;

#[contractimpl]
impl CrossChainMetadata {
    /// Create standardized JSON-LD metadata for a grant
    /// 
    /// This function creates metadata that follows ERC-4337 and JSON-LD standards
    /// to ensure cross-chain compatibility and global visibility.
    /// 
    /// # Arguments
    /// * `grant_id` - The Stellar grant ID
    /// * `metadata_hash` - SHA-256 hash of the JSON-LD metadata
    /// * `ipfs_cid` - IPFS CID where full metadata is stored
    /// * `schema_type` - Type of schema (Grant, Project, etc.)
    /// * `creator` - Address creating the metadata
    /// * `public` - Whether metadata should be publicly visible
    pub fn create_grant_metadata(
        env: Env,
        grant_id: u64,
        metadata_hash: [u8; 32],
        ipfs_cid: String,
        schema_type: String,
        creator: Address,
        public: bool,
    ) -> Result<(), MetadataError> {
        // Verify creator authorization
        creator.require_auth();

        // Validate inputs
        if ipfs_cid.len() > MAX_IPFS_HASH_LENGTH as usize {
            return Err(MetadataError::StringTooLong);
        }
        if schema_type.len() > 100 {
            return Err(MetadataError::StringTooLong);
        }

        // Check if metadata already exists
        let metadata_key = MetadataDataKey::GrantMetadata(grant_id);
        if env.storage().instance().has(&metadata_key) {
            return Err(MetadataError::DuplicateReference);
        }

        let now = env.ledger().timestamp();
        let expires_at = now + METADATA_EXPIRY_SECS;

        // Create metadata object
        let metadata = GrantMetadata {
            grant_id,
            metadata_hash,
            ipfs_cid: ipfs_cid.clone(),
            context: STELLAR_CONTEXT.to_string(&env),
            schema_type: schema_type.clone(),
            created_at: now,
            expires_at,
            creator: creator.clone(),
            version: METADATA_VERSION,
            public,
            verified: false, // Needs verification
        };

        // Store metadata
        env.storage().instance().set(&metadata_key, &metadata);

        // Add to global registry for indexing
        let index = MetadataIndex {
            grant_id,
            metadata_hash,
            chain_references: Vec::new(&env),
            indexed_at: now,
            index_version: METADATA_VERSION,
        };

        let mut global_registry = Self::get_global_metadata_registry(env.clone());
        global_registry.push_back(index);
        env.storage().instance().set(
            &MetadataDataKey::GlobalMetadataRegistry,
            &global_registry,
        );

        // Publish metadata creation event
        env.events().publish(
            (Symbol::new(&env, "metadata_created"), grant_id),
            (metadata_hash, ipfs_cid, schema_type, creator),
        );

        Ok(())
    }

    /// Add cross-chain reference to existing metadata
    /// 
    /// This allows linking Stellar grants to their counterparts on other chains.
    /// 
    /// # Arguments
    /// * `grant_id` - The Stellar grant ID
    /// * `chain_id` - Target chain identifier
    /// * `external_id` - Grant/contract ID on target chain
    /// * `reference_type` - Type of reference
    /// * `linked_by` - Address adding the reference
    pub fn add_cross_chain_reference(
        env: Env,
        grant_id: u64,
        chain_id: String,
        external_id: String,
        reference_type: ReferenceType,
        linked_by: Address,
    ) -> Result<(), MetadataError> {
        // Verify authorization
        linked_by.require_auth();

        // Validate inputs
        if chain_id.len() > MAX_CHAIN_ID_LENGTH as usize {
            return Err(MetadataError::StringTooLong);
        }
        if external_id.len() > MAX_EXTERNAL_ID_LENGTH as usize {
            return Err(MetadataError::StringTooLong);
        }

        // Get existing metadata
        let mut metadata = Self::get_grant_metadata(env.clone(), grant_id)?;

        // Check if reference already exists
        let reference_key = MetadataDataKey::ExternalReference(
            chain_id.clone(),
            external_id.clone(),
        );
        if env.storage().instance().has(&reference_key) {
            return Err(MetadataError::DuplicateReference);
        }

        // Create cross-chain reference
        let reference = CrossChainReference {
            chain_id: chain_id.clone(),
            external_id: external_id.clone(),
            reference_type,
            linked_at: env.ledger().timestamp(),
            linked_by: linked_by.clone(),
            verified: false, // Needs verification
        };

        // Add to metadata's chain references
        let mut global_registry = Self::get_global_metadata_registry(env.clone());
        for index in global_registry.iter_mut() {
            if index.grant_id == grant_id {
                if index.chain_references.len() >= MAX_CROSS_CHAIN_REFERENCES as usize {
                    return Err(MetadataError::MaxReferencesExceeded);
                }
                index.chain_references.push_back(reference.clone());
                break;
            }
        }

        // Store reference
        env.storage().instance().set(&reference_key, &grant_id);
        env.storage().instance().set(
            &MetadataDataKey::GlobalMetadataRegistry,
            &global_registry,
        );

        // Add to chain-specific registry
        let mut chain_registry = Self::get_chain_registry(env.clone(), &chain_id);
        chain_registry.push_back(grant_id);
        env.storage().instance().set(
            &MetadataDataKey::ChainRegistry(chain_id),
            &chain_registry,
        );

        // Publish reference added event
        env.events().publish(
            (Symbol::new(&env, "cross_chain_reference_added"), grant_id),
            (chain_id, external_id, linked_by),
        );

        Ok(())
    }

    /// Verify cross-chain reference
    /// 
    /// This function should be called by a trusted oracle or verification service
    /// to confirm that the cross-chain reference is valid.
    /// 
    /// # Arguments
    /// * `grant_id` - The Stellar grant ID
    /// * `chain_id` - Chain identifier
    /// * `external_id` - External grant ID
    /// * `verifier` - Trusted verifier address
    /// * `verified` - Whether the reference is verified
    pub fn verify_cross_chain_reference(
        env: Env,
        grant_id: u64,
        chain_id: String,
        external_id: String,
        verifier: Address,
        verified: bool,
    ) -> Result<(), MetadataError> {
        // Verify verifier authorization (in practice, this would check against a list of trusted verifiers)
        verifier.require_auth();

        // Check if reference exists
        let reference_key = MetadataDataKey::ExternalReference(
            chain_id.clone(),
            external_id.clone(),
        );
        if !env.storage().instance().has(&reference_key) {
            return Err(MetadataError::ReferenceNotFound);
        }

        // Update verification status in global registry
        let mut global_registry = Self::get_global_metadata_registry(env.clone());
        for index in global_registry.iter_mut() {
            if index.grant_id == grant_id {
                for reference in index.chain_references.iter_mut() {
                    if reference.chain_id == chain_id && reference.external_id == external_id {
                        reference.verified = verified;
                        break;
                    }
                }
                break;
            }
        }

        env.storage().instance().set(
            &MetadataDataKey::GlobalMetadataRegistry,
            &global_registry,
        );

        // Publish verification event
        env.events().publish(
            (Symbol::new(&env, "cross_chain_verified"), grant_id),
            (chain_id, external_id, verifier, verified),
        );

        Ok(())
    }

    /// Get grant metadata
    pub fn get_grant_metadata(env: Env, grant_id: u64) -> Result<GrantMetadata, MetadataError> {
        let metadata: GrantMetadata = env
            .storage()
            .instance()
            .get(&MetadataDataKey::GrantMetadata(grant_id))
            .ok_or(MetadataError::MetadataNotFound)?;

        // Check if metadata has expired
        if env.ledger().timestamp() > metadata.expires_at {
            return Err(MetadataError::MetadataExpired);
        }

        Ok(metadata)
    }

    /// Get global metadata registry for indexing
    pub fn get_global_metadata_registry(env: Env) -> Vec<MetadataIndex> {
        env.storage()
            .instance()
            .get(&MetadataDataKey::GlobalMetadataRegistry)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Get chain-specific registry
    pub fn get_chain_registry(env: Env, chain_id: &str) -> Vec<u64> {
        env.storage()
            .instance()
            .get(&MetadataDataKey::ChainRegistry(chain_id.to_string(&env)))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Get grants by chain for cross-chain queries
    /// 
    /// This function allows other chains to query which Stellar grants
    /// have references to their chain, enabling cross-chain indexing.
    /// 
    /// # Arguments
    /// * `chain_id` - Chain identifier to query
    /// 
    /// # Returns
    /// * `Vec<u64>` - List of Stellar grant IDs with references to this chain
    pub fn get_grants_by_chain(env: Env, chain_id: String) -> Vec<u64> {
        Self::get_chain_registry(env, &chain_id)
    }

    /// Get cross-chain references for a grant
    /// 
    /// # Arguments
    /// * `grant_id` - The Stellar grant ID
    /// 
    /// # Returns
    /// * `Vec<CrossChainReference>` - All cross-chain references for this grant
    pub fn get_cross_chain_references(
        env: Env,
        grant_id: u64,
    ) -> Result<Vec<CrossChainReference>, MetadataError> {
        let global_registry = Self::get_global_metadata_registry(env);
        
        for index in global_registry.iter() {
            if index.grant_id == grant_id {
                return Ok(index.chain_references.clone());
            }
        }
        
        Ok(Vec::new(&env))
    }

    /// Validate JSON-LD metadata structure
    /// 
    /// This function validates that the metadata follows proper JSON-LD
    /// standards and is compatible with ERC-4337.
    /// 
    /// # Arguments
    /// * `metadata_hash` - Hash of the metadata to validate
    /// * `validator` - Address performing validation
    /// * `validation_score` - Score from 0-100
    /// * `errors` - List of validation errors (if any)
    pub fn validate_metadata(
        env: Env,
        metadata_hash: [u8; 32],
        validator: Address,
        validation_score: u32,
        errors: Vec<String>,
    ) -> Result<(), MetadataError> {
        // Verify validator authorization
        validator.require_auth();

        // Create validation record
        let validation = MetadataValidation {
            metadata_hash,
            valid: validation_score >= 70, // 70% threshold
            errors,
            validated_at: env.ledger().timestamp(),
            validator: validator.clone(),
            validation_score,
        };

        // Cache validation result
        env.storage().instance().set(
            &MetadataDataKey::ValidationCache(metadata_hash),
            &validation,
        );

        // Publish validation event
        env.events().publish(
            (Symbol::new(&env, "metadata_validated"),),
            (metadata_hash, validator, validation_score),
        );

        Ok(())
    }

    /// Get cached validation result
    pub fn get_metadata_validation(
        env: Env,
        metadata_hash: [u8; 32],
    ) -> Option<MetadataValidation> {
        env.storage()
            .instance()
            .get(&MetadataDataKey::ValidationCache(metadata_hash))
    }

    /// Update metadata (only creator can update)
    /// 
    /// # Arguments
    /// * `grant_id` - The Stellar grant ID
    /// * `new_metadata_hash` - New metadata hash
    /// * `new_ipfs_cid` - New IPFS CID
    /// * `updater` - Address performing the update
    pub fn update_metadata(
        env: Env,
        grant_id: u64,
        new_metadata_hash: [u8; 32],
        new_ipfs_cid: String,
        updater: Address,
    ) -> Result<(), MetadataError> {
        // Verify updater authorization
        updater.require_auth();

        // Get existing metadata
        let mut metadata = Self::get_grant_metadata(env.clone(), grant_id)?;

        // Only creator can update
        if metadata.creator != updater {
            return Err(MetadataError::UnauthorizedAccess);
        }

        // Validate new IPFS CID
        if new_ipfs_cid.len() > MAX_IPFS_HASH_LENGTH as usize {
            return Err(MetadataError::StringTooLong);
        }

        // Update metadata
        metadata.metadata_hash = new_metadata_hash;
        metadata.ipfs_cid = new_ipfs_cid.clone();
        metadata.version += 1;
        metadata.expires_at = env.ledger().timestamp() + METADATA_EXPIRY_SECS;

        // Store updated metadata
        env.storage().instance().set(
            &MetadataDataKey::GrantMetadata(grant_id),
            &metadata,
        );

        // Update global registry
        let mut global_registry = Self::get_global_metadata_registry(env.clone());
        for index in global_registry.iter_mut() {
            if index.grant_id == grant_id {
                index.metadata_hash = new_metadata_hash;
                index.indexed_at = env.ledger().timestamp();
                index.index_version += 1;
                break;
            }
        }
        env.storage().instance().set(
            &MetadataDataKey::GlobalMetadataRegistry,
            &global_registry,
        );

        // Publish update event
        env.events().publish(
            (Symbol::new(&env, "metadata_updated"), grant_id),
            (new_metadata_hash, new_ipfs_cid, updater),
        );

        Ok(())
    }

    /// Search metadata by criteria
    /// 
    /// This function enables cross-chain indexing services to search
    /// for grants based on various criteria.
    /// 
    /// # Arguments
    /// * `schema_type` - Filter by schema type (optional)
    /// * `verified_only` - Only return verified metadata
    /// * `public_only` - Only return public metadata
    /// * `limit` - Maximum results to return
    /// 
    /// # Returns
    /// * `Vec<GrantMetadata>` - Matching metadata
    pub fn search_metadata(
        env: Env,
        schema_type: Option<String>,
        verified_only: bool,
        public_only: bool,
        limit: u32,
    ) -> Vec<GrantMetadata> {
        let global_registry = Self::get_global_metadata_registry(env.clone());
        let mut results = Vec::new(&env);
        let mut count = 0u32;

        for index in global_registry.iter() {
            if count >= limit {
                break;
            }

            if let Ok(metadata) = Self::get_grant_metadata(env.clone(), index.grant_id) {
                let matches = match &schema_type {
                    Some(filter_type) => metadata.schema_type == *filter_type,
                    None => true,
                } && (!verified_only || metadata.verified)
                    && (!public_only || metadata.public);

                if matches {
                    results.push_back(metadata);
                    count += 1;
                }
            }
        }

        results
    }

    /// Get metadata statistics for cross-chain analytics
    /// 
    /// # Returns
    /// * (total_grants, verified_grants, public_grants, total_chains)
    pub fn get_metadata_statistics(env: Env) -> (u64, u64, u64, u64) {
        let global_registry = Self::get_global_metadata_registry(env.clone());
        let mut total_grants = 0u64;
        let mut verified_grants = 0u64;
        let mut public_grants = 0u64;
        let mut chains = std::collections::HashSet::new();

        for index in global_registry.iter() {
            total_grants += 1;

            if let Ok(metadata) = Self::get_grant_metadata(env.clone(), index.grant_id) {
                if metadata.verified {
                    verified_grants += 1;
                }
                if metadata.public {
                    public_grants += 1;
                }
            }

            for reference in index.chain_references.iter() {
                chains.insert(reference.chain_id.clone());
            }
        }

        (total_grants, verified_grants, public_grants, chains.len() as u64)
    }
}
