#![no_std]
#![allow(clippy::all)]

#[cfg(test)]
mod test;
mod types;

use soroban_sdk::{contract, contractimpl, symbol_short, Address, BytesN, Env, String, Symbol};

pub use types::*;

/// Unit type marking the AgriTrust execution engine inside the WASM module.
///
/// Soroban derives the contract specification, the exported ABI and the
/// `ContractClient` used by integrators and by the unit tests from this type.
#[contract]
pub struct Contract;

fn load_twin(env: &Env, asset_id: &BytesN<32>) -> Result<AssetTwinRecord, ContractError> {
    env.storage()
        .persistent()
        .get::<_, AssetTwinRecord>(&DataKey::Asset(asset_id.clone()))
        .ok_or(ContractError::AssetNotFound)
}

#[contractimpl]
impl Contract {
    /// Establish the protocol administrator.
    ///
    /// The sovereign root of trust may only be set once per deployment, so a
    /// repeated call is rejected rather than silently re-pointing governance.
    pub fn initialize(env: Env, admin: Address) -> Result<(), ContractError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(ContractError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    /// Read the protocol administrator, if the engine has been initialized.
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Admin)
    }

    /// Anchor the digital twin of a physical commodity batch.
    ///
    /// `merkle_root` commits to the off-ledger `:Process` provenance graph, so a
    /// batch's entire lifecycle becomes verifiable against this single on-ledger
    /// commitment. Re-anchoring an existing twin is permitted only to the
    /// recorded originator, which prevents hijacking of a batch already held in
    /// the registry.
    pub fn register_asset_twin(
        env: Env,
        originator: Address,
        asset_id: BytesN<32>,
        did_uri: String,
        commodity: Symbol,
        merkle_root: BytesN<32>,
    ) -> Result<(), ContractError> {
        originator.require_auth();

        if let Some(existing) = env
            .storage()
            .persistent()
            .get::<_, AssetTwinRecord>(&DataKey::Asset(asset_id.clone()))
        {
            if existing.originator != originator {
                return Err(ContractError::Unauthorized);
            }
        }

        let record = AssetTwinRecord {
            asset_id: asset_id.clone(),
            originator: originator.clone(),
            originator_did: did_uri,
            commodity_type: commodity.clone(),
            provenance_merkle_root: merkle_root,
            registered_at: env.ledger().timestamp(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::Asset(asset_id.clone()), &record);

        env.events().publish(
            (symbol_short!("twin_reg"), originator),
            (asset_id, commodity, env.ledger().timestamp()),
        );

        Ok(())
    }

    /// Commit an ODRL usage policy to the ledger so that a `:DataContract` may
    /// reference it.
    ///
    /// Without an anchored digest the usage terms are unverifiable, so the
    /// engine refuses to grant access against them.
    pub fn register_odrl_policy(
        env: Env,
        issuer: Address,
        policy_hash: BytesN<32>,
        policy_uri: String,
    ) -> Result<(), ContractError> {
        issuer.require_auth();

        let record = ODRLPolicyRecord {
            policy_hash: policy_hash.clone(),
            policy_uri,
            issuer: issuer.clone(),
            registered_at: env.ledger().timestamp(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::Policy(policy_hash.clone()), &record);

        env.events().publish(
            (symbol_short!("pol_reg"), issuer),
            (policy_hash, env.ledger().timestamp()),
        );

        Ok(())
    }

    /// Establish a machine-enforceable data usage agreement.
    ///
    /// Only the sovereign originator of `asset_id` may license its compliance
    /// records, and only against an anchored ODRL policy digest. The resulting
    /// grant carries an escrow payment obligation (`access_fee`) and an
    /// automatically expiring validity window of `duration_ledgers`.
    pub fn create_data_contract(
        env: Env,
        farmer: Address,
        consumer: Address,
        contract_id: BytesN<32>,
        asset_id: BytesN<32>,
        policy_hash: BytesN<32>,
        fee: i128,
        duration_ledgers: u32,
    ) -> Result<(), ContractError> {
        farmer.require_auth();

        // Farmer Data Sovereignty: the licensor must be the batch originator.
        let twin = load_twin(&env, &asset_id)?;
        if twin.originator != farmer {
            return Err(ContractError::Unauthorized);
        }

        // Machine-readable terms must resolve to an anchored ODRL policy.
        if !env
            .storage()
            .persistent()
            .has(&DataKey::Policy(policy_hash.clone()))
        {
            return Err(ContractError::PolicyViolation);
        }

        // Re-issuing an existing agreement id is only allowed to its licensor,
        // so one farmer cannot overwrite or hijack another's agreement id.
        if let Some(existing) = env.storage().persistent().get::<_, DataContractRecord>(
            &DataKey::DataContract(contract_id.clone()),
        ) {
            let incumbent = load_twin(&env, &existing.asset_id)?;
            if incumbent.originator != farmer {
                return Err(ContractError::Unauthorized);
            }
        }

        let record = DataContractRecord {
            contract_id: contract_id.clone(),
            asset_id,
            data_consumer: consumer.clone(),
            odrl_policy_hash: policy_hash.clone(),
            access_fee: fee,
            valid_until_ledger: env.ledger().sequence().saturating_add(duration_ledgers),
            is_revoked: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::DataContract(contract_id.clone()), &record);

        env.events().publish(
            (symbol_short!("contract"), farmer),
            (contract_id, consumer, policy_hash),
        );

        Ok(())
    }

    /// Unilaterally revoke a third party's access to compliance records.
    ///
    /// Enforcement of Farmer Data Sovereignty: the originator of the underlying
    /// asset can terminate the grant at any time without the consumer's consent
    /// and without administrator involvement, e.g. when ODRL terms are breached.
    /// Revocation is terminal and irreversible.
    pub fn revoke_data_access(
        env: Env,
        farmer: Address,
        contract_id: BytesN<32>,
    ) -> Result<(), ContractError> {
        farmer.require_auth();

        let mut record = env
            .storage()
            .persistent()
            .get::<_, DataContractRecord>(&DataKey::DataContract(contract_id.clone()))
            .ok_or(ContractError::AssetNotFound)?;

        let twin = load_twin(&env, &record.asset_id)?;
        if twin.originator != farmer {
            return Err(ContractError::Unauthorized);
        }

        if record.is_revoked {
            return Err(ContractError::AccessRevoked);
        }

        record.is_revoked = true;
        env.storage()
            .persistent()
            .set(&DataKey::DataContract(contract_id.clone()), &record);

        env.events()
            .publish((symbol_short!("revoked"), farmer), contract_id);

        Ok(())
    }

    /// Enforcement entrypoint: a consumer proves it may read the compliance
    /// records of an asset twin.
    ///
    /// On success the engine returns the batch's provenance Merkle root, the
    /// cryptographic commitment to the `:Process` graph the consumer is licensed
    /// to verify against. Every failure mode is an explicit, machine-readable
    /// ODRL enforcement outcome.
    pub fn verify_data_access(
        env: Env,
        consumer: Address,
        contract_id: BytesN<32>,
    ) -> Result<BytesN<32>, ContractError> {
        consumer.require_auth();

        let record = env
            .storage()
            .persistent()
            .get::<_, DataContractRecord>(&DataKey::DataContract(contract_id))
            .ok_or(ContractError::AssetNotFound)?;

        if record.data_consumer != consumer {
            return Err(ContractError::Unauthorized);
        }

        if record.is_revoked {
            return Err(ContractError::AccessRevoked);
        }

        if env.ledger().sequence() > record.valid_until_ledger {
            return Err(ContractError::ContractExpired);
        }

        let twin = load_twin(&env, &record.asset_id)?;
        if !env
            .storage()
            .persistent()
            .has(&DataKey::Policy(record.odrl_policy_hash))
        {
            return Err(ContractError::PolicyViolation);
        }

        Ok(twin.provenance_merkle_root)
    }

    /// Resolve an anchored asset twin (`:Asset`).
    pub fn get_asset_twin(env: Env, asset_id: BytesN<32>) -> Option<AssetTwinRecord> {
        env.storage().persistent().get(&DataKey::Asset(asset_id))
    }

    /// Resolve a data usage agreement (`:DataContract`).
    pub fn get_data_contract(env: Env, contract_id: BytesN<32>) -> Option<DataContractRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::DataContract(contract_id))
    }

    /// Resolve an anchored ODRL policy (`:ODRLPolicy`).
    pub fn get_odrl_policy(env: Env, policy_hash: BytesN<32>) -> Option<ODRLPolicyRecord> {
        env.storage().persistent().get(&DataKey::Policy(policy_hash))
    }
}
