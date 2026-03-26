#[cfg(test)]
mod test_cross_chain_features {
    use super::*;
    use soroban_sdk::{Address, Env, String, Symbol, vec, testutils::Address as TestAddress};

    #[test]
    fn test_wasm_hash_verification_initialization() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let grant_id = 1u64;
        let wasm_hash = [1u8; 32];
        let version = String::from_str(&env, "v1.0.0");

        // Test successful initialization
        let result = WasmHashVerification::initialize_grant_wasm_hash(
            env.clone(),
            grant_id,
            wasm_hash,
            version.clone(),
            admin.clone(),
        );
        assert!(result.is_ok());

        // Test duplicate initialization fails
        let result = WasmHashVerification::initialize_grant_wasm_hash(
            env.clone(),
            grant_id,
            wasm_hash,
            version,
            admin,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_wasm_hash_verification() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let grant_id = 1u64;
        let correct_hash = [1u8; 32];
        let wrong_hash = [2u8; 32];
        let version = String::from_str(&env, "v1.0.0");

        // Initialize with correct hash
        WasmHashVerification::initialize_grant_wasm_hash(
            env.clone(),
            grant_id,
            correct_hash,
            version,
            admin,
        ).unwrap();

        // Test verification with correct hash
        let result = WasmHashVerification::verify_grant_wasm_hash(
            env.clone(),
            grant_id,
            correct_hash,
        );
        assert!(result.is_ok());
        assert!(result.unwrap());

        // Test verification with wrong hash
        let result = WasmHashVerification::verify_grant_wasm_hash(
            env.clone(),
            grant_id,
            wrong_hash,
        );
        assert!(result.is_ok());
        assert!(!result.unwrap());

        // Test verification for non-existent grant
        let result = WasmHashVerification::verify_grant_wasm_hash(
            env.clone(),
            999u64,
            correct_hash,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_version_registry() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let wasm_hash = [1u8; 32];
        let version = String::from_str(&env, "v1.0.0");
        let description = String::from_str(&env, "Initial version");
        let security_level = crate::wasm_hash_verification::SecurityLevel::Medium;

        // Test version registration
        let result = WasmHashVerification::register_version(
            env.clone(),
            wasm_hash,
            version.clone(),
            description.clone(),
            security_level,
            admin.clone(),
        );
        assert!(result.is_ok());

        // Test duplicate registration fails
        let result = WasmHashVerification::register_version(
            env.clone(),
            wasm_hash,
            version,
            description,
            security_level,
            admin,
        );
        assert!(result.is_err());

        // Test retrieving version info
        let version_info = WasmHashVerification::get_version_info(env.clone(), wasm_hash);
        assert!(version_info.is_ok());
        assert_eq!(version_info.unwrap().security_level, security_level);
    }

    #[test]
    fn test_upgrade_proposal_and_decision() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let grant_id = 1u64;
        let initial_hash = [1u8; 32];
        let new_hash = [2u8; 32];
        let version = String::from_str(&env, "v1.0.0");
        let reason = String::from_str(&env, "Security update");

        // Initialize grant
        WasmHashVerification::initialize_grant_wasm_hash(
            env.clone(),
            grant_id,
            initial_hash,
            version,
            admin.clone(),
        ).unwrap();

        // Register new version
        WasmHashVerification::register_version(
            env.clone(),
            new_hash,
            String::from_str(&env, "v1.1.0"),
            String::from_str(&env, "Updated version"),
            crate::wasm_hash_verification::SecurityLevel::High,
            admin.clone(),
        ).unwrap();

        // Propose upgrade
        let result = WasmHashVerification::propose_upgrade(
            env.clone(),
            grant_id,
            new_hash,
            reason.clone(),
            true, // security critical
            admin,
        );
        assert!(result.is_ok());

        // Test user accepts upgrade
        let result = WasmHashVerification::decide_upgrade(
            env.clone(),
            grant_id,
            true, // accept
            user.clone(),
        );
        assert!(result.is_ok());

        // Verify hash was updated
        let hash_info = WasmHashVerification::get_grant_wasm_hash(env.clone(), grant_id).unwrap();
        assert_eq!(hash_info.current_hash, new_hash);

        // Test that decision cannot be made again
        let result = WasmHashVerification::decide_upgrade(
            env.clone(),
            grant_id,
            false, // reject
            user,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_cross_chain_metadata_creation() {
        let env = Env::default();
        let grant_id = 1u64;
        let metadata_hash = [1u8; 32];
        let ipfs_cid = String::from_str(&env, "QmTest123");
        let schema_type = String::from_str(&env, "Grant");
        let creator = Address::generate(&env);

        // Test successful metadata creation
        let result = CrossChainMetadata::create_grant_metadata(
            env.clone(),
            grant_id,
            metadata_hash,
            ipfs_cid.clone(),
            schema_type.clone(),
            creator.clone(),
            true, // public
        );
        assert!(result.is_ok());

        // Test duplicate creation fails
        let result = CrossChainMetadata::create_grant_metadata(
            env.clone(),
            grant_id,
            metadata_hash,
            ipfs_cid,
            schema_type,
            creator,
            true,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_cross_chain_references() {
        let env = Env::default();
        let grant_id = 1u64;
        let metadata_hash = [1u8; 32];
        let ipfs_cid = String::from_str(&env, "QmTest123");
        let schema_type = String::from_str(&env, "Grant");
        let creator = Address::generate(&env);
        let chain_id = String::from_str(&env, "ethereum");
        let external_id = String::from_str(&env, "0x1234567890123456789012345678901234567890");

        // Create metadata first
        CrossChainMetadata::create_grant_metadata(
            env.clone(),
            grant_id,
            metadata_hash,
            ipfs_cid,
            schema_type,
            creator.clone(),
            true,
        ).unwrap();

        // Test adding cross-chain reference
        let result = CrossChainMetadata::add_cross_chain_reference(
            env.clone(),
            grant_id,
            chain_id.clone(),
            external_id.clone(),
            crate::cross_chain_metadata::ReferenceType::Contract,
            creator.clone(),
        );
        assert!(result.is_ok());

        // Test duplicate reference fails
        let result = CrossChainMetadata::add_cross_chain_reference(
            env.clone(),
            grant_id,
            chain_id,
            external_id,
            crate::cross_chain_metadata::ReferenceType::Contract,
            creator,
        );
        assert!(result.is_err());

        // Test retrieving references
        let references = CrossChainMetadata::get_cross_chain_references(env.clone(), grant_id).unwrap();
        assert_eq!(references.len(), 1);
        assert_eq!(references.get(0).unwrap().chain_id, "ethereum");

        // Test getting grants by chain
        let grants = CrossChainMetadata::get_grants_by_chain(env.clone(), String::from_str(&env, "ethereum"));
        assert_eq!(grants.len(), 1);
        assert_eq!(grants.get(0).unwrap(), &grant_id);
    }

    #[test]
    fn test_cross_chain_verification() {
        let env = Env::default();
        let grant_id = 1u64;
        let metadata_hash = [1u8; 32];
        let ipfs_cid = String::from_str(&env, "QmTest123");
        let schema_type = String::from_str(&env, "Grant");
        let creator = Address::generate(&env);
        let verifier = Address::generate(&env);
        let chain_id = String::from_str(&env, "ethereum");
        let external_id = String::from_str(&env, "0x1234567890123456789012345678901234567890");

        // Create metadata and reference
        CrossChainMetadata::create_grant_metadata(
            env.clone(),
            grant_id,
            metadata_hash,
            ipfs_cid,
            schema_type,
            creator.clone(),
            true,
        ).unwrap();

        CrossChainMetadata::add_cross_chain_reference(
            env.clone(),
            grant_id,
            chain_id.clone(),
            external_id.clone(),
            crate::cross_chain_metadata::ReferenceType::Contract,
            creator,
        ).unwrap();

        // Test verification
        let result = CrossChainMetadata::verify_cross_chain_reference(
            env.clone(),
            grant_id,
            chain_id,
            external_id,
            verifier.clone(),
            true, // verified
        );
        assert!(result.is_ok());

        // Check that reference is now verified
        let references = CrossChainMetadata::get_cross_chain_references(env.clone(), grant_id).unwrap();
        assert_eq!(references.get(0).unwrap().verified, true);
    }

    #[test]
    fn test_metadata_search_and_statistics() {
        let env = Env::default();
        let creator = Address::generate(&env);

        // Create multiple grants with different properties
        for i in 1..=3 {
            let grant_id = i;
            let metadata_hash = [i as u8; 32];
            let ipfs_cid = String::from_str(&env, &format!("QmTest{}", i));
            let schema_type = if i == 1 { 
                String::from_str(&env, "Grant") 
            } else { 
                String::from_str(&env, "Project") 
            };
            let public = i != 2; // Grant 2 is private

            CrossChainMetadata::create_grant_metadata(
                env.clone(),
                grant_id,
                metadata_hash,
                ipfs_cid,
                schema_type,
                creator.clone(),
                public,
            ).unwrap();
        }

        // Test search functionality
        let all_metadata = CrossChainMetadata::search_metadata(
            env.clone(),
            None, // no schema filter
            false, // include unverified
            false, // include private
            10, // limit
        );
        assert_eq!(all_metadata.len(), 2); // Only public grants

        let grant_metadata = CrossChainMetadata::search_metadata(
            env.clone(),
            Some(String::from_str(&env, "Grant")), // filter by Grant schema
            false,
            false,
            10,
        );
        assert_eq!(grant_metadata.len(), 1);

        // Test statistics
        let (total, verified, public_grants, chains) = CrossChainMetadata::get_metadata_statistics(env.clone());
        assert_eq!(total, 3);
        assert_eq!(verified, 0); // None verified yet
        assert_eq!(public_grants, 2);
        assert_eq!(chains, 0); // No cross-chain references yet
    }

    #[test]
    fn test_metadata_validation() {
        let env = Env::default();
        let validator = Address::generate(&env);
        let metadata_hash = [1u8; 32];
        let errors = vec![&env];

        // Test validation with high score
        let result = CrossChainMetadata::validate_metadata(
            env.clone(),
            metadata_hash,
            validator.clone(),
            85, // high score
            errors.clone(),
        );
        assert!(result.is_ok());

        // Check cached validation
        let validation = CrossChainMetadata::get_metadata_validation(env.clone(), metadata_hash);
        assert!(validation.is_some());
        assert!(validation.unwrap().valid);

        // Test validation with low score
        let result = CrossChainMetadata::validate_metadata(
            env.clone(),
            [2u8; 32],
            validator,
            65, // low score
            errors,
        );
        assert!(result.is_ok());

        let validation = CrossChainMetadata::get_metadata_validation(env.clone(), [2u8; 32]);
        assert!(validation.is_some());
        assert!(!validation.unwrap().valid);
    }

    #[test]
    fn test_metadata_update() {
        let env = Env::default();
        let creator = Address::generate(&env);
        let grant_id = 1u64;
        let metadata_hash = [1u8; 32];
        let ipfs_cid = String::from_str(&env, "QmTest123");
        let schema_type = String::from_str(&env, "Grant");

        // Create metadata
        CrossChainMetadata::create_grant_metadata(
            env.clone(),
            grant_id,
            metadata_hash,
            ipfs_cid.clone(),
            schema_type.clone(),
            creator.clone(),
            true,
        ).unwrap();

        // Test successful update by creator
        let new_hash = [2u8; 32];
        let new_cid = String::from_str(&env, "QmUpdated456");
        let result = CrossChainMetadata::update_metadata(
            env.clone(),
            grant_id,
            new_hash,
            new_cid.clone(),
            creator.clone(),
        );
        assert!(result.is_ok());

        // Verify update
        let updated_metadata = CrossChainMetadata::get_grant_metadata(env.clone(), grant_id).unwrap();
        assert_eq!(updated_metadata.metadata_hash, new_hash);
        assert_eq!(updated_metadata.ipfs_cid, new_cid);
        assert_eq!(updated_metadata.version, 2);

        // Test update by non-creator fails
        let impostor = Address::generate(&env);
        let result = CrossChainMetadata::update_metadata(
            env.clone(),
            grant_id,
            [3u8; 32],
            String::from_str(&env, "QmFake789"),
            impostor,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_integration_with_main_contract() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let recipient = Address::generate(&env);
        let token = Address::generate(&env);

        // Initialize contract
        GrantContract::initialize(
            env.clone(),
            admin.clone(),
            token.clone(),
            Address::generate(&env), // treasury
            Address::generate(&env), // oracle
            token.clone(), // native token
        ).unwrap();

        // Create a grant (this should automatically create WASM hash and metadata)
        let grant_id = 1u64;
        let config = GranteeConfig {
            recipient: recipient.clone(),
            asset: token,
            total_amount: 1000,
            flow_rate: 100,
            warmup_duration: 0,
            priority_level: 1,
            security_deposit_percentage: 500,
            validator: None,
            linked_addresses: Vec::new(&env),
            milestone_amount: 0,
            total_milestones: 0,
        };

        GrantContract::batch_init(
            env.clone(),
            vec![&env, config],
            grant_id,
        ).unwrap();

        // Test that WASM hash verification exists
        let wasm_result = WasmHashVerification::get_grant_wasm_hash(env.clone(), grant_id);
        assert!(wasm_result.is_ok());

        // Test that cross-chain metadata exists
        let metadata_result = CrossChainMetadata::get_grant_metadata(env.clone(), grant_id);
        assert!(metadata_result.is_ok());

        // Test adding cross-chain reference through main contract
        let result = GrantContract::add_cross_chain_reference(
            env.clone(),
            grant_id,
            String::from_str(&env, "ethereum"),
            String::from_str(&env, "0x1234567890123456789012345678901234567890"),
            0, // Contract type
        );
        assert!(result.is_ok());

        // Test getting cross-chain statistics
        let (total, verified, public_grants, chains) = GrantContract::get_cross_chain_statistics(env.clone());
        assert_eq!(total, 1);
        assert_eq!(public_grants, 1);
        assert_eq!(chains, 1); // One chain reference added
    }
}
