#![no_std]

pub mod constant_product_amm;
pub mod dynamic_fee;
pub mod leverage_module;
pub mod liquidity_pool;
pub mod settlement_oracle;

use constant_product_amm::{buy_cost, sell_return, validate_outcomes};
use liquidity_pool::LiquidityState;
use settlement_oracle::{assert_timelock_elapsed, report, OracleReport};
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Market {
    pub admin: Address,
    pub oracle: Address,
    pub outcomes: u32,
    pub bucket_upper_bounds_bps: Vec<u32>,
    pub reserves: Vec<i128>,
    pub resolved_outcome: u32,
    pub settled: bool,
    pub has_report: bool,
    pub oracle_report: OracleReport,
    pub liquidity: LiquidityState,
}

#[contracttype]
pub enum DataKey {
    Market,
}

#[contract]
pub struct PredictionMarket;

#[contractimpl]
impl PredictionMarket {
    pub fn initialize(
        env: Env,
        admin: Address,
        oracle: Address,
        bucket_upper_bounds_bps: Vec<u32>,
        seed_liquidity: i128,
    ) {
        admin.require_auth();
        validate_outcomes(bucket_upper_bounds_bps.len());
        if seed_liquidity <= 0 {
            panic!("invalid seed liquidity");
        }
        let mut reserves = Vec::new(&env);
        for _ in 0..bucket_upper_bounds_bps.len() {
            reserves.push_back(seed_liquidity);
        }
        let market = Market {
            admin: admin.clone(),
            oracle,
            outcomes: bucket_upper_bounds_bps.len(),
            bucket_upper_bounds_bps,
            reserves,
            resolved_outcome: 0,
            settled: false,
            has_report: false,
            oracle_report: OracleReport {
                final_yield_bps: 0,
                published_timestamp: 0,
            },
            liquidity: LiquidityState::new(&env),
        };
        env.storage().instance().set(&DataKey::Market, &market);
    }

    pub fn add_liquidity(env: Env, provider: Address, amount_per_outcome: i128) -> i128 {
        let mut market = get_market(&env);
        let minted = market
            .liquidity
            .deposit(provider, amount_per_outcome, market.outcomes);
        for i in 0..market.reserves.len() {
            market
                .reserves
                .set(i, market.reserves.get(i).unwrap() + amount_per_outcome);
        }
        put_market(&env, &market);
        minted
    }

    pub fn buy(
        env: Env,
        outcome: u32,
        amount: i128,
        max_cost: i128,
        price_changes_bps: Vec<i128>,
    ) -> i128 {
        let mut market = get_open_market(&env);
        let fee_bps = dynamic_fee::fee_bps(&env, &price_changes_bps);
        let cost = buy_cost(&market.reserves, outcome, amount, fee_bps);
        if cost > max_cost {
            panic!("slippage");
        }
        let no_fee = buy_cost(&market.reserves, outcome, amount, 0);
        market.liquidity.accrue_fee(cost - no_fee);
        market
            .reserves
            .set(outcome, market.reserves.get(outcome).unwrap() - amount);
        put_market(&env, &market);
        cost
    }

    pub fn sell(
        env: Env,
        outcome: u32,
        amount: i128,
        min_return: i128,
        price_changes_bps: Vec<i128>,
    ) -> i128 {
        let mut market = get_open_market(&env);
        let fee_bps = dynamic_fee::fee_bps(&env, &price_changes_bps);
        let returned = sell_return(&market.reserves, outcome, amount, fee_bps);
        if returned < min_return {
            panic!("slippage");
        }
        let no_fee = sell_return(&market.reserves, outcome, amount, 0);
        market.liquidity.accrue_fee(no_fee - returned);
        market
            .reserves
            .set(outcome, market.reserves.get(outcome).unwrap() + amount);
        put_market(&env, &market);
        returned
    }

    pub fn publish_yield(env: Env, final_yield_bps: u32) {
        let mut market = get_open_market(&env);
        market.oracle_report = report(&env, &market.oracle, final_yield_bps);
        market.has_report = true;
        put_market(&env, &market);
    }

    pub fn settle(env: Env) -> u32 {
        let mut market = get_open_market(&env);
        if !market.has_report {
            panic!("missing oracle report");
        }
        let oracle_report = market.oracle_report.clone();
        assert_timelock_elapsed(&env, &oracle_report);
        market.resolved_outcome = bucket_for(
            &market.bucket_upper_bounds_bps,
            oracle_report.final_yield_bps,
        );
        market.settled = true;
        let outcome = market.resolved_outcome;
        put_market(&env, &market);
        outcome
    }

    pub fn payout_per_share(env: Env, outcome: u32) -> i128 {
        let market = get_market(&env);
        if !market.settled {
            panic!("not settled");
        }
        if outcome == market.resolved_outcome {
            1_000_000
        } else {
            0
        }
    }

    pub fn market(env: Env) -> Market {
        get_market(&env)
    }
}

fn bucket_for(bounds: &Vec<u32>, value: u32) -> u32 {
    for i in 0..bounds.len() {
        if value < bounds.get(i).unwrap() {
            return i;
        }
    }
    bounds.len() - 1
}

fn get_market(env: &Env) -> Market {
    env.storage()
        .instance()
        .get(&DataKey::Market)
        .expect("not initialized")
}
fn get_open_market(env: &Env) -> Market {
    let market = get_market(env);
    if market.settled {
        panic!("settled");
    }
    market
}
fn put_market(env: &Env, market: &Market) {
    env.storage().instance().set(&DataKey::Market, market);
}

#[cfg(test)]
mod test;
