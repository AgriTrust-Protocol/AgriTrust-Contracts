#![no_std]
#[cfg(test)]
extern crate std;
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env, Vec};

#[cfg(test)]
mod test;

pub mod settlement;

pub const TTL_EXTENSION_PERIOD: u32 = 518_400; // 30 days in ledgers (~5s per ledger)
pub const MAX_SETTLEMENT_WINDOW: u64 = 30 * 24 * 60 * 60; // 30 days in seconds
pub const FEE_SAFETY_MARGIN: u64 = 8000; // 80% in basis points
pub const FEE_RESERVE_XLM: i128 = 5_000_000; // 5 XLM in stroops (1 XLM = 10^7 stroops)
pub const OPS_PER_HOP: u64 = 6000; // ~6000 operations per hop (2 reads + 1 write + 1 token transfer + 1 event)
pub const OPS_PER_TOKEN_TRANSFER: u64 = 5000; // Token transfer host function cost

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
    EscrowCycleCounter,
    ExpiredEscrows,
    SettlementHopSnapshot(u32), // Snapshot for rollback during partial settlement
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowLockData {
    pub buyer: Address,
    pub seller: Address,
    pub arbitration_id: u32,
    pub amount: i128,
    pub locked_at: u64,
    pub fee_reserve: i128, // Fee reserve held for this escrow
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
    PartiallyResolved,
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
    pub payout_hops: Vec<SettlementHop>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettlementHop {
    pub recipient: Address,
    pub amount: i128,
    pub is_fee: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SettlementStatus {
    Complete,
    Partial(u32), // Number of hops completed
}



#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettlementSnapshot {
    pub escrow_balance: i128,
    pub hops_completed: u32,
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
            payout_hops: Vec::new(&env),
        };

        env.storage().persistent().set(&DataKey::Dispute(counter), &dispute);
        counter
    }
    
    /// Settle a dispute with early fee budget check and fee reserve support
    pub fn settle_dispute(
        env: Env,
        cycle: u32,
        dispute_id: u32,
        fee_budget_xlm: i128,
        arbitrator: Address,
        payout_hops: Vec<SettlementHop>,
    ) -> SettlementStatus {
        settlement::settle_dispute(&env, cycle, dispute_id, fee_budget_xlm, &arbitrator, payout_hops)
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
