extern crate std;

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{vec, Address, Env};

#[test]
fn four_outcome_market_trades_and_settles() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(PredictionMarket, ());
    let client = PredictionMarketClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    client.initialize(
        &admin,
        &oracle,
        &vec![&env, 2_000, 4_000, 6_000, u32::MAX],
        &10_000,
    );
    let provider = Address::generate(&env);
    assert_eq!(client.add_liquidity(&provider, &1_000), 4_000);

    let changes = vec![&env, 10_i128, -20_i128, 30_i128, -40_i128];
    let cost = client.buy(&2, &1_000, &2_000, &changes);
    assert!(cost > 0 && cost <= 2_000);
    let returned = client.sell(&2, &100, &1, &changes);
    assert!(returned > 0);

    client.publish_yield(&4_500);
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + settlement_oracle::SETTLEMENT_TIMELOCK_SECONDS);
    assert_eq!(client.settle(), 2);
    assert_eq!(client.payout_per_share(&2), 1_000_000);
    assert_eq!(client.payout_per_share(&1), 0);
}

#[test]
#[should_panic(expected = "settlement timelock active")]
fn settlement_requires_timelock() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(PredictionMarket, ());
    let client = PredictionMarketClient::new(&env, &contract_id);
    client.initialize(
        &Address::generate(&env),
        &Address::generate(&env),
        &vec![&env, 2_000, 4_000, u32::MAX],
        &10_000,
    );
    client.publish_yield(&2_500);
    client.settle();
}
