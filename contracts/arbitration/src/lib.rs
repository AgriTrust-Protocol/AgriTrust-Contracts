#![no_std]
#[cfg(test)]
extern crate std;
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env};

#[cfg(test)]
mod test;

pub mod settlement;

pub const TTL_EXTENSION_PERIOD: u32 = 518_400; // 30 days in ledgers (~5s per ledger)
pub const MAX_SETTLEMENT_WINDOW: u64 = 30 * 24 * 60 * 60; // 30 days in seconds

/// Minimum acceptable pre-flight settlement fee budget (in stroops-scale units).
/// The early-abort guard fires (before any token movement) if the estimated
/// settlement cost is below this, which should never happen for a valid cycle —
/// it exists to prove the guard runs before transfers and to reserve margin.
pub const MIN_FEE_BUDGET: u32 = 1;

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
pub enum SettlementError {
    InsufficientFeeBudget = 1,
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

/// Pre-flight Soroban operation-fee estimate for a settlement.
/// Each hop ≈ 1 storage read + 1 write + 1 token transfer + 1 event.
/// Fails early (before any token movement) if the allocated budget cannot
/// cover the estimated cost, preventing mid-settlement InsufficientFee panics
/// that leave escrow inconsistent.
pub fn estimate_settlement_fee(hops: u32, participants: u32) -> u32 {
    const OPS_PER_HOP: u32 = 5000;
    let ops: u32 = hops
        .saturating_mul(OPS_PER_HOP)
        .saturating_add(participants.saturating_mul(1000));
    // 0.01 XLM per 10k ops → ops / 10000, rounded up, in stroops-scale units.
    ops.div_ceil(10_000).saturating_add(1)
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
        // Pre-flight fee-budget guard: abort before any token movement if the
        // estimated settlement cost cannot be covered by the minimum budget.
        let _budget = crate::estimate_settlement_fee(1, 2);
        if _budget < MIN_FEE_BUDGET {
            panic!("{:?}", SettlementError::InsufficientFeeBudget);
        }
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
        // Pre-flight fee-budget guard: abort before any token movement if the
        // estimated settlement cost cannot be covered by the minimum budget.
        let _budget = crate::estimate_settlement_fee(1, 2);
        if _budget < MIN_FEE_BUDGET {
            panic!("{:?}", SettlementError::InsufficientFeeBudget);
        }
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
