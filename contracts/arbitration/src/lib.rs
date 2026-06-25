#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env};

pub mod settlement;
#[cfg(test)]
mod test;


#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscrowStatus {
    Locked,
    Settled(u32), // proposal_id
    Released,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowState {
    pub sequence: u32,
    pub status: EscrowStatus,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Token,
    DisputeCounter,
    Dispute(u32),
    EscrowState(u32),
    SettlementLock(u32),
    UsedNonce(u32, u64),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    Pending,
    InArbitration,
    Resolved,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dispute {
    pub grant_id: u32,
    pub funder: Address,
    pub grantee: Address,
    pub amount: i128,
    pub status: DisputeStatus,
    pub arbitrator: Address,
    pub arbitrator_public_key: soroban_sdk::BytesN<32>,
}

#[contract]
pub struct ArbitrationContract;

#[contractimpl]
impl ArbitrationContract {
    pub fn init(env: Env, admin: Address, token: Address) {
        admin.require_auth();
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage().instance().set(&DataKey::DisputeCounter, &0u32);
    }

    pub fn raise_dispute(
        env: Env,
        grant_id: u32,
        funder: Address,
        grantee: Address,
        amount: i128,
        arbitrator: Address,
        arbitrator_public_key: soroban_sdk::BytesN<32>,
    ) -> u32 {
        funder.require_auth();
        
        let token_addr: Address = env.storage().instance().get(&DataKey::Token).unwrap();
        let token_client = token::Client::new(&env, &token_addr);
        token_client.transfer(&funder, &env.current_contract_address(), &amount);

        let mut counter: u32 = env.storage().instance().get(&DataKey::DisputeCounter).unwrap();
        counter += 1;
        env.storage().instance().set(&DataKey::DisputeCounter, &counter);

        let dispute = Dispute {
            grant_id,
            funder,
            grantee,
            amount,
            status: DisputeStatus::Pending,
            arbitrator,
            arbitrator_public_key,
        };

        env.storage().persistent().set(&DataKey::Dispute(counter), &dispute);

        let escrow_state = EscrowState {
            sequence: 0,
            status: EscrowStatus::Locked,
        };
        env.storage().persistent().set(&DataKey::EscrowState(counter), &escrow_state);

        counter
    }

    pub fn resolve_dispute(env: Env, dispute_id: u32, funder_award: i128, grantee_award: i128) {
        let mut dispute: Dispute = env.storage().persistent().get(&DataKey::Dispute(dispute_id)).unwrap();
        dispute.arbitrator.require_auth();
        
        if dispute.status == DisputeStatus::Resolved { panic!("Already resolved"); }
        if funder_award + grantee_award > dispute.amount { panic!("Awards exceed amount"); }

        dispute.status = DisputeStatus::Resolved;

        let token_addr: Address = env.storage().instance().get(&DataKey::Token).unwrap();
        let token_client = token::Client::new(&env, &token_addr);

        if funder_award > 0 {
            token_client.transfer(&env.current_contract_address(), &dispute.funder, &funder_award);
        }
        if grantee_award > 0 {
            token_client.transfer(&env.current_contract_address(), &dispute.grantee, &grantee_award);
        }

        env.storage().persistent().set(&DataKey::Dispute(dispute_id), &dispute);
    }

    pub fn finalize_settlement(
        env: Env,
        arbitration_id: u32,
        proposal_id: u32,
        expected_sequence: u32,
        arbitrator_public_key: soroban_sdk::BytesN<32>,
        signature: soroban_sdk::BytesN<64>,
        funder_award: i128,
        grantee_award: i128,
        nonce: u64,
    ) {
        settlement::finalize_settlement(
            env,
            arbitration_id,
            proposal_id,
            expected_sequence,
            arbitrator_public_key,
            signature,
            funder_award,
            grantee_award,
            nonce,
        );
    }

    pub fn get_escrow_state(env: Env, arbitration_id: u32) -> EscrowState {
        env.storage().persistent().get(&DataKey::EscrowState(arbitration_id)).unwrap()
    }
}

