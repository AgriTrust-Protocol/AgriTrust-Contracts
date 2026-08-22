//! Deferred cliff mechanism for vesting schedules.
//!
//! The cliff check is deferred from creation time to the first release
//! attempt. When capital conversion incurs slippage (delay), the effective
//! cliff start is set to `actual_conversion_time` rather than the intended
//! `start_time`. This prevents grantees from receiving cliff-accrued tokens
//! when the conversion was delayed beyond the cliff period.

use crate::{VestingSchedule, VESTING_TTL_LEDGERS};
use agritrust_common::storage_keys::derive_storage_key;
use soroban_sdk::{contracttype, BytesN, Env, Symbol};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Cliff period in ledgers: 180 days at 5 seconds/ledger.
pub const CLIFF_PERIOD: u64 = 1_555_200;

/// Vesting period in ledgers: 365 days at 5 seconds/ledger.
pub const VESTING_PERIOD: u64 = 3_153_600;

/// Maximum acceptable conversion delay in ledgers: 14 days at 5 seconds/ledger.
pub const CONVERSION_DELAY_MAX: u64 = 120_960;

/// Slippage threshold in basis points: 500 bps = 5%.
pub const MAX_SLIPPAGE_BPS: u64 = 500;

// ---------------------------------------------------------------------------
// Delayed-conversion tracking
// ---------------------------------------------------------------------------

/// Per-grant record of when the capital conversion actually completed.
/// Stored separately from the schedule so the schedule struct preserves its
/// original shape for backward-compatible reads.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversionRecord {
    /// The actual conversion completion ledger (set after conversion).
    pub actual_conversion_time: u64,
    /// Basis-point slippage realized during conversion.
    pub slippage_bps: u64,
    /// Whether the conversion has been finalized.
    pub finalized: bool,
}

// ---------------------------------------------------------------------------
// Effective cliff computation
// ---------------------------------------------------------------------------

/// Compute the effective cliff start for a schedule, accounting for slippage.
///
/// - If `actual_conversion_time` <= `start_time + CLIFF_PERIOD`, cliff is
///   unaffected — returns `start_time + CLIFF_PERIOD`.
/// - If conversion finished after the cliff, the effective cliff start is
///   shifted to `actual_conversion_time`, reducing the vesting window.
/// - If the delay pushed the conversion past `start_time + CLIFF_PERIOD +
///   VESTING_PERIOD`, the schedule is invalid (returns `None`).
pub fn effective_cliff_start(
    start_time: u64,
    actual_conversion_time: u64,
) -> Option<u64> {
    let intended_cliff_end = start_time.saturating_add(CLIFF_PERIOD);
    if actual_conversion_time <= intended_cliff_end {
        // Conversion finished on time or early — standard cliff applies.
        return Some(intended_cliff_end);
    }
    // Conversion finished after the intended cliff end.
    let max_valid_time = start_time.saturating_add(CLIFF_PERIOD).saturating_add(VESTING_PERIOD);
    if actual_conversion_time >= max_valid_time {
        // Too late — entire vesting window consumed by delay.
        return None;
    }
    Some(actual_conversion_time)
}

