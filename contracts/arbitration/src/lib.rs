#![no_std]
#[cfg(test)]
extern crate std;
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env};

#[cfg(test)]
mod test;

pub mod settlement;

pub const TTL_EXTENSION_PERIOD: u32 = 518_400; // 30 days in ledgers (~5s per ledger)
pub const MAX_SETTLEMENT_WINDOW: u64 = 30 * 24 * 60 * 60; // 30 days in seconds

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Token,
    DisputeCounter,
    Dispute(u32),
    EscrowLock(u32),
    EscrowRelease(u32),
    EscrowTtlDeadline(u32),
    EscrowState(u32),
    EscrowCycleCounter,
    ExpiredEscrows,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowLockData {
    pub buyer: Address,
    pub seller: Address,
    pub arbitration_id: u32,
    pub amount: i128,
    pub locked_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowReleaseData {
    pub buyer: Address,
    pub seller: Address,
    pub arbitration_id: u32,
    pub amount: i128,
    pub released_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtlDeadline {
    pub ledger_sequence: u32,
    pub ttl_extension_period: u32,
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

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscrowStatus {
    Locked,
    Released,
    Expired,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowState {
    pub sequence: u32,
    pub status: EscrowStatus,
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
        // Extend instance TTL so the contract survives the max settlement window
        env.storage().instance().extend_ttl(0, 518_400);
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

    // ── Escrow Settlement Functions ──────────────────────────────────────────

    pub fn lock_settlement(
        env: Env,
        cycle: u32,
        buyer: Address,
        seller: Address,
        arbitration_id: u32,
        amount: i128,
    ) {
        settlement::lock_settlement(&env, cycle, &buyer, &seller, arbitration_id, amount);
    }

    pub fn release_settlement(
        env: Env,
        cycle: u32,
        buyer: Address,
        seller: Address,
        arbitration_id: u32,
        amount: i128,
    ) {
        settlement::release_settlement(&env, cycle, &buyer, &seller, arbitration_id, amount);
    }

    pub fn synchronize_escrow_ttl(env: Env, cycle: u32) {
        settlement::synchronize_escrow_ttl(&env, cycle);
    }

    pub fn garbage_collect_expired_escrows(env: Env, max_cycles: u32) -> u32 {
        settlement::garbage_collect_expired_escrows(&env, max_cycles)
    }

    /// Settle a dispute with a pre-flight fee-budget guard. Aborts cleanly
    /// (without moving tokens) if the submitted `max_fee` is too low to cover
    /// the estimated settlement cost, preventing mid-execution `InsufficientFee`.
    pub fn settle_dispute(
        env: Env,
        cycle: u32,
        buyer: Address,
        seller: Address,
        arbitration_id: u32,
        amount: i128,
        available_fee_stroops: i128,
    ) -> Result<(), settlement::SettlementBudgetError> {
        settlement::settle_dispute(
            &env,
            cycle,
            &buyer,
            &seller,
            arbitration_id,
            amount,
            available_fee_stroops,
        )
    }

    pub fn get_escrow_lock(env: Env, cycle: u32) -> Option<EscrowLockData> {
        env.storage().persistent().get(&DataKey::EscrowLock(cycle))
    }

    pub fn get_escrow_release(env: Env, cycle: u32) -> Option<EscrowReleaseData> {
        env.storage().persistent().get(&DataKey::EscrowRelease(cycle))
    }

    pub fn get_escrow_ttl_deadline(env: Env, cycle: u32) -> Option<TtlDeadline> {
        env.storage().persistent().get(&DataKey::EscrowTtlDeadline(cycle))
    }
}

