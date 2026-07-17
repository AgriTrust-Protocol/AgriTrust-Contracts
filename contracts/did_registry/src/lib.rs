#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, BytesN, Env, String, Vec,
};

#[cfg(test)]
mod test;

const MAX_BATCH_KEYS: u32 = 10;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Did(Address),
    Key(Address, String),
    Service(Address, String),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DidRecord {
    pub identity: Address,
    pub did: String,
    pub document_hash: BytesN<32>,
    pub ipfs_cid: String,
    pub also_known_as: Vec<String>,
    pub active: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationKey {
    pub id: String,
    pub key_type: String,
    pub controller: Address,
    pub public_key_multibase: String,
    pub revoked: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceEndpoint {
    pub id: String,
    pub service_type: String,
    pub service_endpoint: String,
    pub active: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyChange {
    pub id: String,
    pub key_type: String,
    pub public_key_multibase: String,
    pub revoke: bool,
}

#[contract]
pub struct DidRegistryContract;

#[contractimpl]
impl DidRegistryContract {
    pub fn register_did(
        env: Env,
        identity: Address,
        document_hash: BytesN<32>,
        ipfs_cid: String,
        also_known_as: Vec<String>,
    ) -> DidRecord {
        identity.require_auth();
        let did = did_for(&env);
        let record = DidRecord {
            identity: identity.clone(),
            did,
            document_hash: document_hash.clone(),
            ipfs_cid: ipfs_cid.clone(),
            also_known_as,
            active: true,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Did(identity.clone()), &record);
        env.events().publish(
            (symbol_short!("did_reg"), identity),
            (document_hash, ipfs_cid),
        );
        record
    }

    pub fn add_key(
        env: Env,
        identity: Address,
        id: String,
        key_type: String,
        public_key_multibase: String,
    ) -> VerificationKey {
        identity.require_auth();
        require_registered(&env, &identity);
        let key = VerificationKey {
            id: id.clone(),
            key_type,
            controller: identity.clone(),
            public_key_multibase,
            revoked: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Key(identity.clone(), id.clone()), &key);
        env.events().publish(
            (symbol_short!("key_add"), identity, id),
            (key.key_type.clone(), key.public_key_multibase.clone()),
        );
        key
    }

    pub fn revoke_key(env: Env, identity: Address, key_id: String) {
        identity.require_auth();
        let storage_key = DataKey::Key(identity.clone(), key_id.clone());
        let mut key: VerificationKey = env
            .storage()
            .persistent()
            .get(&storage_key)
            .expect("key not found");
        key.revoked = true;
        env.storage().persistent().set(&storage_key, &key);
        env.events()
            .publish((symbol_short!("key_rev"), identity, key_id), &());
    }

    pub fn batch_rotate_keys(env: Env, identity: Address, changes: Vec<KeyChange>) {
        identity.require_auth();
        require_registered(&env, &identity);
        if changes.len() > MAX_BATCH_KEYS {
            panic!("too many key changes");
        }
        for change in changes.iter() {
            if change.revoke {
                let storage_key = DataKey::Key(identity.clone(), change.id.clone());
                let mut key: VerificationKey = env
                    .storage()
                    .persistent()
                    .get(&storage_key)
                    .expect("key not found");
                key.revoked = true;
                env.storage().persistent().set(&storage_key, &key);
                env.events()
                    .publish((symbol_short!("key_rev"), identity.clone(), change.id), &());
            } else {
                let key = VerificationKey {
                    id: change.id.clone(),
                    key_type: change.key_type,
                    controller: identity.clone(),
                    public_key_multibase: change.public_key_multibase,
                    revoked: false,
                };
                env.storage()
                    .persistent()
                    .set(&DataKey::Key(identity.clone(), key.id.clone()), &key);
                env.events().publish(
                    (symbol_short!("key_add"), identity.clone(), key.id.clone()),
                    (key.key_type.clone(), key.public_key_multibase.clone()),
                );
            }
        }
    }

    pub fn add_service(
        env: Env,
        identity: Address,
        id: String,
        service_type: String,
        service_endpoint: String,
    ) -> ServiceEndpoint {
        identity.require_auth();
        require_registered(&env, &identity);
        let service = ServiceEndpoint {
            id: id.clone(),
            service_type,
            service_endpoint,
            active: true,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Service(identity.clone(), id.clone()), &service);
        env.events().publish(
            (symbol_short!("svc_add"), identity, id),
            (
                service.service_type.clone(),
                service.service_endpoint.clone(),
            ),
        );
        service
    }

    pub fn update_service(
        env: Env,
        identity: Address,
        service_id: String,
        endpoint: String,
    ) -> ServiceEndpoint {
        identity.require_auth();
        let storage_key = DataKey::Service(identity.clone(), service_id.clone());
        let mut service: ServiceEndpoint = env
            .storage()
            .persistent()
            .get(&storage_key)
            .expect("service not found");
        service.service_endpoint = endpoint;
        env.storage().persistent().set(&storage_key, &service);
        env.events().publish(
            (symbol_short!("svc_upd"), identity, service_id),
            service.service_endpoint.clone(),
        );
        service
    }

    pub fn update_document(
        env: Env,
        identity: Address,
        document_hash: BytesN<32>,
        ipfs_cid: String,
    ) -> DidRecord {
        identity.require_auth();
        let storage_key = DataKey::Did(identity.clone());
        let mut record: DidRecord = env
            .storage()
            .persistent()
            .get(&storage_key)
            .expect("DID not registered");
        record.document_hash = document_hash.clone();
        record.ipfs_cid = ipfs_cid.clone();
        env.storage().persistent().set(&storage_key, &record);
        env.events().publish(
            (symbol_short!("did_doc"), identity),
            (document_hash, ipfs_cid),
        );
        record
    }

    pub fn get_did(env: Env, identity: Address) -> DidRecord {
        require_registered(&env, &identity)
    }
    pub fn get_key(env: Env, identity: Address, key_id: String) -> VerificationKey {
        env.storage()
            .persistent()
            .get(&DataKey::Key(identity, key_id))
            .expect("key not found")
    }
    pub fn get_service(env: Env, identity: Address, service_id: String) -> ServiceEndpoint {
        env.storage()
            .persistent()
            .get(&DataKey::Service(identity, service_id))
            .expect("service not found")
    }
}

fn require_registered(env: &Env, identity: &Address) -> DidRecord {
    env.storage()
        .persistent()
        .get(&DataKey::Did(identity.clone()))
        .expect("DID not registered")
}

fn did_for(env: &Env) -> String {
    String::from_str(env, "did:agritrust:{address}")
}
