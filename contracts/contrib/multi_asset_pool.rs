use soroban_sdk::{Env, Address, panic, Symbol, IntoVal, Val};
use std::collections::HashMap;

#[derive(Clone)]
pub struct GrantPool {
    pub pool_id: String,
    pub balances: HashMap<Address, i128>, // Map of asset address → balance
    pub oracle: Address,                  // Oracle for price conversions
}

pub fn emit_deposit_event(env: &Env, pool_id: String, asset: Address, operator: Address, amount: i128) {
    let topics = (
        Symbol::new(env, "deposit"),
        pool_id,
        asset,
        operator,
    );
    let timestamp = env.ledger().timestamp();
    let data = (amount, timestamp, Option::<Val>::None);
    env.events().publish(topics, data);
}

pub fn emit_withdrawal_event(env: &Env, pool_id: String, asset: Address, operator: Address, amount: i128) {
    let topics = (
        Symbol::new(env, "withdraw"),
        pool_id,
        asset,
        operator,
    );
    let timestamp = env.ledger().timestamp();
    let data = (amount, timestamp, Option::<Val>::None);
    env.events().publish(topics, data);
}

pub fn emit_rebalance_event(env: &Env, pool_id: String, asset: Address, operator: Address, amount: i128) {
    let topics = (
        Symbol::new(env, "rebalance"),
        pool_id,
        asset,
        operator,
    );
    let timestamp = env.ledger().timestamp();
    let data = (amount, timestamp, Option::<Val>::None);
    env.events().publish(topics, data);
}

pub fn deposit(env: &Env, pool_id: String, asset: Address, amount: i128) {
    let mut pool: GrantPool = env.storage().get(&format!("pool:{}", pool_id))
        .unwrap_or(GrantPool {
            pool_id: pool_id.clone(),
            balances: HashMap::new(),
            oracle: Address::random(env), // placeholder
        });

    let entry = pool.balances.entry(asset.clone()).or_insert(0);
    *entry += amount;

    env.storage().set(&format!("pool:{}", pool_id), &pool);

    let operator = env.invoker();
    emit_deposit_event(env, pool_id, asset, operator, amount);
}

pub fn withdraw(env: &Env, pool_id: String, grantee: Address, amount: i128, preferred_asset: Option<Address>) {
    let mut pool: GrantPool = env.storage().get(&format!("pool:{}", pool_id))
        .unwrap_or_else(|| panic!("Pool not found"));

    if amount <= 0 {
        panic!("Withdrawal amount must be positive");
    }

    if let Some(asset) = preferred_asset {
        // Single asset withdrawal based on oracle conversion
        let converted_amount = convert_via_oracle(env, &pool.oracle, amount, &asset);
        let balance = pool.balances.get_mut(&asset).unwrap_or_else(|| panic!("Asset not in pool"));
        if *balance < converted_amount {
            panic!("Insufficient asset balance in pool");
        }
        *balance -= converted_amount;

        emit_withdrawal_event(env, pool_id, asset, grantee, converted_amount);
    } else {
        // Basket withdrawal: pro-rata across all assets
        let total_value = total_pool_value(env, &pool);
        if total_value < amount {
            panic!("Insufficient pool value");
        }

        // Split `amount` across assets proportional to each balance. A naive
        // `(*bal * amount) / total_value` truncation per asset destroys value
        // (sum of truncated shares < amount), which is the integer-division
        // asymmetry this issue fixes. Distribute the lost remainder to the
        // largest-fraction assets so the sum of shares equals `amount` exactly
        // and deposit/withdraw is symmetric.
        let balances: Vec<(Address, i128)> = pool
            .balances
            .iter()
            .map(|(a, b)| (a.clone(), *b))
            .collect();
        let shares = split_pro_rata(&balances, total_value, amount);

        for ((asset, bal), share) in balances.iter().zip(shares.iter()) {
            let mut entry = pool.balances.get_mut(asset).unwrap();
            *entry -= *share;
            emit_withdrawal_event(env, pool_id.clone(), asset.clone(), grantee.clone(), *share);
        }
    }

    env.storage().set(&format!("pool:{}", pool_id), &pool);
}

fn convert_via_oracle(env: &Env, oracle: &Address, amount: i128, asset: &Address) -> i128 {
    // Simplified: fetch conversion rate from oracle
    let rate: i128 = env.storage().get(&format!("oracle:{}:rate", asset)).unwrap_or(1);
    amount * rate
}

fn total_pool_value(env: &Env, pool: &GrantPool) -> i128 {
    let mut total = 0;
    for (asset, bal) in pool.balances.iter() {
        let rate: i128 = env.storage().get(&format!("oracle:{}:rate", asset)).unwrap_or(1);
        total += bal * rate;
    }
    total
}

/// Split `amount` across `weights` proportional to each weight, using
/// `total_weight` as the denominator. Uses truncating division per bucket, then
/// distributes the leftover (the sum of truncated fractions) to the buckets with
/// the largest remainders, so the returned shares sum to exactly `amount`.
///
/// This is pure (no `Env`) so it can be unit-tested and verified in isolation
/// without the Soroban toolchain. It eliminates the truncation asymmetry where a
/// naive per-asset `weight * amount / total` loses value on every withdrawal.
pub fn split_pro_rata(weights: &[(Address, i128)], total_weight: i128, amount: i128) -> Vec<i128> {
    let n = weights.len();
    let mut shares: Vec<i128> = vec![0; n];
    if total_weight <= 0 || n == 0 {
        return shares;
    }

    let mut distributed: i128 = 0;
    // (remainder, index) so we can hand out the leftover to largest remainders.
    let mut remainders: Vec<(i128, usize)> = Vec::with_capacity(n);
    for (i, (_, w)) in weights.iter().enumerate() {
        let prod = w * amount;
        let q = prod / total_weight;
        let r = prod - q * total_weight; // = prod % total_weight (non-negative)
        shares[i] = q;
        distributed += q;
        remainders.push((r, i));
    }

    // Hand the leftover (amount - sum of truncated shares) to the largest
    // remainders, one share unit each, until the full `amount` is allocated.
    let mut leftover = amount - distributed;
    remainders.sort_by(|a, b| b.0.cmp(&a.0));
    let mut idx = 0;
    while leftover > 0 {
        let (_, i) = remainders[idx % n];
        shares[i] += 1;
        leftover -= 1;
        idx += 1;
    }
    shares
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{Env, Address};

    #[test]
    fn test_event_topic_limit() {
        let env = Env::default();
        let pool_id = String::from("test_pool");
        let asset = Address::random(&env);
        let operator = Address::random(&env);
        let amount = 1000i128;

        emit_deposit_event(&env, pool_id.clone(), asset.clone(), operator.clone(), amount);
        emit_withdrawal_event(&env, pool_id.clone(), asset.clone(), operator.clone(), amount);
        emit_rebalance_event(&env, pool_id.clone(), asset.clone(), operator.clone(), amount);

        let events = env.events().all();
        assert_eq!(events.len(), 3);

        for event in events.iter() {
            assert!(event.topics.len() <= 4);
        }
    }
}
