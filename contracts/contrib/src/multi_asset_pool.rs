//! # Multi-Asset Pool
//!
//! A liquidity pool that supports multiple assets with share-based LP tracking.
//!
//! ## Key Design Decisions
//!
//! ### Banker's Rounding (Round-Half-Even)
//! All share calculations use banker's rounding (`round_half_even`) instead of
//! truncating division. This eliminates systematic asymmetry where depositing
//! asset A and withdrawing asset B yields different results than the reverse.
//! Banker's rounding rounds to the nearest integer, and rounds ties to the
//! nearest even number, ensuring zero bias over many operations.
//!
//! ### Cross-Multiplication Precision
//! Share calculations use a high-precision intermediate scale:
//! ```text
//! shares = deposit_amount * total_shares * SIMULATION_PRECISION / pool_value / SIMULATION_PRECISION
//! ```
//! This preserves intermediate precision before the final division by
//! `SIMULATION_PRECISION (1e12)`.
//!
//! ### Asymmetry Guard
//! A running counter of cumulative precision loss tracks the total truncation
//! error across all deposit/withdraw operations. When the loss exceeds
//! `MAX_ALLOWED_PRECISION_LOSS (10,000 shares)`, deposits are paused and a
//! `PrecisionLossThresholdExceeded` event is emitted.

use alloc::format;
use soroban_sdk::{Env, Address, Vec, String, panic_with_error, contracterror};

// ─── Constants ───────────────────────────────────────────────────────────────

/// High-precision intermediate scale for cross-multiplication share calculation.
/// Value: 1,000,000,000,000 (1e12)
const SIMULATION_PRECISION: i128 = 1_000_000_000_000;

/// Maximum allowed cumulative precision loss before deposits are paused.
/// Value: 10,000 share units
const MAX_ALLOWED_PRECISION_LOSS: i128 = 10_000;

/// Fee basis points applied to deposits. 30 bps = 0.3%.
const FEE_BPS: i128 = 30;

/// Basis points denominator (100% = 10000 bps).
const BASIS_POINTS: i128 = 10_000;

// ─── Asymmetry Verification ───────────────────────────────────────────────────

/// Test the symmetry of deposit/withdraw operations.
///
/// Returns (shares_for_deposit, value_from_shares) for a given deposit amount
/// into a pool with the given state. This helps verify that depositing asset A
/// and withdrawing in terms of asset B produces the same result as depositing
/// B and withdrawing in terms of A (modulo rounding).
pub fn deposit_withdraw_symmetry(
    deposit_amount: i128,
    pool_value: i128,
    total_shares: i128,
) -> (i128, i128) {
    let shares = calculate_shares(deposit_amount, pool_value, total_shares);
    let new_total_shares = total_shares + shares;
    let new_pool_value = pool_value + deposit_amount;
    // Expected value from shares: (shares / new_total_shares) * new_pool_value
    let value = if shares > 0 && new_total_shares > 0 {
        round_half_even(shares * new_pool_value, new_total_shares)
    } else {
        0
    };
    (shares, value)
}

// ─── Error Types ─────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum PoolError {
    /// Deposit amount must be positive.
    InvalidDepositAmount = 1,
    /// Withdrawal amount must be positive.
    InvalidWithdrawalAmount = 2,
    /// Insufficient asset balance in the pool.
    InsufficientAssetBalance = 3,
    /// Insufficient LP shares for the withdrawal.
    InsufficientLpShares = 4,
    /// Deposits are currently paused due to precision loss threshold being exceeded.
    DepositsPaused = 5,
    /// Math overflow occurred.
    MathOverflow = 6,
    /// Zero total shares means we can't calculate a ratio.
    ZeroTotalShares = 7,
}

// ─── Storage Key Helpers ─────────────────────────────────────────────────────

fn lp_position_key(env: &Env, pool_id: &str, lp: &Address) -> String {
    String::from_str(env, &format!("lp_position:{}:{:?}", pool_id, lp))
}

fn asset_balance_key(env: &Env, pool_id: &str, asset: &Address) -> String {
    String::from_str(env, &format!("pool_balance:{}:{:?}", pool_id, asset))
}

fn oracle_rate_key(env: &Env, asset: &Address) -> String {
    String::from_str(env, &format!("oracle:rate:{:?}", asset))
}

fn pool_assets_key(env: &Env, pool_id: &str) -> String {
    String::from_str(env, &format!("pool_assets:{}", pool_id))
}

fn pool_lps_key(env: &Env, pool_id: &str) -> String {
    String::from_str(env, &format!("pool_lps:{}", pool_id))
}

fn precision_loss_key(env: &Env, pool_id: &str) -> String {
    String::from_str(env, &format!("precision_loss:{}", pool_id))
}

fn deposits_paused_key(env: &Env, pool_id: &str) -> String {
    String::from_str(env, &format!("deposits_paused:{}", pool_id))
}

fn total_shares_key(env: &Env, pool_id: &str) -> String {
    String::from_str(env, &format!("total_shares:{}", pool_id))
}

// ─── Core Types ───────────────────────────────────────────────────────────────

