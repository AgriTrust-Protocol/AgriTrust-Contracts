use crate::{DidRegistryContract, DidRegistryContractClient, KeyChange};
use soroban_sdk::{
    testutils::{Address as _, BytesN as _},
    Address, BytesN, Env, String, Vec,
};

fn s(env: &Env, value: &str) -> String {
    String::from_str(env, value)
}

#[test]
fn register_add_rotate_revoke_leaves_two_active_keys() {
    let env = Env::default();
    env.mock_all_auths();

    let identity = Address::generate(&env);
    let contract_id = env.register_contract(None, DidRegistryContract);
    let client = DidRegistryContractClient::new(&env, &contract_id);

    let aliases = Vec::from_array(&env, [s(&env, "acct:co-op-42@agritrust")]);
    let hash = BytesN::<32>::random(&env);
    let record = client.register_did(&identity, &hash, &s(&env, "ipfs://bafy-initial"), &aliases);
    assert!(record.active);
    assert_eq!(record.document_hash, hash);

    client.add_key(
        &identity,
        &s(&env, "key-1"),
        &s(&env, "Ed25519VerificationKey2020"),
        &s(&env, "z6Mk1"),
    );
    client.add_key(
        &identity,
        &s(&env, "key-2"),
        &s(&env, "Ed25519VerificationKey2020"),
        &s(&env, "z6Mk2"),
    );
    client.add_key(
        &identity,
        &s(&env, "key-3"),
        &s(&env, "Ed25519VerificationKey2020"),
        &s(&env, "z6Mk3"),
    );

    let changes = Vec::from_array(
        &env,
        [
            KeyChange {
                id: s(&env, "key-1"),
                key_type: s(&env, "Ed25519VerificationKey2020"),
                public_key_multibase: s(&env, "z6Mk1-rotated"),
                revoke: false,
            },
            KeyChange {
                id: s(&env, "key-2"),
                key_type: s(&env, ""),
                public_key_multibase: s(&env, ""),
                revoke: true,
            },
        ],
    );
    client.batch_rotate_keys(&identity, &changes);

    let key_1 = client.get_key(&identity, &s(&env, "key-1"));
    let key_2 = client.get_key(&identity, &s(&env, "key-2"));
    let key_3 = client.get_key(&identity, &s(&env, "key-3"));

    let mut active_keys = 0;
    if !key_1.revoked {
        active_keys += 1;
    }
    if !key_2.revoked {
        active_keys += 1;
    }
    if !key_3.revoked {
        active_keys += 1;
    }

    assert_eq!(key_1.public_key_multibase, s(&env, "z6Mk1-rotated"));
    assert!(key_2.revoked);
    assert_eq!(active_keys, 2);
}

#[test]
fn services_and_document_hash_are_updated() {
    let env = Env::default();
    env.mock_all_auths();

    let identity = Address::generate(&env);
    let contract_id = env.register_contract(None, DidRegistryContract);
    let client = DidRegistryContractClient::new(&env, &contract_id);
    let aliases = Vec::new(&env);

    client.register_did(
        &identity,
        &BytesN::<32>::random(&env),
        &s(&env, "ipfs://bafy-initial"),
        &aliases,
    );
    client.add_service(
        &identity,
        &s(&env, "svc-1"),
        &s(&env, "DIDCommMessaging"),
        &s(&env, "https://farm.example/didcomm"),
    );
    let updated = client.update_service(
        &identity,
        &s(&env, "svc-1"),
        &s(&env, "https://farm.example/v2/didcomm"),
    );
    assert_eq!(
        updated.service_endpoint,
        s(&env, "https://farm.example/v2/didcomm")
    );

    let new_hash = BytesN::<32>::random(&env);
    let record = client.update_document(&identity, &new_hash, &s(&env, "ipfs://bafy-updated"));
    assert_eq!(record.document_hash, new_hash);
    assert_eq!(record.ipfs_cid, s(&env, "ipfs://bafy-updated"));
}
