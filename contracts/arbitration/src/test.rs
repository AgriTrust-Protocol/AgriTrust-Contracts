#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address, Env, Bytes, BytesN};
use soroban_sdk::token::Client as TokenClient;
use ed25519_dalek::{Signer, SigningKey};

fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn bytesn32(env: &Env, bytes: [u8; 32]) -> BytesN<32> {
    BytesN::from_array(env, &bytes)
}

fn sign_award(
    env: &Env,
    arbitration_id: u32,
    proposal_id: u32,
    funder_award: i128,
    grantee_award: i128,
    nonce: u64,
    key: &SigningKey,
) -> BytesN<64> {
    use soroban_sdk::xdr::ToXdr;
    let mut message = Bytes::new(env);
    message.append(&arbitration_id.to_xdr(env));
    message.append(&proposal_id.to_xdr(env));
    message.append(&funder_award.to_xdr(env));
    message.append(&grantee_award.to_xdr(env));
    message.append(&nonce.to_xdr(env));

    let signature = key.sign(&message.to_alloc_vec()).to_bytes();
    BytesN::from_array(env, &signature)
}

#[test]
fn test_successful_settlement() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let funder = Address::generate(&env);
    let grantee = Address::generate(&env);
    
    // Generate key pair for the arbitrator
    let arbitrator_key = signing_key(1);
    let arbitrator_pub_key = bytesn32(&env, arbitrator_key.verifying_key().to_bytes());
    let arbitrator = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract(token_admin);
    let token_client = token::StellarAssetClient::new(&env, &token_addr);
    token_client.mint(&funder, &1000);

    let contract_id = env.register_contract(None, ArbitrationContract);
    let client = ArbitrationContractClient::new(&env, &contract_id);

    client.init(&admin, &token_addr);
    let dispute_id = client.raise_dispute(&1, &funder, &grantee, &1000, &arbitrator, &arbitrator_pub_key);

    // Check initial escrow state
    let state = client.get_escrow_state(&dispute_id);
    assert_eq!(state.sequence, 0);
    assert_eq!(state.status, EscrowStatus::Locked);

    // Sign and finalize settlement
    let proposal_id = 42;
    let funder_award = 600;
    let grantee_award = 400;
    let nonce = 12345;
    let signature = sign_award(&env, dispute_id, proposal_id, funder_award, grantee_award, nonce, &arbitrator_key);

    client.finalize_settlement(
        &dispute_id,
        &proposal_id,
        &0,
        &arbitrator_pub_key,
        &signature,
        &funder_award,
        &grantee_award,
        &nonce,
    );

    // Verify token distribution
    let real_token = TokenClient::new(&env, &token_addr);
    assert_eq!(real_token.balance(&funder), 600);
    assert_eq!(real_token.balance(&grantee), 400);

    // Check final escrow state
    let state = client.get_escrow_state(&dispute_id);
    assert_eq!(state.sequence, 1);
    assert_eq!(state.status, EscrowStatus::Settled(proposal_id));
}

#[test]
#[should_panic(expected = "ConcurrentSettlementInProgress")]
fn test_concurrent_settlement_in_same_ledger_batch_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let funder = Address::generate(&env);
    let grantee = Address::generate(&env);
    
    let arbitrator_key = signing_key(1);
    let arbitrator_pub_key = bytesn32(&env, arbitrator_key.verifying_key().to_bytes());
    let arbitrator = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract(token_admin);
    let token_client = token::StellarAssetClient::new(&env, &token_addr);
    token_client.mint(&funder, &1000);

    let contract_id = env.register_contract(None, ArbitrationContract);
    let client = ArbitrationContractClient::new(&env, &contract_id);

    client.init(&admin, &token_addr);
    let dispute_id = client.raise_dispute(&1, &funder, &grantee, &1000, &arbitrator, &arbitrator_pub_key);

    // Proposal A
    let proposal_a = 1;
    let sig_a = sign_award(&env, dispute_id, proposal_a, 500, 500, 100, &arbitrator_key);

    // Proposal B (different parameters, same ledger sequence/batch)
    let proposal_b = 2;
    let sig_b = sign_award(&env, dispute_id, proposal_b, 600, 400, 200, &arbitrator_key);

    // We call finalize_settlement for proposal A
    client.finalize_settlement(
        &dispute_id,
        &proposal_a,
        &0,
        &arbitrator_pub_key,
        &sig_a,
        &500,
        &500,
        &100,
    );

    // We call finalize_settlement for proposal B in the same ledger batch (same sequence)
    // This must trigger the SettlementLock and panic with ConcurrentSettlementInProgress
    client.finalize_settlement(
        &dispute_id,
        &proposal_b,
        &0,
        &arbitrator_pub_key,
        &sig_b,
        &600,
        &400,
        &200,
    );
}

#[test]
#[should_panic(expected = "NonceAlreadyUsed")]
fn test_replay_protection_using_nonce() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let funder = Address::generate(&env);
    let grantee = Address::generate(&env);
    
    let arbitrator_key = signing_key(1);
    let arbitrator_pub_key = bytesn32(&env, arbitrator_key.verifying_key().to_bytes());
    let arbitrator = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract(token_admin);
    let token_client = token::StellarAssetClient::new(&env, &token_addr);
    token_client.mint(&funder, &2000);

    let contract_id = env.register_contract(None, ArbitrationContract);
    let client = ArbitrationContractClient::new(&env, &contract_id);

    client.init(&admin, &token_addr);
    let dispute_id = client.raise_dispute(&1, &funder, &grantee, &1000, &arbitrator, &arbitrator_pub_key);

    let nonce = 999;
    let sig_1 = sign_award(&env, dispute_id, 1, 500, 500, nonce, &arbitrator_key);

    // Finalize first dispute settlement
    client.finalize_settlement(
        &dispute_id,
        &1,
        &0,
        &arbitrator_pub_key,
        &sig_1,
        &500,
        &500,
        &nonce,
    );

    // Let the ledger sequence progress so the optimistic lock expires
    env.ledger().with_mut(|li| {
        li.sequence_number += 15;
    });

    // Try to reuse the same nonce/signature for the same dispute
    client.finalize_settlement(
        &dispute_id,
        &1,
        &0,
        &arbitrator_pub_key,
        &sig_1,
        &500,
        &500,
        &nonce,
    );
}

#[test]
#[should_panic(expected = "Invalid expected sequence")]
fn test_invalid_expected_sequence() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let funder = Address::generate(&env);
    let grantee = Address::generate(&env);
    
    let arbitrator_key = signing_key(1);
    let arbitrator_pub_key = bytesn32(&env, arbitrator_key.verifying_key().to_bytes());
    let arbitrator = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract(token_admin);
    let token_client = token::StellarAssetClient::new(&env, &token_addr);
    token_client.mint(&funder, &1000);

    let contract_id = env.register_contract(None, ArbitrationContract);
    let client = ArbitrationContractClient::new(&env, &contract_id);

    client.init(&admin, &token_addr);
    let dispute_id = client.raise_dispute(&1, &funder, &grantee, &1000, &arbitrator, &arbitrator_pub_key);

    let sig = sign_award(&env, dispute_id, 1, 500, 500, 100, &arbitrator_key);
    
    // Provide 1 as expected_sequence instead of 0
    client.finalize_settlement(
        &dispute_id,
        &1,
        &1,
        &arbitrator_pub_key,
        &sig,
        &500,
        &500,
        &100,
    );
}