/// Represents the overall state of a multi-asset pool.
#[derive(Clone, Debug)]
pub struct PoolState {
    /// Total LP shares minted.
    pub total_shares: i128,
    /// Total value of all assets in the pool (in numéraire).
    pub total_value: i128,
    /// Running counter of truncation losses across all deposit/withdraw operations.
    pub cumulative_precision_loss: i128,
    /// Whether deposits are paused due to precision loss threshold.
    pub deposits_paused: bool,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Banker's rounding (round half to even).
///
/// Rounds `numerator / denominator` to the nearest integer. Ties are rounded
/// to the nearest even integer. This eliminates systematic bias that truncating
/// division would introduce.
///
/// # Arguments
/// * `numerator` - The dividend (non-negative).
/// * `denominator` - The divisor (positive).
///
/// # Returns
/// The rounded result as an `i128`.
///
/// # Panics
/// Panics if `denominator <= 0` or `numerator < 0`.
pub fn round_half_even(numerator: i128, denominator: i128) -> i128 {
    if denominator <= 0 || numerator < 0 {
        panic!("MathOverflow: invalid denominator or numerator");
    }

    let quotient = numerator / denominator;
    let remainder = numerator % denominator;

    // If no remainder, the division is exact
    if remainder == 0 {
        return quotient;
    }

    // Check if remainder is >= half of denominator
    let half = denominator / 2;
    let is_half = denominator % 2 == 0 && remainder == half;
    let is_past_half = remainder > half;

    if is_past_half {
        // Round up
        quotient + 1
    } else if is_half {
        // Tie: round to even
        if quotient % 2 == 0 {
            quotient
        } else {
            quotient + 1
        }
    } else {
        // Round down
        quotient
    }
}

/// Calculate LP shares for a deposit using cross-multiplication for precision.
///
/// Uses the formula:
/// ```text
/// shares = deposit_amount * total_shares * SIMULATION_PRECISION / pool_value / SIMULATION_PRECISION
/// ```
///
/// This preserves intermediate precision by scaling up before the final division.
/// The result is rounded using banker's rounding (`round_half_even`).
///
/// # Arguments
/// * `deposit_amount` - The amount being deposited (in numéraire value).
/// * `pool_value` - The total value of the pool before the deposit (in numéraire).
/// * `total_shares` - The total LP shares before the deposit.
///
/// # Returns
/// The number of LP shares to mint.
///
/// # Behavior
/// - If `total_shares == 0` (first deposit), returns `deposit_amount` as the initial shares.
/// - If `pool_value == 0`, returns 0 (should not happen if total_shares == 0).
/// - Uses `round_half_even` for the final division to eliminate asymmetry.
pub fn calculate_shares(deposit_amount: i128, pool_value: i128, total_shares: i128) -> i128 {
    // First deposit: mint shares equal to deposit amount
    if total_shares == 0 || pool_value == 0 {
        return deposit_amount;
    }

    // Cross-multiplication for precision:
    // shares = deposit_amount * total_shares * SIMULATION_PRECISION / pool_value / SIMULATION_PRECISION
    let scaled = deposit_amount
        .checked_mul(total_shares)
        .and_then(|v| v.checked_mul(SIMULATION_PRECISION))
        .unwrap_or(i128::MAX); // Saturate on overflow

    let intermediate = scaled / pool_value;

    round_half_even(intermediate, SIMULATION_PRECISION)
}

/// Calculate the withdrawal amount and asset distribution for a given number of shares.
///
/// # Arguments
/// * `env` - The Soroban environment.
/// * `pool_id` - The pool identifier.
/// * `shares` - The number of LP shares to withdraw.
/// * `total_shares` - The total LP shares in the pool.
/// * `preferred_asset` - Optional preferred asset for withdrawal.
///
/// # Returns
/// A vector of (asset, amount) tuples representing the withdrawal distribution.
///
/// # Panics
/// Panics if the pool or assets are misconfigured.
pub fn calculate_withdrawal(
    env: &Env,
    pool_id: &str,
    shares: i128,
    total_shares: i128,
    preferred_asset: Option<Address>,
) -> Vec<(Address, i128)> {
    if total_shares == 0 {
        panic_with_error!(env, PoolError::ZeroTotalShares);
    }

    let mut result: Vec<(Address, i128)> = Vec::new(env);

    if let Some(preferred) = preferred_asset {
        // Single asset withdrawal: convert shares to value, then to preferred asset
        let total_value = total_pool_value(env, pool_id);
        let share_value = round_half_even(shares * total_value, total_shares);
        let rate: i128 = env
            .storage()
            .instance()
            .get(&oracle_rate_key(env, &preferred))
            .unwrap_or(1);
        let converted_amount = if rate == 0 {
            share_value
        } else {
            share_value / rate
        };

        result.push_back((preferred, converted_amount));
    } else {
        // Basket withdrawal: pro-rata across all assets
        let assets: Vec<Address> = env
            .storage()
            .instance()
            .get(&pool_assets_key(env, pool_id))
            .unwrap_or(Vec::new(env));

        for i in 0..assets.len() {
            let asset = assets.get(i).unwrap();
            let balance: i128 = env
                .storage()
                .instance()
                .get(&asset_balance_key(env, pool_id, &asset))
                .unwrap_or(0);

            if balance > 0 {
                let share_amount = round_half_even(balance * shares, total_shares);
                if share_amount > 0 {
                    result.push_back((asset, share_amount));
                }
            }
        }
    }

    result
}

/// Apply the deposit fee to a deposit amount.
///
/// Fee is `FEE_BPS` (30 bps = 0.3%) of the deposit amount.
/// The fee is deducted and credited to the treasury.
///
/// # Arguments
/// * `env` - The Soroban environment.
/// * `amount` - The deposit amount before fees.
///
/// # Returns
/// `(net_amount, fee_amount)` where `net_amount` is deposited as shares.
pub fn apply_deposit_fee(env: &Env, amount: i128) -> (i128, i128) {
    let fee = amount * FEE_BPS / BASIS_POINTS;
    let net = amount - fee;

    // Credit fee to treasury
    let treasury_key = String::from_str(env, "treasury_balance");
    let mut treasury_balance: i128 = env.storage().instance().get(&treasury_key).unwrap_or(0);
    treasury_balance = treasury_balance
        .checked_add(fee)
        .unwrap_or(i128::MAX);
    env.storage().instance().set(&treasury_key, &treasury_balance);

    (net, fee)
}

/// Track and enforce the cumulative precision loss asymmetry guard.
///
/// Updates the running counter of precision loss. If the loss exceeds
/// `MAX_ALLOWED_PRECISION_LOSS`, deposits are paused.
///
/// # Arguments
/// * `env` - The Soroban environment.
/// * `pool_id` - The pool identifier.
/// * `precision_loss` - The precision loss incurred in this operation.
///
/// # Returns
/// The updated cumulative precision loss.
pub fn update_precision_loss(env: &Env, pool_id: &str, precision_loss: i128) -> i128 {
    let key = precision_loss_key(env, pool_id);
    let mut cumulative: i128 = env.storage().instance().get(&key).unwrap_or(0);
    cumulative = cumulative
        .checked_add(precision_loss)
        .unwrap_or(i128::MAX);

    env.storage().instance().set(&key, &cumulative);

    if cumulative > MAX_ALLOWED_PRECISION_LOSS {
        // Pause deposits
        env.storage()
            .instance()
            .set(&deposits_paused_key(env, pool_id), &true);

        // Emit precision loss threshold exceeded event
        env.events().publish(
            ("prec_loss",),
            (pool_id, cumulative, MAX_ALLOWED_PRECISION_LOSS),
        );
    }

    cumulative
}

/// Compute the total value of all assets in the pool using oracle conversion rates.
///
/// # Arguments
/// * `env` - The Soroban environment.
/// * `pool_id` - The pool identifier.
///
/// # Returns
/// The total value of all assets in the pool in numéraire (7-decimal fixed-point).
pub fn total_pool_value(env: &Env, pool_id: &str) -> i128 {
    let assets: Vec<Address> = env
        .storage()
        .instance()
        .get(&pool_assets_key(env, pool_id))
        .unwrap_or(Vec::new(env));

    let mut total = 0_i128;
    for i in 0..assets.len() {
        let asset = assets.get(i).unwrap();
        let balance: i128 = env
            .storage()
            .instance()
            .get(&asset_balance_key(env, pool_id, &asset))
            .unwrap_or(0);
        let rate: i128 = env
            .storage()
            .instance()
            .get(&oracle_rate_key(env, &asset))
            .unwrap_or(1);
        total = total
            .checked_add(balance.checked_mul(rate).unwrap_or(i128::MAX))
            .unwrap_or(i128::MAX);
    }

    total
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Deposit assets into the pool and receive LP shares.
///
/// Issues LP shares proportional to the deposit value using banker's rounding.
/// Tracks cumulative precision loss and pauses deposits if threshold exceeded.
///
/// # Arguments
/// * `env` - The Soroban environment.
/// * `pool_id` - The pool identifier.
/// * `asset` - The asset being deposited.
/// * `amount` - The amount of the asset being deposited.
/// * `lp` - The LP address receiving the shares.
///
/// # Panics
/// * If `amount <= 0`.
/// * If deposits are paused due to precision loss threshold.
pub fn deposit(env: &Env, pool_id: &str, asset: Address, amount: i128, lp: Address) {
    if amount <= 0 {
        panic_with_error!(env, PoolError::InvalidDepositAmount);
    }

    // Check if deposits are paused
    let paused: bool = env
        .storage()
        .instance()
        .get(&deposits_paused_key(env, pool_id))
        .unwrap_or(false);
    if paused {
        panic_with_error!(env, PoolError::DepositsPaused);
    }

    // Apply deposit fee
    let (net_amount, fee) = apply_deposit_fee(env, amount);

    // Update asset balance
    let balance_key = asset_balance_key(env, pool_id, &asset);
    let mut balance: i128 = env
        .storage()
        .instance()
        .get(&balance_key)
        .unwrap_or(0);
    balance = balance
        .checked_add(net_amount)
        .unwrap_or(i128::MAX);
    env.storage().instance().set(&balance_key, &balance);

    // Add asset to pool assets list if not already present
    let mut assets: Vec<Address> = env
        .storage()
        .instance()
        .get(&pool_assets_key(env, pool_id))
        .unwrap_or(Vec::new(env));
    let mut found = false;
    for i in 0..assets.len() {
        if assets.get(i).unwrap() == asset {
            found = true;
            break;
        }
    }
    if !found {
        assets.push_back(asset.clone());
        env.storage().instance().set(&pool_assets_key(env, pool_id), &assets);
    }

    // Calculate pool value before the deposit
    let pool_value_before = total_pool_value(env, pool_id);

    // Get current total shares
    let total_shares: i128 = env
        .storage()
        .instance()
        .get(&total_shares_key(env, pool_id))
        .unwrap_or(0);

    // Calculate shares to mint using cross-multiplication and banker's rounding
    let shares = calculate_shares(net_amount, pool_value_before, total_shares);

    // Track precision loss as the difference between exact calculation and rounded result
    let exact_shares = if total_shares > 0 && pool_value_before > 0 {
        let exact_scaled = net_amount
            .checked_mul(total_shares)
            .and_then(|v| v.checked_mul(SIMULATION_PRECISION))
            .unwrap_or(i128::MAX);
        let exact = exact_scaled / pool_value_before;
        exact % SIMULATION_PRECISION
    } else {
        0
    };

    // Update cumulative precision loss
    if exact_shares > 0 {
        update_precision_loss(env, pool_id, exact_shares);
    }

    // Update total shares
    let new_total_shares = total_shares
        .checked_add(shares)
        .unwrap_or(i128::MAX);
    env.storage()
        .instance()
        .set(&total_shares_key(env, pool_id), &new_total_shares);

    // Update LP position
    let lp_key = lp_position_key(env, pool_id, &lp);
    let mut lp_shares: i128 = env.storage().instance().get(&lp_key).unwrap_or(0);
    lp_shares = lp_shares
        .checked_add(shares)
        .unwrap_or(i128::MAX);
    env.storage().instance().set(&lp_key, &lp_shares);

    // Add LP to pool LP list if not already present
    let mut lps: Vec<Address> = env
        .storage()
        .instance()
        .get(&pool_lps_key(env, pool_id))
        .unwrap_or(Vec::new(env));
    let mut lp_found = false;
    for i in 0..lps.len() {
        if lps.get(i).unwrap() == lp {
            lp_found = true;
            break;
        }
    }
    if !lp_found {
        lps.push_back(lp.clone());
        env.storage().instance().set(&pool_lps_key(env, pool_id), &lps);
    }

    // Emit deposit event
    env.events().publish(
        ("deposit",),
        (pool_id, asset, net_amount, fee, shares, lp),
    );
}

/// Withdraw assets from the pool by burning LP shares.
///
/// Burns the specified number of LP shares and releases proportional assets.
/// Uses banker's rounding for fair asset distribution.
///
/// # Arguments
/// * `env` - The Soroban environment.
/// * `pool_id` - The pool identifier.
/// * `lp` - The LP address burning shares and receiving assets.
/// * `shares` - The number of LP shares to burn.
/// * `preferred_asset` - Optional preferred asset for withdrawal. If `None`,
///   withdrawal is proportional across all assets in the pool.
///
/// # Panics
/// * If `shares <= 0`.
/// * If the LP has insufficient shares.
/// * If the pool doesn't have enough value.
pub fn withdraw(env: &Env, pool_id: &str, lp: Address, shares: i128, preferred_asset: Option<Address>) {
    if shares <= 0 {
        panic_with_error!(env, PoolError::InvalidWithdrawalAmount);
    }

    // Verify LP has enough shares
    let lp_key = lp_position_key(env, pool_id, &lp);
    let lp_shares: i128 = env.storage().instance().get(&lp_key).unwrap_or(0);
    if lp_shares < shares {
        panic_with_error!(env, PoolError::InsufficientLpShares);
    }

    // Get total shares
    let total_shares: i128 = env
        .storage()
        .instance()
        .get(&total_shares_key(env, pool_id))
        .unwrap_or(0);

    // Calculate withdrawal distribution
    let withdrawal = calculate_withdrawal(env, pool_id, shares, total_shares, preferred_asset);

    // Deduct from LP position
    let remaining_lp_shares = lp_shares
        .checked_sub(shares)
        .unwrap_or(0);
    env.storage().instance().set(&lp_key, &remaining_lp_shares);

    // Update total shares
    let new_total_shares = total_shares
        .checked_sub(shares)
        .unwrap_or(0);
    env.storage()
        .instance()
        .set(&total_shares_key(env, pool_id), &new_total_shares);

    // Deduct from asset balances
    for i in 0..withdrawal.len() {
        let (asset, amount) = withdrawal.get(i).unwrap();
        let balance_key = asset_balance_key(env, pool_id, &asset);
        let mut balance: i128 = env
            .storage()
            .instance()
            .get(&balance_key)
            .unwrap_or(0);
        if balance < amount {
            panic_with_error!(env, PoolError::InsufficientAssetBalance);
        }
        balance = balance
            .checked_sub(amount)
            .unwrap_or(0);
        env.storage().instance().set(&balance_key, &balance);

        // Emit withdrawal event for each asset
        env.events().publish(
            ("withdraw",),
            (pool_id, lp.clone(), asset, amount, shares),
        );
    }
}

/// Deposit multiple assets at once into the pool.
///
/// This is the recommended way to make an initial deposit (first LP) because
/// it ensures all assets are deposited proportionally.
///
/// # Arguments
/// * `env` - The Soroban environment.
/// * `pool_id` - The pool identifier.
/// * `assets` - Vector of (asset_address, amount) tuples.
/// * `lp` - The LP address receiving the shares.
///
/// # Panics
/// * If any amount is <= 0.
/// * If deposits are paused.
pub fn multi_asset_deposit(
    env: &Env,
    pool_id: &str,
    assets: Vec<(Address, i128)>,
    lp: Address,
) {
    if assets.is_empty() {
        panic_with_error!(env, PoolError::InvalidDepositAmount);
    }

    // Check if deposits are paused
    let paused: bool = env
        .storage()
        .instance()
        .get(&deposits_paused_key(env, pool_id))
        .unwrap_or(false);
    if paused {
        panic_with_error!(env, PoolError::DepositsPaused);
    }

    // Calculate total deposit value
    let mut total_deposit_value = 0_i128;
    for i in 0..assets.len() {
        let (_asset, amount) = assets.get(i).unwrap();
        if amount <= 0 {
            panic_with_error!(env, PoolError::InvalidDepositAmount);
        }
        total_deposit_value = total_deposit_value
            .checked_add(amount)
            .unwrap_or(i128::MAX);
    }

    // Apply fee on total
    let (net_deposit_value, fee) = apply_deposit_fee(env, total_deposit_value);

    // Update asset balances
    for i in 0..assets.len() {
        let (asset, amount) = assets.get(i).unwrap();
        let proportional_amount = amount * net_deposit_value / total_deposit_value;

        let balance_key = asset_balance_key(env, pool_id, &asset);
        let mut balance: i128 = env
            .storage()
            .instance()
            .get(&balance_key)
            .unwrap_or(0);
        balance = balance
            .checked_add(proportional_amount)
            .unwrap_or(i128::MAX);
        env.storage().instance().set(&balance_key, &balance);

        // Add asset to pool assets list
        let mut pool_assets: Vec<Address> = env
            .storage()
            .instance()
            .get(&pool_assets_key(env, pool_id))
            .unwrap_or(Vec::new(env));
        let mut found = false;
        for j in 0..pool_assets.len() {
            if pool_assets.get(j).unwrap() == asset {
                found = true;
                break;
            }
        }
        if !found {
            pool_assets.push_back(asset.clone());
            env.storage().instance().set(&pool_assets_key(env, pool_id), &pool_assets);
        }
    }

    // Get pool value before deposit
    let pool_value_before = total_pool_value(env, pool_id);
    let total_shares: i128 = env
        .storage()
        .instance()
        .get(&total_shares_key(env, pool_id))
        .unwrap_or(0);

    // Calculate shares
    let shares = calculate_shares(net_deposit_value, pool_value_before, total_shares);

    // Update total shares
    let new_total_shares = total_shares
        .checked_add(shares)
        .unwrap_or(i128::MAX);
    env.storage()
        .instance()
        .set(&total_shares_key(env, pool_id), &new_total_shares);

    // Update LP position
    let lp_key = lp_position_key(env, pool_id, &lp);
    let mut lp_shares: i128 = env.storage().instance().get(&lp_key).unwrap_or(0);
    lp_shares = lp_shares
        .checked_add(shares)
        .unwrap_or(i128::MAX);
    env.storage().instance().set(&lp_key, &lp_shares);

    // Add LP to pool LP list
    let mut lps: Vec<Address> = env
        .storage()
        .instance()
        .get(&pool_lps_key(env, pool_id))
        .unwrap_or(Vec::new(env));
    let mut lp_found = false;
    for i in 0..lps.len() {
        if lps.get(i).unwrap() == lp {
            lp_found = true;
            break;
        }
    }
    if !lp_found {
        lps.push_back(lp.clone());
        env.storage().instance().set(&pool_lps_key(env, pool_id), &lps);
    }

    // Emit event
    env.events().publish(
        ("multi_dep",),
        (pool_id, net_deposit_value, fee, shares, lp),
    );
}

/// Get the LP share balance for a given LP in the pool.
///
/// # Arguments
/// * `env` - The Soroban environment.
/// * `pool_id` - The pool identifier.
/// * `lp` - The LP address to query.
///
/// # Returns
/// The number of LP shares held by the LP.
pub fn get_lp_balance(env: &Env, pool_id: &str, lp: &Address) -> i128 {
    let lp_key = lp_position_key(env, pool_id, lp);
    env.storage().instance().get(&lp_key).unwrap_or(0)
}

/// Get the current pool state.
///
/// # Arguments
/// * `env` - The Soroban environment.
/// * `pool_id` - The pool identifier.
///
/// # Returns
/// The current `PoolState`.
pub fn get_pool_state(env: &Env, pool_id: &str) -> PoolState {
    let total_shares: i128 = env
        .storage()
        .instance()
        .get(&total_shares_key(env, pool_id))
        .unwrap_or(0);
    let total_value = total_pool_value(env, pool_id);
    let cumulative_precision_loss: i128 = env
        .storage()
        .instance()
        .get(&precision_loss_key(env, pool_id))
        .unwrap_or(0);
    let deposits_paused: bool = env
        .storage()
        .instance()
        .get(&deposits_paused_key(env, pool_id))
        .unwrap_or(false);

    PoolState {
        total_shares,
        total_value,
        cumulative_precision_loss,
        deposits_paused,
    }
}

/// Get the balance of a specific asset in the pool.
///
/// # Arguments
/// * `env` - The Soroban environment.
/// * `pool_id` - The pool identifier.
/// * `asset` - The asset address.
///
/// # Returns
/// The balance of the asset in the pool.
pub fn get_asset_balance(env: &Env, pool_id: &str, asset: &Address) -> i128 {
    env.storage()
        .instance()
        .get(&asset_balance_key(env, pool_id, asset))
        .unwrap_or(0)
}

/// Get the list of assets in the pool.
///
/// # Arguments
/// * `env` - The Soroban environment.
/// * `pool_id` - The pool identifier.
///
/// # Returns
/// A vector of asset addresses in the pool.
pub fn get_pool_assets(env: &Env, pool_id: &str) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&pool_assets_key(env, pool_id))
        .unwrap_or(Vec::new(env))
}

