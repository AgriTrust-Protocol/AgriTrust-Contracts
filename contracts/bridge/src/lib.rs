#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Bytes, BytesN, Env, IntoVal, Vec,
};

pub const MERKLE_DEPTH: u32 = 16;
pub const MAX_FARMS: u32 = 65_535;
pub const ROOT_UPDATE_LEDGERS: u32 = 1_000;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    IdentityRoot,
    LastRootLedger,
    KnownRoot(BytesN<32>),
    Delegation(BytesN<32>, BytesN<32>, u32),
    RelayNonce(BytesN<32>, u64),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentityProof {
    pub farm_id: BytesN<32>,
    pub chain_id: u32,
    pub public_key: BytesN<32>,
    pub metadata_hash: BytesN<32>,
    pub merkle_proof: Vec<BytesN<32>>,
    pub leaf_index: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Delegation {
    pub farm_id: BytesN<32>,
    pub delegate_address: BytesN<32>,
    pub chain_id: u32,
    pub expiry: u64,
    pub signer: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootUpdate {
    pub root: BytesN<32>,
    pub ledger: u32,
    pub nonce: u64,
}

#[contract]
pub struct BridgeContract;

#[contractimpl]
impl BridgeContract {
    pub fn initialize(env: Env, admin: Address, initial_root: BytesN<32>) {
        if env.storage().persistent().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Admin, &admin);
        write_root(&env, initial_root);
    }

    pub fn update_root(env: Env, new_root: BytesN<32>, proof_of_update: BytesN<32>) -> RootUpdate {
        require_admin(&env).require_auth();
        let expected = root_update_digest(&env, &new_root);
        if proof_of_update != expected {
            panic!("invalid root update proof");
        }
        write_root(&env, new_root.clone());
        let update = RootUpdate {
            root: new_root,
            ledger: env.ledger().sequence(),
            nonce: env.ledger().sequence() as u64,
        };
        env.events()
            .publish((symbol_short!("root_upd"),), update.clone());
        update
    }

    pub fn identity_root(env: Env) -> BytesN<32> {
        env.storage()
            .persistent()
            .get(&DataKey::IdentityRoot)
            .expect("root not set")
    }

    pub fn root_update_digest(env: Env, new_root: BytesN<32>) -> BytesN<32> {
        root_update_digest(&env, &new_root)
    }

    pub fn leaf_hash(
        env: Env,
        farm_id: BytesN<32>,
        chain_id: u32,
        public_key: BytesN<32>,
        metadata_hash: BytesN<32>,
    ) -> BytesN<32> {
        leaf_hash(&env, &farm_id, chain_id, &public_key, &metadata_hash)
    }

    pub fn verify_identity(env: Env, proof: IdentityProof) -> bool {
        let root: BytesN<32> = env
            .storage()
            .persistent()
            .get(&DataKey::IdentityRoot)
            .expect("root not set");
        verify_identity_against_root(&env, &root, &proof)
    }

    pub fn relay_root(env: Env, relayer: Address, source_chain: BytesN<32>, update: RootUpdate) {
        relayer.require_auth();
        let key = DataKey::RelayNonce(source_chain.clone(), update.nonce);
        if env.storage().temporary().has(&key) {
            panic!("relay replay");
        }
        env.storage().temporary().set(&key, &true);
        env.storage()
            .persistent()
            .set(&DataKey::KnownRoot(update.root.clone()), &update.ledger);
        env.events()
            .publish((symbol_short!("relayed"), source_chain), update);
    }

    pub fn delegate(
        env: Env,
        signer: Address,
        farm_id: BytesN<32>,
        delegate_address: BytesN<32>,
        chain_id: u32,
        expiry: u64,
    ) -> Delegation {
        signer.require_auth();
        if expiry <= env.ledger().timestamp() {
            panic!("delegation expired");
        }
        let delegation = Delegation {
            farm_id: farm_id.clone(),
            delegate_address: delegate_address.clone(),
            chain_id,
            expiry,
            signer,
        };
        env.storage().persistent().set(
            &DataKey::Delegation(farm_id, delegate_address, chain_id),
            &delegation,
        );
        env.events()
            .publish((symbol_short!("delegate"), chain_id), delegation.clone());
        delegation
    }

    pub fn is_authorized(
        env: Env,
        farm_id: BytesN<32>,
        delegate_address: BytesN<32>,
        chain_id: u32,
    ) -> bool {
        let key = DataKey::Delegation(farm_id, delegate_address, chain_id);
        let delegation: Option<Delegation> = env.storage().persistent().get(&key);
        match delegation {
            Some(d) => d.expiry > env.ledger().timestamp(),
            None => false,
        }
    }
}

pub fn verify_identity_against_root(env: &Env, root: &BytesN<32>, proof: &IdentityProof) -> bool {
    if proof.leaf_index >= MAX_FARMS || proof.merkle_proof.len() > MERKLE_DEPTH {
        return false;
    }
    let leaf = leaf_hash(
        env,
        &proof.farm_id,
        proof.chain_id,
        &proof.public_key,
        &proof.metadata_hash,
    );
    verify_merkle_proof(env, root, &leaf, &proof.merkle_proof, proof.leaf_index)
}

pub fn verify_merkle_proof(
    env: &Env,
    root: &BytesN<32>,
    leaf: &BytesN<32>,
    proof: &Vec<BytesN<32>>,
    mut index: u32,
) -> bool {
    if proof.len() > MERKLE_DEPTH {
        return false;
    }
    let mut computed = leaf.clone();
    for sibling in proof.iter() {
        computed = if index & 1 == 0 {
            hash_pair(env, &computed, &sibling)
        } else {
            hash_pair(env, &sibling, &computed)
        };
        index >>= 1;
    }
    computed == *root
}

pub fn hash_pair(env: &Env, left: &BytesN<32>, right: &BytesN<32>) -> BytesN<32> {
    let mut input = Bytes::new(env);
    input.append(&left.clone().into_val(env));
    input.append(&right.clone().into_val(env));
    env.crypto().sha256(&input).into()
}

fn append_u32(bytes: &mut Bytes, value: u32) {
    bytes.push_back(((value >> 24) & 0xff) as u8);
    bytes.push_back(((value >> 16) & 0xff) as u8);
    bytes.push_back(((value >> 8) & 0xff) as u8);
    bytes.push_back((value & 0xff) as u8);
}

fn leaf_hash(
    env: &Env,
    farm_id: &BytesN<32>,
    chain_id: u32,
    public_key: &BytesN<32>,
    metadata_hash: &BytesN<32>,
) -> BytesN<32> {
    let mut input = Bytes::new(env);
    input.append(&farm_id.clone().into_val(env));
    append_u32(&mut input, chain_id);
    input.append(&public_key.clone().into_val(env));
    input.append(&metadata_hash.clone().into_val(env));
    env.crypto().sha256(&input).into()
}

fn root_update_digest(env: &Env, new_root: &BytesN<32>) -> BytesN<32> {
    let mut input = Bytes::new(env);
    input.append(&new_root.clone().into_val(env));
    append_u32(&mut input, ROOT_UPDATE_LEDGERS);
    env.crypto().sha256(&input).into()
}

fn write_root(env: &Env, root: BytesN<32>) {
    env.storage()
        .persistent()
        .set(&DataKey::IdentityRoot, &root);
    env.storage()
        .persistent()
        .set(&DataKey::LastRootLedger, &env.ledger().sequence());
}

fn require_admin(env: &Env) -> Address {
    env.storage()
        .persistent()
        .get(&DataKey::Admin)
        .expect("admin not set")
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, BytesN as _, Ledger},
        vec, Env,
    };

    fn setup() -> (Env, BridgeContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(BridgeContract, ());
        let client = BridgeContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let zero = BytesN::from_array(&env, &[0; 32]);
        client.initialize(&admin, &zero);
        (env, client, admin)
    }

    #[test]
    fn verifies_identity_across_domain_root() {
        let (env, client, _admin) = setup();
        let farm_id = BytesN::random(&env);
        let public_key = BytesN::random(&env);
        let metadata_hash = BytesN::random(&env);
        let leaf = client.leaf_hash(&farm_id, &137, &public_key, &metadata_hash);
        let sibling = BytesN::random(&env);
        let root = hash_pair(&env, &leaf, &sibling);
        let digest = client.root_update_digest(&root);
        client.update_root(&root, &digest);
        let proof = IdentityProof {
            farm_id,
            chain_id: 137,
            public_key,
            metadata_hash,
            merkle_proof: vec![&env, sibling],
            leaf_index: 0,
        };
        assert!(client.verify_identity(&proof));
    }

    #[test]
    fn delegate_authorization_expires() {
        let (env, client, _admin) = setup();
        env.ledger().with_mut(|l| l.timestamp = 100);
        let signer = Address::generate(&env);
        let farm_id = BytesN::random(&env);
        let delegate = BytesN::random(&env);
        client.delegate(&signer, &farm_id, &delegate, &42220, &200);
        assert!(client.is_authorized(&farm_id, &delegate, &42220));
        env.ledger().with_mut(|l| l.timestamp = 201);
        assert!(!client.is_authorized(&farm_id, &delegate, &42220));
    }
}
