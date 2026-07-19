extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Env, String,
};

fn setup() -> (
    Env,
    CropFuturesMarketClient<'static>,
    Address,
    Address,
    Address,
    Address,
    u64,
) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CropFuturesMarket, ());
    let client = CropFuturesMarketClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let farmer = Address::generate(&env);
    let buyer = Address::generate(&env);
    let liquidator = Address::generate(&env);
    client.initialize(&admin);
    client.deposit_stable(&farmer, &20_000);
    client.deposit_stable(&buyer, &200_000);
    let future_id = client.create_future(
        &admin,
        &String::from_str(&env, "corn"),
        &String::from_str(&env, "A"),
        &String::from_str(&env, "iowa"),
        &100,
        &1_000,
    );
    (env, client, admin, farmer, buyer, liquidator, future_id)
}

#[test]
fn create_future_trade_price_crash_and_liquidate_with_bonus() {
    let (env, client, admin, farmer, buyer, liquidator, future_id) = setup();

    let position = client.mint_short(&farmer, &future_id, &100);
    assert_eq!(client.balance(&farmer, &future_id), 100);
    assert_eq!(client.stable_balance(&farmer), 10_000);

    client.add_liquidity(&farmer, &future_id, &500, &2_000, &0, &100);
    client.trade(&buyer, &future_id, &25);
    assert_eq!(client.balance(&buyer, &future_id), 25);

    client.update_oracle_twap(&admin, &future_id, &1_200, &ORACLE_TWAP_WINDOW_SECONDS);
    let bonus = client.liquidate(&liquidator, &position);
    assert_eq!(bonus, 500);
    assert_eq!(client.stable_balance(&liquidator), 500);
    assert!(!client.positions().get(position).unwrap().open);
    drop(env);
}

#[test]
fn physical_delivery_requires_matching_provenance_and_cash_settlement_penalizes() {
    let (env, client, admin, farmer, buyer, _liquidator, future_id) = setup();
    client.mint_short(&farmer, &future_id, &10);
    client.trade(&buyer, &future_id, &5);
    env.ledger().set_sequence_number(101);

    client.mint_provenance_nft(
        &admin,
        &7,
        &ProvenanceNFT {
            owner: buyer.clone(),
            future_id,
            crop: String::from_str(&env, "corn"),
            grade: String::from_str(&env, "A"),
            region: String::from_str(&env, "iowa"),
            tons: 2,
            burned: false,
        },
    );
    client.claim_physical(&buyer, &future_id, &2, &7);
    assert_eq!(client.balance(&buyer, &future_id), 3);
    let payout = client.cash_settle(&buyer, &future_id, &3);
    assert_eq!(payout, 2_940);
}