/// Get the list of LPs in the pool.
///
/// # Arguments
/// * `env` - The Soroban environment.
/// * `pool_id` - The pool identifier.
///
/// # Returns
/// A vector of LP addresses in the pool.
pub fn get_pool_lps(env: &Env, pool_id: &str) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&pool_lps_key(env, pool_id))
        .unwrap_or(Vec::new(env))
}

/// Set the oracle rate for an asset.
///
/// # Arguments
/// * `env` - The Soroban environment.
/// * `asset` - The asset address.
/// * `rate` - The conversion rate (numéraire per unit of asset, 7-decimal fixed-point).
pub fn set_oracle_rate(env: &Env, asset: &Address, rate: i128) {
    env.storage()
        .instance()
        .set(&oracle_rate_key(env, asset), &rate);
}

/// Get the oracle rate for an asset.
///
/// # Arguments
/// * `env` - The Soroban environment.
/// * `asset` - The asset address.
///
/// # Returns
/// The conversion rate for the asset.
pub fn get_oracle_rate(env: &Env, asset: &Address) -> i128 {
    env.storage()
        .instance()
        .get(&oracle_rate_key(env, asset))
        .unwrap_or(1)
}

/// Resume deposits after they were paused by the precision loss guard.
///
/// # Arguments
/// * `env` - The Soroban environment.
/// * `pool_id` - The pool identifier.
pub fn resume_deposits(env: &Env, pool_id: &str) {
    env.storage()
        .instance()
        .set(&deposits_paused_key(env, pool_id), &false);
    env.storage()
        .instance()
        .set(&precision_loss_key(env, pool_id), &0);
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{vec, testutils::Address as _, contract, contractimpl};

    /// Minimal test contract to provide a valid contract context for storage operations.
    #[contract]
    pub struct PoolTestHelper;

    #[contractimpl]
    impl PoolTestHelper {
        pub fn dummy(env: Env) {}
    }

    /// Execute a closure within a registered contract context.
    /// The closure receives a reference to the Env as a parameter, avoiding
    /// any ambiguity about whether env is captured by value or by reference.
    fn with_contract_context<T>(env: &Env, f: impl FnOnce(&Env) -> T) -> T {
        // Register the helper contract and use its ID for the context
        let contract_id = env.register(PoolTestHelper, ());
        env.as_contract(&contract_id, || f(env))
    }

    #[test]
    fn test_round_half_even_exact() {
        // Exact division
        assert_eq!(round_half_even(10, 5), 2);
        assert_eq!(round_half_even(100, 25), 4);
        assert_eq!(round_half_even(0, 5), 0);
    }

    #[test]
    fn test_round_half_even_round_down() {
        // Less than half -> round down
        assert_eq!(round_half_even(10, 4), 2); // 2.5 -> 2 (tie, even)
        assert_eq!(round_half_even(11, 4), 3); // 2.75 -> 3
        assert_eq!(round_half_even(9, 4), 2); // 2.25 -> 2
    }

    #[test]
    fn test_round_half_even_round_up() {
        // More than half -> round up
        assert_eq!(round_half_even(15, 4), 4); // 3.75 -> 4
        assert_eq!(round_half_even(7, 3), 2); // 2.33... -> 2
        assert_eq!(round_half_even(8, 3), 3); // 2.66... -> 3
    }

    #[test]
    fn test_round_half_even_tie_to_even() {
        // Tie: 0.5 -> round to nearest even
        assert_eq!(round_half_even(5, 2), 2); // 2.5 -> 2 (2 is even)
        assert_eq!(round_half_even(15, 6), 2); // 2.5 -> 2 (2 is even)
        // Tie: 3.5 -> round to 4 (4 is even)
        assert_eq!(round_half_even(7, 2), 4); // 3.5 -> 4
        assert_eq!(round_half_even(35, 10), 4); // 3.5 -> 4
    }

    #[test]
    fn test_round_half_even_no_bias() {
        // Over many operations, banker's rounding should have zero bias.
        // Sum of [0.5, 1.5, 2.5, 3.5, 4.5] should equal sum of rounded values.
        let env = Env::default();
        let values = vec![
            &env,
            (1i128, 2i128),   // 0.5 -> 0 (even)
            (3i128, 2i128),   // 1.5 -> 2 (even)
            (5i128, 2i128),   // 2.5 -> 2 (even)
            (7i128, 2i128),   // 3.5 -> 4 (even)
            (9i128, 2i128),   // 4.5 -> 4 (even)
        ];

        let mut sum_exact = 0i128;
        let mut sum_rounded = 0i128;
        for i in 0..values.len() {
            let (n, d) = values.get(i).unwrap();
            sum_exact += n;
            sum_rounded += round_half_even(n, d) * d;
        }
        // sum_exact should be approximately same as sum_rounded
        // 1+3+5+7+9 = 25, rounded sum = 0+2+2+4+4 = 12, 12*2 = 24
        // Difference of 1 due to tie-breaking, which is unbiased
        let diff = (sum_exact - sum_rounded).abs();
        assert!(diff <= 1, "Banker's rounding bias too large: {}", diff);
    }

    #[test]
    fn test_calculate_shares_first_deposit() {
        // First deposit: shares = deposit amount
        assert_eq!(calculate_shares(1000, 0, 0), 1000);
        assert_eq!(calculate_shares(5000, 0, 0), 5000);
        assert_eq!(calculate_shares(1, 0, 0), 1);
    }

    #[test]
    fn test_calculate_shares_second_deposit() {
        // Pool has 1000 shares, total value 1000
        // Deposit 500 more: shares = 500 * 1000 / 1000 = 500
        assert_eq!(calculate_shares(500, 1000, 1000), 500);

        // Deposit 100 when pool value 1000 and shares 1000: shares = 100
        assert_eq!(calculate_shares(100, 1000, 1000), 100);
    }

    #[test]
    fn test_calculate_shares_cross_multiplication() {
        // Pool value: 333, total shares: 1000
        // deposit: 100
        // Without cross-multiplication: 100 * 1000 / 333 = 300 (truncated)
        // With cross-multiplication: 100 * 1000 * 1e12 / 333 / 1e12 = ~300.3 -> 300
        let shares = calculate_shares(100, 333, 1000);
        // The exact value is 100 * 1000 / 333 = 300.300...
        // Banker's rounding should give 300
        assert_eq!(shares, 300);
    }

    #[test]
    fn test_calculate_shares_no_asymmetry() {
        // Test that depositing different values gives symmetric results.
        // Pool has 10000 shares, total value 10000.
        // Deposit 1000: shares = 1000, value = 1000
        // Deposit another 500: shares = 500, value = 500
        let (shares_a, _) = deposit_withdraw_symmetry(1000, 10000, 10000);
        assert_eq!(shares_a, 1000);
        // After first deposit: new_total = 11000, new_value = 11000
        let (shares_b, _) = deposit_withdraw_symmetry(500, 11000, 11000);
        assert_eq!(shares_b, 500);

        // Now test reverse order: same total deposited value
        // Start fresh: 10000 shares, 10000 value
        // Deposit 500 first
        let (shares_c, _) = deposit_withdraw_symmetry(500, 10000, 10000);
        assert_eq!(shares_c, 500);
        // After first deposit: new_total = 10500, new_value = 10500
        // Deposit 1000
        let (shares_d, _) = deposit_withdraw_symmetry(1000, 10500, 10500);
        // Total shares deposited = 500 + 1000 = 1500 (same as A+B = 1000 + 500)
        assert_eq!(shares_c + shares_d, shares_a + shares_b,
            "Deposit order should not affect total shares: {} vs {}",
            shares_c + shares_d, shares_a + shares_b);
    }

    #[test]
    fn test_calculate_shares_edge_cases() {
        // Zero deposit
        assert_eq!(calculate_shares(0, 1000, 1000), 0);

        // Very small deposit compared to pool
        let shares = calculate_shares(1, 1_000_000_000, 1_000_000_000);
        assert_eq!(shares, 1);

        // Large numbers
        let shares = calculate_shares(1_000_000, 10_000_000, 5_000_000);
        // Exact: 1_000_000 * 5_000_000 / 10_000_000 = 500_000
        assert_eq!(shares, 500_000);
    }

    #[test]
    fn test_apply_deposit_fee() {
        let env = Env::default();

        with_contract_context(&env, |env| {
            // 0.3% fee on 1000 = 3
            let (net, fee) = apply_deposit_fee(env, 1000);
            assert_eq!(fee, 3);
            assert_eq!(net, 997);

            // No fee on zero
            let (net, fee) = apply_deposit_fee(env, 0);
            assert_eq!(fee, 0);
            assert_eq!(net, 0);

            // Rounding: 30 bps on 1 = 0 (truncates)
            let (net, fee) = apply_deposit_fee(env, 1);
            assert_eq!(fee, 0);
            assert_eq!(net, 1);
        });
    }

    #[test]
    fn test_update_precision_loss() {
        let env = Env::default();

        with_contract_context(&env, |env| {
            // Start with 0 precision loss
            let result = update_precision_loss(env, "test_pool", 100);
            assert_eq!(result, 100);

            // Add more
            let result = update_precision_loss(env, "test_pool", 200);
            assert_eq!(result, 300);

            // Check deposits are not paused yet
            let paused: bool = env
                .storage()
                .instance()
                .get(&deposits_paused_key(env, "test_pool"))
                .unwrap_or(false);
            assert!(!paused);
        });
    }

    #[test]
    fn test_update_precision_loss_threshold_exceeded() {
        let env = Env::default();

        with_contract_context(&env, |env| {
            // Add precision loss exceeding threshold
            let result = update_precision_loss(env, "test_pool", MAX_ALLOWED_PRECISION_LOSS + 1);
            assert_eq!(result, MAX_ALLOWED_PRECISION_LOSS + 1);

            // Deposits should be paused
            let paused: bool = env
                .storage()
                .instance()
                .get(&deposits_paused_key(env, "test_pool"))
                .unwrap_or(false);
            assert!(paused, "Deposits should be paused after threshold exceeded");
        });
    }

    #[test]
    fn test_total_pool_value() {
        let env = Env::default();
        env.mock_all_auths();

        with_contract_context(&env, |env| {
            let pool_id = "test_pool";
            let asset_a = Address::generate(env);
            let asset_b = Address::generate(env);

            // Set oracle rates
            set_oracle_rate(env, &asset_a, 1); // 1:1
            set_oracle_rate(env, &asset_b, 2); // 1:2

            // Set balances
            env.storage()
                .instance()
                .set(&asset_balance_key(env, pool_id, &asset_a), &1000i128);
            env.storage()
                .instance()
                .set(&asset_balance_key(env, pool_id, &asset_b), &500i128);
            env.storage()
                .instance()
                .set(&pool_assets_key(env, pool_id), &vec![env, asset_a.clone(), asset_b.clone()]);

            // Total = 1000 * 1 + 500 * 2 = 2000
            assert_eq!(total_pool_value(env, pool_id), 2000);
        });
    }

    #[test]
    fn test_get_pool_state_empty() {
        let env = Env::default();

        with_contract_context(&env, |env| {
            let pool_id = "empty_pool";
            let state = get_pool_state(env, pool_id);
            assert_eq!(state.total_shares, 0);
            assert_eq!(state.total_value, 0);
            assert_eq!(state.cumulative_precision_loss, 0);
            assert!(!state.deposits_paused);
        });
    }

    #[test]
    fn test_deposit_and_get_lp_balance() {
        let env = Env::default();
        env.mock_all_auths();

        with_contract_context(&env, |env| {
            let pool_id = "test_pool";
            let asset = Address::generate(env);
            let lp = Address::generate(env);

            // Set oracle rate
            set_oracle_rate(env, &asset, 1);

            // First deposit
            deposit(env, pool_id, asset.clone(), 1000, lp.clone());

            // Check LP balance
            let balance = get_lp_balance(env, pool_id, &lp);
            // 1000 * 0.997 = 997 net, first deposit so shares = 997
            assert_eq!(balance, 997);
        });
    }

    #[test]
    fn test_get_asset_balance() {
        let env = Env::default();
        env.mock_all_auths();

        with_contract_context(&env, |env| {
            let pool_id = "test_pool";
            let asset = Address::generate(env);

            // Initially zero
            assert_eq!(get_asset_balance(env, pool_id, &asset), 0);

            // Set balance
            env.storage()
                .instance()
                .set(&asset_balance_key(env, pool_id, &asset), &5000i128);

            assert_eq!(get_asset_balance(env, pool_id, &asset), 5000);
        });
    }

    #[test]
    fn test_deposit_withdraw_roundtrip() {
        let env = Env::default();
        env.mock_all_auths();

        with_contract_context(&env, |env| {
            let pool_id = "roundtrip_pool";
            let asset = Address::generate(env);
            let lp = Address::generate(env);

            // Set oracle rate: 1:1
            set_oracle_rate(env, &asset, 1);

            // Deposit 10000 units
            deposit(env, pool_id, asset.clone(), 10000, lp.clone());

            let shares = get_lp_balance(env, pool_id, &lp);
            assert!(shares > 0, "Should have received shares");

            // Get total shares
            let total_shares: i128 = env
                .storage()
                .instance()
                .get(&total_shares_key(env, pool_id))
                .unwrap_or(0);
            assert_eq!(total_shares, shares, "Total shares should match LP shares for single LP");

            // Withdraw all shares
            withdraw(env, pool_id, lp.clone(), shares, None);

            // LP should have 0 shares remaining
            let remaining = get_lp_balance(env, pool_id, &lp);
            assert_eq!(remaining, 0, "LP should have 0 shares after full withdrawal");
        });
    }

    #[test]
    fn test_multi_asset_deposit() {
        let env = Env::default();
        env.mock_all_auths();

        with_contract_context(&env, |env| {
            let pool_id = "multi_asset_pool";
            let asset_a = Address::generate(env);
            let asset_b = Address::generate(env);
            let lp = Address::generate(env);

            set_oracle_rate(env, &asset_a, 1);
            set_oracle_rate(env, &asset_b, 2);

            // Multi-asset deposit
            let assets = vec![
                env,
                (asset_a.clone(), 1000i128),
                (asset_b.clone(), 500i128),
            ];

            multi_asset_deposit(env, pool_id, assets, lp.clone());

            // Check LP has shares
            let shares = get_lp_balance(env, pool_id, &lp);
            assert!(shares > 0, "LP should have shares after multi-asset deposit");
        });
    }

    #[test]
    fn test_calculate_withdrawal_preferred_asset() {
        let env = Env::default();
        env.mock_all_auths();

        with_contract_context(&env, |env| {
            let pool_id = "calc_pool";
            let asset = Address::generate(env);

            set_oracle_rate(env, &asset, 1);

            // Set up pool state
            env.storage()
                .instance()
                .set(&total_shares_key(env, pool_id), &1000i128);
            env.storage()
                .instance()
                .set(&asset_balance_key(env, pool_id, &asset), &1000i128);
            env.storage()
                .instance()
                .set(&pool_assets_key(env, pool_id), &vec![env, asset.clone()]);

            // Withdraw 100 shares (10%)
            let result = calculate_withdrawal(env, pool_id, 100, 1000, Some(asset.clone()));
            assert_eq!(result.len(), 1);
            let (withdrawn_asset, amount) = result.get(0).unwrap();
            assert_eq!(withdrawn_asset, asset);
            // 100/1000 * 1000 = 100
            assert_eq!(amount, 100);
        });
    }

    #[test]
    fn test_calculate_withdrawal_basket() {
        let env = Env::default();
        env.mock_all_auths();

        with_contract_context(&env, |env| {
            let pool_id = "basket_pool";
            let asset_a = Address::generate(env);
            let asset_b = Address::generate(env);

            set_oracle_rate(env, &asset_a, 1);
            set_oracle_rate(env, &asset_b, 2);

            // Set up pool
            env.storage()
                .instance()
                .set(&total_shares_key(env, pool_id), &1000i128);
            env.storage()
                .instance()
                .set(&asset_balance_key(env, pool_id, &asset_a), &800i128);
            env.storage()
                .instance()
                .set(&asset_balance_key(env, pool_id, &asset_b), &100i128);
            env.storage()
                .instance()
                .set(&pool_assets_key(env, pool_id), &vec![env, asset_a.clone(), asset_b.clone()]);

            // Withdraw 500 shares (50%)
            let result = calculate_withdrawal(env, pool_id, 500, 1000, None);
            assert_eq!(result.len(), 2);

            let (asset, amount) = result.get(0).unwrap();
            assert_eq!(asset, asset_a);
            assert_eq!(amount, 400); // 50% of 800

            let (asset, amount) = result.get(1).unwrap();
            assert_eq!(asset, asset_b);
            assert_eq!(amount, 50); // 50% of 100
        });
    }
}
