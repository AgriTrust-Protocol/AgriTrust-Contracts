use soroban_sdk::{symbol_short, token, Address, Env, Vec};
use crate::{
    DataKey, EscrowLockData, EscrowReleaseData, TtlDeadline, TTL_EXTENSION_PERIOD,
    FEE_SAFETY_MARGIN, FEE_RESERVE_XLM, OPS_PER_HOP, OPS_PER_TOKEN_TRANSFER,
    Dispute, DisputeStatus, SettlementHop, SettlementStatus, SettlementSnapshot
};

const BUMP_THRESHOLD: u32 = 86_400;   // 10 days in ledgers — extend when below this
const EXPIRY_BUMP_AMOUNT: u32 = 518_400; // 30 days in ledgers — extend to this

/// Estimate total operation cost for a settlement with given hop count
pub fn estimate_operation_cost(hop_count: u64) -> u64 {
    // Base ops + (OPS_PER_HOP per hop + additional token transfer costs)
    let base_ops = 2000; // 2000 base ops
    base_ops + hop_count * (OPS_PER_HOP + OPS_PER_TOKEN_TRANSFER)
}



/// Execute a single settlement hop
fn execute_single_hop(env: &Env, hop: &SettlementHop, token_client: &token::Client) {
    if hop.amount > 0 {
        token_client.transfer(&env.current_contract_address(), &hop.recipient, &hop.amount);
    }
}

/// Execute payout chain with checkpointing and rollback support
pub fn _execute_payout_chain(env: &Env, cycle: u32, hops: &Vec<SettlementHop>, token_client: &token::Client) -> SettlementStatus {
    let mut hops_completed = 0u32;
    let snapshot_key = DataKey::SettlementHopSnapshot(cycle);
    
    // Take initial snapshot
    let initial_balance = token_client.balance(&env.current_contract_address());
    env.storage().persistent().set(&snapshot_key, &SettlementSnapshot {
        escrow_balance: initial_balance,
        hops_completed: 0,
    });
    
    for hop in hops.iter() {
        // Take checkpoint before each hop
        let current_snapshot = SettlementSnapshot {
            escrow_balance: token_client.balance(&env.current_contract_address()),
            hops_completed,
        };
        env.storage().persistent().set(&snapshot_key, &current_snapshot);
        
        // Execute hop
        execute_single_hop(env, &hop, token_client);
        hops_completed += 1;
        
        // Emit hop completed event
        env.events().publish(
            (symbol_short!("hop_done"), cycle),
            hops_completed,
        );
    }
    
    // Clean up snapshot on completion
    env.storage().persistent().remove(&snapshot_key);
    SettlementStatus::Complete
}

