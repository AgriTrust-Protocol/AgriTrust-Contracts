use soroban_sdk::{Address, Bytes, BytesN, Env, token};
use soroban_sdk::xdr::ToXdr;
use crate::{DataKey, EscrowState, EscrowStatus, Dispute};

pub fn validate_award(
    env: &Env,
    arbitrator_public_key: &BytesN<32>,
    signature: &BytesN<64>,
    arbitration_id: u32,
    proposal_id: u32,
    funder_award: i128,
    grantee_award: i128,
    nonce: u64,
) {
    let mut message = Bytes::new(env);
    message.append(&arbitration_id.to_xdr(env));
    message.append(&proposal_id.to_xdr(env));
    message.append(&funder_award.to_xdr(env));
    message.append(&grantee_award.to_xdr(env));
    message.append(&nonce.to_xdr(env));

    env.crypto().ed25519_verify(arbitrator_public_key, &message, signature);
}

pub fn finalize_settlement(
    env: Env,
    arbitration_id: u32,
    proposal_id: u32,
    expected_sequence: u32,
    arbitrator_public_key: BytesN<32>,
    signature: BytesN<64>,
    funder_award: i128,
    grantee_award: i128,
    nonce: u64,
) {
    // 1. Optimistic lock check (temporary storage)
    let current_sequence = env.ledger().sequence();
    let lock_key = DataKey::SettlementLock(arbitration_id);
    if env.storage().temporary().has(&lock_key) {
        let stored_sequence: u32 = env.storage().temporary().get(&lock_key).unwrap();
        if stored_sequence == current_sequence {
            panic!("ConcurrentSettlementInProgress");
        }
    }
    env.storage().temporary().set(&lock_key, &current_sequence);
    env.storage().temporary().extend_ttl(&lock_key, 0, 10);

    // 2. Read Dispute details (persistent storage)
    let dispute_key = DataKey::Dispute(arbitration_id);
    if !env.storage().persistent().has(&dispute_key) {
        panic!("Dispute does not exist");
    }
    let dispute: Dispute = env.storage().persistent().get(&dispute_key).unwrap();

    // Verify arbitrator public key matches the stored one
    if arbitrator_public_key != dispute.arbitrator_public_key {
        panic!("Arbitrator public key mismatch");
    }

    // 3. Verify and store nonce (prevent replay) - Check this before escrow state to fail on nonce reuse first
    let nonce_key = DataKey::UsedNonce(arbitration_id, nonce);
    if env.storage().persistent().has(&nonce_key) {
        panic!("NonceAlreadyUsed");
    }
    env.storage().persistent().set(&nonce_key, &true);

    // 4. Read EscrowState (persistent storage)
    let escrow_key = DataKey::EscrowState(arbitration_id);
    if !env.storage().persistent().has(&escrow_key) {
        panic!("EscrowState does not exist");
    }
    let mut escrow_state: EscrowState = env.storage().persistent().get(&escrow_key).unwrap();

    // 5. Verify Escrow status and sequence
    if escrow_state.status != EscrowStatus::Locked {
        panic!("Escrow is not locked");
    }
    if escrow_state.sequence != expected_sequence {
        panic!("Invalid expected sequence");
    }

    // 6. Validate arbitrator signature on award
    validate_award(
        &env,
        &dispute.arbitrator_public_key,
        &signature,
        arbitration_id,
        proposal_id,
        funder_award,
        grantee_award,
        nonce,
    );

    // 7. Validate award bounds
    if funder_award < 0 || grantee_award < 0 {
        panic!("Awards cannot be negative");
    }
    if funder_award + grantee_award > dispute.amount {
        panic!("Awards exceed dispute amount");
    }

    // 8. Update EscrowState
    escrow_state.sequence += 1;
    escrow_state.status = EscrowStatus::Settled(proposal_id);
    env.storage().persistent().set(&escrow_key, &escrow_state);

    // 9. Transfer funds
    let token_addr: Address = env.storage().instance().get(&DataKey::Token).unwrap();
    let token_client = token::Client::new(&env, &token_addr);

    if funder_award > 0 {
        token_client.transfer(&env.current_contract_address(), &dispute.funder, &funder_award);
    }
    if grantee_award > 0 {
        token_client.transfer(&env.current_contract_address(), &dispute.grantee, &grantee_award);
    }
}
