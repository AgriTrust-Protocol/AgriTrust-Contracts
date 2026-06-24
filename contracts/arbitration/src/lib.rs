#![no_std]
use treasury::SpeedBumpContractClient;

// ---------------------------------------------------------------------
// Data keys for the Arbitration contract
// ---------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Token,
    Treasury,
    DisputeCounter,
    Dispute(u32),
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
}

#[contract]
pub struct ArbitrationContract;

#[contractimpl]
impl ArbitrationContract {
    pub fn init(env: Env, admin: Address, token: Address, treasury: Address) {
        admin.require_auth();
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        env.storage().instance().set(&DataKey::DisputeCounter, &0u32);
    }

    pub fn raise_dispute(env: Env, grant_id: u32, funder: Address, grantee: Address, amount: i128, arbitrator: Address) -> u32 {
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
        };

        env.storage().persistent().set(&DataKey::Dispute(counter), &dispute);
        counter
    }

    pub fn resolve_dispute(env: Env, dispute_id: u32, funder_award: i128, grantee_award: i128) {
        let mut dispute: Dispute = env.storage().persistent().get(&DataKey::Dispute(dispute_id)).unwrap();
        dispute.arbitrator.require_auth();
        
        if dispute.status == DisputeStatus::Resolved { panic!("Already resolved"); }
        if funder_award + grantee_award > dispute.amount { panic!("Awards exceed amount"); }

        dispute.status = DisputeStatus::Resolved;

        // Use Treasury speed‑bump for payouts
        let treasury_addr: Address = env.storage().instance().get(&DataKey::Treasury).expect("Treasury not set");
        let treasury_client = crate::contracts::treasury::SpeedBumpContractClient::new(&env, &treasury_addr);

        if funder_award > 0 {
            // Transfer from treasury to funder via speed‑bump
            treasury_client.release_with_speedbump(&env.current_contract_address(), &dispute.funder, &(funder_award as u64));
        }
        if grantee_award > 0 {
            treasury_client.release_with_speedbump(&env.current_contract_address(), &dispute.grantee, &(grantee_award as u64));
        }

        env.storage().persistent().set(&DataKey::Dispute(dispute_id), &dispute);
    }
}