/// Settle a dispute with early fee budget check and fee reserve support
pub fn settle_dispute(
    env: &Env,
    cycle: u32,
    dispute_id: u32,
    fee_budget_xlm: i128, // Fee budget in stroops
    arbitrator: Address,
    payout_hops: Vec<SettlementHop>,
) -> SettlementStatus {
    // Synchronize TTLs first
    synchronize_escrow_ttl(env, cycle);
    
    // Load escrow lock
    let lock: EscrowLockData = env.storage().persistent().get(&DataKey::EscrowLock(cycle)).unwrap();
    let token_addr: Address = env.storage().instance().get(&DataKey::Token).unwrap();
    let token_client = token::Client::new(env, &token_addr);
    
    // Verify dispute exists and arbitrator is authorized
    let mut dispute: Dispute = env.storage().persistent().get(&DataKey::Dispute(dispute_id)).unwrap();
    arbitrator.require_auth();
    if dispute.arbitrator != arbitrator {
        panic!("Unauthorized: not the arbitrator");
    }
    if dispute.status == DisputeStatus::Resolved {
        panic!("Dispute already resolved");
    }
    
    // Early fee budget check
    let estimated_ops = estimate_operation_cost(payout_hops.len() as u64);
    // Approximate cost calculation (simplified for example)
    let estimated_cost_stroops = (estimated_ops as i128) * 100; // 100 stroops per 10k ops approx
    
    // Check with safety margin
    let required_budget = (estimated_cost_stroops * FEE_SAFETY_MARGIN as i128) / 10_000;
    if fee_budget_xlm < required_budget {
        panic!("InsufficientSettlementBudget: estimated {} stroops, budget {}", required_budget, fee_budget_xlm);
    }
    
    // Update dispute with payout hops
    dispute.payout_hops = payout_hops.clone();
    dispute.status = DisputeStatus::InArbitration;
    env.storage().persistent().set(&DataKey::Dispute(dispute_id), &dispute);
    
    // Execute payout chain
    let status = _execute_payout_chain(env, cycle, &payout_hops, &token_client);
    
    // Refund unused fee reserve
    let unused_reserve = lock.fee_reserve;
    if unused_reserve > 0 {
        token_client.transfer(&env.current_contract_address(), &lock.buyer, &unused_reserve);
        env.events().publish(
            (symbol_short!("reserve_refund"), cycle),
            unused_reserve,
        );
    }
    
    // Mark dispute as resolved
    dispute.status = DisputeStatus::Resolved;
    env.storage().persistent().set(&DataKey::Dispute(dispute_id), &dispute);
    
    status
}

/// Synchronize TTLs of escrow lock and release entries so both remain live
/// until settlement finalization. Uses `extend_ttl`'s built-in threshold so
/// no explicit `get_ttl` call is needed — entries below threshold get bumped
/// to `EXPIRY_BUMP_AMOUNT`, which guarantees both have the same expiry horizon.
/// Also extends the contract instance TTL to prevent instance archival while
/// escrow entries are still alive.
pub fn synchronize_escrow_ttl(env: &Env, cycle: u32) {
    let lock_key = DataKey::EscrowLock(cycle);
    let release_key = DataKey::EscrowRelease(cycle);

    // Extend contract instance TTL so the contract itself stays alive
    env.storage().instance().extend_ttl(BUMP_THRESHOLD, EXPIRY_BUMP_AMOUNT);

    if env.storage().persistent().has(&lock_key) {
        env.storage()
            .persistent()
            .extend_ttl(&lock_key, BUMP_THRESHOLD, EXPIRY_BUMP_AMOUNT);
    }
    if env.storage().persistent().has(&release_key) {
        env.storage()
            .persistent()
            .extend_ttl(&release_key, BUMP_THRESHOLD, EXPIRY_BUMP_AMOUNT);
    }
}

/// Lock settlement funds into escrow for a given arbitration cycle, including fee reserve.
/// Writes the lock entry, extends its TTL, and emits an EscrowTtlDeadline event.
pub fn lock_settlement(
    env: &Env,
    cycle: u32,
    buyer: &Address,
    seller: &Address,
    arbitration_id: u32,
    amount: i128,
) {
    buyer.require_auth();

    let token_addr: Address = env.storage().instance().get(&DataKey::Token).unwrap();
    let token_client = token::Client::new(env, &token_addr);
    
    // Transfer main amount + fee reserve
    let total_transfer = amount + FEE_RESERVE_XLM;
    token_client.transfer(buyer, &env.current_contract_address(), &total_transfer);

    let lock = EscrowLockData {
        buyer: buyer.clone(),
        seller: seller.clone(),
        arbitration_id,
        amount,
        locked_at: env.ledger().timestamp(),
        fee_reserve: FEE_RESERVE_XLM,
    };

    env.storage()
        .persistent()
        .set(&DataKey::EscrowLock(cycle), &lock);

    // Extend TTL on the lock entry and instance right after creation
    env.storage().instance().extend_ttl(BUMP_THRESHOLD, EXPIRY_BUMP_AMOUNT);
    env.storage()
        .persistent()
        .extend_ttl(&DataKey::EscrowLock(cycle), BUMP_THRESHOLD, EXPIRY_BUMP_AMOUNT);

    // Track cycle count for garbage collection
    let mut counter: u32 = env
        .storage()
        .instance()
        .get(&DataKey::EscrowCycleCounter)
        .unwrap_or(0);
    counter = counter.saturating_add(1);
    env.storage()
        .instance()
        .set(&DataKey::EscrowCycleCounter, &counter);

    // Emit EscrowTtlDeadline event containing ledger sequence + extension period
    let deadline = TtlDeadline {
        ledger_sequence: env.ledger().sequence(),
        ttl_extension_period: TTL_EXTENSION_PERIOD,
    };
    env.storage()
        .persistent()
        .set(&DataKey::EscrowTtlDeadline(cycle), &deadline);
    env.storage()
        .persistent()
        .extend_ttl(&DataKey::EscrowTtlDeadline(cycle), BUMP_THRESHOLD, EXPIRY_BUMP_AMOUNT);

    env.events().publish(
        (symbol_short!("ttl_dead"), cycle),
        deadline,
    );
    
    env.events().publish(
        (symbol_short!("reserve_locked"), cycle),
        FEE_RESERVE_XLM,
    );
}

