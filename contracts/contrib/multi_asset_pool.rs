use soroban_sdk::{Env, Address, panic, Symbol, IntoVal, Val};
use std::collections::HashMap;

/// Simulation precision used by the pro-rata share cross-multiplication to
/// minimize intermediate rounding before the final truncation back to whole units.
const SIMULATION_PRECISION: i128 = 1_000_000_000_000; // 1e12

#[derive(Clone)]
pub struct GrantPool {
    pub pool_id: String,
    pub balances: HashMap<Address, i128>, // Map of asset address → balance
    pub oracle: Address,                  // Oracle for price conversions
}

/// Banker's rounding: round to nearest, ties to even.
pub fn round_half_even(numerator: i128, denominator: i128) -> i128 {
    if denominator == 0 {
        panic!("round_half_even: division by zero");
    }
    let q = numerator / denominator;
    let r = numerator % denominator;
    let twice = r * 2;
    if twice < denominator {
        q // round down
    } else if twice > denominator {
        q + 1 // round up
    } else {
        // tie: round to even
        if q % 2 == 0 { q } else { q + 1 }
    }
}

/// Pro-rata share of `amount` for an asset holding `balance`, given pool `total_value`.
/// Uses cross-multiplication with SIMULATION_PRECISION to minimize intermediate rounding.
pub fn pro_rata_share(balance: i128, amount: i128, total_value: i128) -> i128 {
    let scaled = balance * amount * SIMULATION_PRECISION;
    let divided = round_half_even(scaled, total_value);
    divided / SIMULATION_PRECISION
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

        for (asset, bal) in pool.balances.iter_mut() {
            let share = pro_rata_share(*bal, amount, total_value);
            *bal -= share;
            emit_withdrawal_event(env, pool_id.clone(), asset.clone(), grantee.clone(), share);
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

    #[test]
    fn test_pro_rata_symmetry() {
        // Two-asset pool, pro-rata split of amount 1500.
        let total_value = 3000i128;
        let bal_a = 1000i128;
        let bal_b = 2000i128;
        let amt = 1500i128;
        let share_a = pro_rata_share(bal_a, amt, total_value);
        let share_b = pro_rata_share(bal_b, amt, total_value);
        assert_eq!(share_a + share_b, amt, "pro-rata shares must sum to the withdrawn amount");
        // round_half_even tie-to-even: 5/2 = 2 (even), 7/2 = 4 (even? 3 is odd -> 4)
        assert_eq!(round_half_even(5, 2), 2);
        assert_eq!(round_half_even(7, 2), 4);
        assert_eq!(round_half_even(6, 2), 3);
    }
}