/// Compute the adjusted total vestable amount after accounting for conversion delay.
///
/// Formula: `adjusted = total * (VESTING_PERIOD - delay_past_cliff) / VESTING_PERIOD`
pub fn adjusted_total_amount(
    total_amount: i128,
    start_time: u64,
    actual_conversion_time: u64,
) -> i128 {
    let intended_cliff_end = start_time.saturating_add(CLIFF_PERIOD);
    if actual_conversion_time <= intended_cliff_end {
        return total_amount;
    }
    let delay_past_cliff = actual_conversion_time.saturating_sub(intended_cliff_end);
    if delay_past_cliff >= VESTING_PERIOD {
        0
    } else {
        total_amount
            .saturating_mul(VESTING_PERIOD.saturating_sub(delay_past_cliff) as i128)
            .checked_div(VESTING_PERIOD as i128)
            .unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Data key helpers
// ---------------------------------------------------------------------------

fn conversion_record_key(env: &Env, grant_id: &BytesN<32>) -> BytesN<32> {
    // Use variant 2 to avoid collision with the schedule-v1 key (variant 1).
    derive_storage_key(env, crate::DOMAIN_VESTING, 2, grant_id)
}

fn initialization_flag_key(env: &Env, grant_id: &BytesN<32>) -> BytesN<32> {
    // Variant 3 for the "initialized" marker (first-release flag).
    derive_storage_key(env, crate::DOMAIN_VESTING, 3, grant_id)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Store a conversion record after capital conversion completes.
///
/// This is called by the treasury / capital conversion layer when the
/// conversion transaction lands on-ledger. It records the actual
/// conversion time for deferred-cliff computation.
pub fn record_conversion(
    env: &Env,
    grant_id: &BytesN<32>,
    actual_conversion_time: u64,
    slippage_bps: u64,
) {
    let key = conversion_record_key(env, grant_id);
    let record = ConversionRecord {
        actual_conversion_time,
        slippage_bps,
        finalized: true,
    };
    env.storage().persistent().set(&key, &record);
    env.storage()
        .persistent()
        .extend_ttl(&key, VESTING_TTL_LEDGERS, VESTING_TTL_LEDGERS);

    env.events().publish(
        (Symbol::new(env, "conversion_recorded"), grant_id.clone()),
        (actual_conversion_time, slippage_bps),
    );
}

/// Read a conversion record for a grant, if one exists.
pub fn read_conversion_record(env: &Env, grant_id: &BytesN<32>) -> Option<ConversionRecord> {
    let key = conversion_record_key(env, grant_id);
    if env.storage().persistent().has(&key) {
        let record: ConversionRecord = env.storage().persistent().get(&key).unwrap();
        Some(record)
    } else {
        None
    }
}

/// Returns true if the deferred-cliff initialization has already run for this grant.
pub fn is_initialized(env: &Env, grant_id: &BytesN<32>) -> bool {
    let flag_key = initialization_flag_key(env, grant_id);
    env.storage().persistent().has(&flag_key)
}

/// Notify the scheduler that the first `release_vested` call has arrived.
///
/// This is where the deferred cliff check happens.  If no conversion record
/// exists yet we fall back to the legacy `start_time + CLIFF_PERIOD` rule.
/// If a conversion record exists we compute the effective cliff from the
/// actual conversion time and, when required, shrink the stored schedule to
/// the adjusted vestable total.
///
/// Returns `None` if the schedule must be invalidated (delay consumed the
/// entire vesting window), otherwise returns `Some(())` with the schedule
/// updated in storage.
///
/// # Panics
///
/// Panics with `"slippage_exceeded"` if the recorded slippage exceeds
/// [`MAX_SLIPPAGE_BPS`].
pub fn apply_deferred_cliff(
    env: &Env,
    grant_id: &BytesN<32>,
    schedule: &mut VestingSchedule,
) -> Option<()> {
    let flag_key = initialization_flag_key(env, grant_id);
    if env.storage().persistent().has(&flag_key) {
        // Already processed — nothing to do on subsequent releases.
        return Some(());
    }

    // Mark as initialized so we only run this logic once.
    env.storage().persistent().set(&flag_key, &true);
    env.storage()
        .persistent()
        .extend_ttl(&flag_key, VESTING_TTL_LEDGERS, VESTING_TTL_LEDGERS);

    let record = read_conversion_record(env, grant_id);

    let actual_time = match record {
        Some(ref r) if r.finalized => r.actual_conversion_time,
        _ => {
            // No conversion record — treat ledger timestamp as the effective
            // conversion time.  This preserves the original on-chain behavior for
            // grants that do not go through the capital-conversion path.
            env.ledger().timestamp()
        }
    };

    // Validate slippage threshold.
    if let Some(ref r) = record {
        if r.slippage_bps > MAX_SLIPPAGE_BPS {
            env.events().publish(
                (Symbol::new(env, "slippage_exceeded"), grant_id.clone()),
                r.slippage_bps,
            );
            panic!("slippage_exceeded");
        }
    }

    let effective_start = match effective_cliff_start(schedule.start_time, actual_time) {
        Some(ts) => ts,
        None => {
            // Delay consumed the entire vesting window — invalidate.
            env.events().publish(
                (Symbol::new(env, "schedule_invalidated"), grant_id.clone()),
                actual_time,
            );
            return None;
        }
    };

    // Recompute the end time: effective_start + VESTING_PERIOD.
    schedule.end_time = effective_start.saturating_add(VESTING_PERIOD);

    // Adjusted total amount (may be unchanged if delay was within cliff).
    let adjusted =
        adjusted_total_amount(schedule.total_amount, schedule.start_time, actual_time);
    schedule.total_amount = adjusted;

    // Persist the updated schedule.
    let schedule_key = crate::vesting_schedule_key(env, grant_id);
    env.storage().persistent().set(&schedule_key, schedule);
    env.storage()
        .persistent()
        .extend_ttl(&schedule_key, VESTING_TTL_LEDGERS, VESTING_TTL_LEDGERS);

    env.events().publish(
        (Symbol::new(env, "cliff_applied"), grant_id.clone()),
        (effective_start, adjusted),
    );

    Some(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_cliff_unchanged_when_conversion_is_on_time() {
        let start: u64 = 10_000;
        // Conversion finished exactly at cliff end or before.
        assert_eq!(
            effective_cliff_start(start, start + CLIFF_PERIOD),
            Some(start + CLIFF_PERIOD)
        );
        assert_eq!(
            effective_cliff_start(start, start + 100),
            Some(start + CLIFF_PERIOD)
        );
    }

    #[test]
    fn effective_cliff_shifts_past_intended_end() {
        let start: u64 = 10_000;
        // Conversion finishes 10 ledgers after cliff end — shift forward by 10.
        let actual = start + CLIFF_PERIOD + 10;
        assert_eq!(effective_cliff_start(start, actual), Some(actual));
    }

    #[test]
    fn effective_cliff_none_when_delay_exceeds_vesting() {
        let start: u64 = 10_000;
        // Conversion finishes after cliff + vesting = schedule invalid.
        let actual = start + CLIFF_PERIOD + VESTING_PERIOD + 1;
        assert_eq!(effective_cliff_start(start, actual), None);
    }

    #[test]
    fn effective_cliff_none_at_exact_vesting_boundary() {
        let start: u64 = 10_000;
        // Conversion finishes exactly at cliff + vesting = invalid (>= check).
        let actual = start + CLIFF_PERIOD + VESTING_PERIOD;
        assert_eq!(effective_cliff_start(start, actual), None);
    }

    #[test]
    fn adjusted_total_unchanged_within_cliff() {
        let total: i128 = 1_000_000_000; // 1000 tokens (7 decimals)
        let start: u64 = 10_000;
        // Conversion on time → no adjustment.
        assert_eq!(
            adjusted_total_amount(total, start, start + CLIFF_PERIOD),
            total
        );
        assert_eq!(adjusted_total_amount(total, start, start), total);
    }

    #[test]
    fn adjusted_total_reduced_past_cliff() {
        // 1000 tokens over 365-day vesting.  Delay = 36.5 days (exactly 10%).
        let total: i128 = 1_000_000_000;
        let start: u64 = 10_000;
        let delay_past_cliff: u64 = VESTING_PERIOD / 10; // 10% of vesting period
        let actual = start + CLIFF_PERIOD + delay_past_cliff;
        let expected = total * (VESTING_PERIOD - delay_past_cliff) as i128 / VESTING_PERIOD as i128;
        assert_eq!(adjusted_total_amount(total, start, actual), expected);
    }

    #[test]
    fn adjusted_total_zero_when_delay_consumes_full_vesting() {
        let total: i128 = 1_000_000_000;
        let start: u64 = 10_000;
        let actual = start + CLIFF_PERIOD + VESTING_PERIOD;
        assert_eq!(adjusted_total_amount(total, start, actual), 0);
    }

    #[test]
    fn adjusted_total_zero_when_delay_exceeds_full_vesting() {
        let total: i128 = 1_000_000_000;
        let start: u64 = 10_000;
        let actual = start + CLIFF_PERIOD + VESTING_PERIOD + 100;
        assert_eq!(adjusted_total_amount(total, start, actual), 0);
    }

    #[test]
    fn adjusted_total_half_when_half_vesting_delayed() {
        let total: i128 = 1_000_000_000;
        let start: u64 = 10_000;
        // Delay = exactly half the vesting period past cliff.
        let delay_past_cliff: u64 = VESTING_PERIOD / 2;
        let actual = start + CLIFF_PERIOD + delay_past_cliff;
        // adjusted = total * (VESTING_PERIOD - VESTING_PERIOD/2) / VESTING_PERIOD = total * 0.5
        let expected = total / 2;
        assert_eq!(adjusted_total_amount(total, start, actual), expected);
    }
}