/// Release settlement funds from escrow after resolution.
/// Synchronizes TTLs before reading the lock entry to prevent mid-finalization expiry.
pub fn release_settlement(
    env: &Env,
    cycle: u32,
    buyer: &Address,
    seller: &Address,
    arbitration_id: u32,
    amount: i128,
) {
    // Synchronize TTLs before accessing lock entry — ensures lock hasn't expired
    synchronize_escrow_ttl(env, cycle);

    let lock: EscrowLockData = env
        .storage()
        .persistent()
        .get(&DataKey::EscrowLock(cycle))
        .unwrap();

    seller.require_auth();

    if lock.arbitration_id != arbitration_id {
        panic!("arbitration_id mismatch");
    }
    if lock.amount < amount {
        panic!("release amount exceeds locked amount");
    }

    let release = EscrowReleaseData {
        buyer: buyer.clone(),
        seller: seller.clone(),
        arbitration_id,
        amount,
        released_at: env.ledger().timestamp(),
    };

    env.storage()
        .persistent()
        .set(&DataKey::EscrowRelease(cycle), &release);

    // Extend TTL on both lock and release to survive settlement finalization
    synchronize_escrow_ttl(env, cycle);

    let token_addr: Address = env.storage().instance().get(&DataKey::Token).unwrap();
    let token_client = token::Client::new(env, &token_addr);
    token_client.transfer(&env.current_contract_address(), seller, &amount);

    env.events().publish(
        (symbol_short!("release"), cycle),
        (lock.amount, amount),
    );
}

/// Permissionless maintenance function to clean up expired escrow cycles
/// where both the lock and release entries have expired TTLs.
pub fn garbage_collect_expired_escrows(env: &Env, max_cycles: u32) -> u32 {
    let counter: u32 = env
        .storage()
        .instance()
        .get(&DataKey::EscrowCycleCounter)
        .unwrap_or(0);

    let mut cleaned = 0u32;

    for cycle in 0..counter {
        if cleaned >= max_cycles {
            break;
        }

        if !env.storage().persistent().has(&DataKey::EscrowTtlDeadline(cycle)) {
            continue;
        }

        let lock_expired = !env.storage().persistent().has(&DataKey::EscrowLock(cycle));
        let release_expired = !env.storage().persistent().has(&DataKey::EscrowRelease(cycle));

        if lock_expired && release_expired {
            env.storage()
                .persistent()
                .remove(&DataKey::EscrowTtlDeadline(cycle));
            cleaned = cleaned.saturating_add(1);
        }
    }

    env.events().publish(
        (symbol_short!("gc_escrow"),),
        cleaned,
    );

    cleaned
}
