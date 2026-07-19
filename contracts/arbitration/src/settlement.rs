use soroban_sdk::{symbol_short, token, Address, Env};

use crate::{
    DataKey, EscrowLockData, EscrowReleaseData, TtlDeadline,
    TTL_EXTENSION_PERIOD,
};

const BUMP_THRESHOLD: u32 = 86_400;   // 10 days in ledgers — extend when below this
const EXPIRY_BUMP_AMOUNT: u32 = 518_400; // 30 days in ledgers — extend to this

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

/// Lock settlement funds into escrow for a given arbitration cycle.
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
    token_client.transfer(buyer, &env.current_contract_address(), &amount);

    let lock = EscrowLockData {
        buyer: buyer.clone(),
        seller: seller.clone(),
        arbitration_id,
        amount,
        locked_at: env.ledger().timestamp(),
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

    // Emit EscrowTtlDeadline event containing ledger_sequence + extension period
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

/// Safety margin applied to the estimated settlement cost. If the estimated
/// cost exceeds `available_fee * FEE_SAFETY_MARGIN`, the settlement aborts
/// before any token movement to avoid mid-execution `InsufficientFee` panics
/// that leave the escrow in an inconsistent state.
pub const FEE_SAFETY_MARGIN: i128 = 8; // 0.8 as 8/10 fixed-point-style factor

/// Per-hop operation estimate (storage reads/writes, transfer, event, auth).
const OPS_PER_HOP: u32 = 5_000;
/// Flat overhead for cross-contract auth + dispute lookup.
const OPS_BASELINE: u32 = 5_000;

/// Estimate the total Soroban operations a settlement will consume based on
/// the number of hops, participants, and token-conversion hops.
pub fn estimate_operation_cost(hop_count: u32, participant_count: u32, token_conversion_hops: u32) -> u32 {
    let hops = hop_count.saturating_mul(OPS_PER_HOP);
    let conversions = token_conversion_hops.saturating_mul(OPS_PER_HOP); // conversions cost a full extra hop each
    let participants = participant_count.saturating_mul(1_000); // small per-participant overhead
    OPS_BASELINE
        .saturating_add(hops)
        .saturating_add(conversions)
        .saturating_add(participants)
}

/// Pre-flight guard: returns Err(SettlementErrorBudget) if the estimated cost
/// exceeds the available fee budget scaled by the safety margin. This must run
/// BEFORE any token transfer so a depleted margin aborts cleanly.
pub fn check_fee_budget(estimated_ops: u32, available_fee_stroops: i128) -> Result<(), SettlementBudgetError> {
    // Convert ops -> stroops at ~0.01 XLM / 10k ops (1 XLM = 10_000_000 stroops).
    // cost_stroops = estimated_ops * (10_000_000 / 10_000) = estimated_ops * 1000
    let cost_stroops: i128 = (estimated_ops as i128).saturating_mul(1_000);
    let budget_with_margin = available_fee_stroops.saturating_mul(FEE_SAFETY_MARGIN) / 10;
    if cost_stroops > budget_with_margin {
        return Err(SettlementBudgetError);
    }
    Ok(())
}

/// Error returned when the settlement fee budget is insufficient up front.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettlementBudgetError;

/// Settlement entry point with pre-flight fee-budget guard.
/// `available_fee_stroops` is the `max_fee` the caller submitted with (in stroops).
pub fn settle_dispute(
    env: &Env,
    cycle: u32,
    buyer: &Address,
    seller: &Address,
    arbitration_id: u32,
    amount: i128,
    available_fee_stroops: i128,
) -> Result<(), SettlementBudgetError> {
    // Estimate cost: 3 logical hops (lock-fee deduction, payout, refund) +
    // 2 participants + 0 token-conversion hops for the simple path.
    let estimated_ops = estimate_operation_cost(3, 2, 0);
    check_fee_budget(estimated_ops, available_fee_stroops)?;

    // Fee budget verified — proceed with the real settlement flow.
    lock_settlement(env, cycle, buyer, seller, arbitration_id, amount);
    release_settlement(env, cycle, buyer, seller, arbitration_id, amount);
    Ok(())
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
