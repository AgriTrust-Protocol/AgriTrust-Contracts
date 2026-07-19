extern crate std;

use super::*;
use soroban_sdk::{testutils::Address as _, token, Address, Env, String};

fn risk(env: &Env, farmer: &Address, coverage: i128, nonce: u64) -> RiskScore {
    RiskScore {
        farmer: farmer.clone(),
        coverage_amount: coverage,
        crop_type: String::from_str(env, "maize"),
        region: String::from_str(env, "north"),
        historical_yield_variance_bps: 500,
        risk_multiplier_bps: 12_000,
        valid_until_ledger: env.ledger().sequence() + 100,
        nonce,
    }
}

#[test]
fn deposit_100k_write_50_policies_and_drought_pays_claims() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let aave = Address::generate(&env);
    let liquidity_provider = Address::generate(&env);
    let reinsurer_funder = Address::generate(&env);

    let token_contract = env.register_stellar_asset_contract_v2(admin.clone());
    let token_id = token_contract.address();
    let token_admin = token::StellarAssetClient::new(&env, &token_id);

    let reinsurance_id = env.register(ReinsuranceModule, ());
    let pool_id = env.register(CropInsurancePool, ());
    let reinsurance = ReinsuranceModuleClient::new(&env, &reinsurance_id);
    let pool = CropInsurancePoolClient::new(&env, &pool_id);

    reinsurance.init_reinsurance(&admin, &token_id, &pool_id);
    pool.initialize(&admin, &token_id, &oracle, &reinsurance_id, &aave, &500u32);

    token_admin.mint(&liquidity_provider, &100_000);
    token_admin.mint(&reinsurer_funder, &50_000);
    pool.deposit(&liquidity_provider, &100_000);
    reinsurance.fund(&reinsurer_funder, &50_000);

    let mut farmers = soroban_sdk::Vec::new(&env);
    for i in 0..50u64 {
        let farmer = Address::generate(&env);
        token_admin.mint(&farmer, &1_000);
        let score = risk(&env, &farmer, 1_000, i);
        let quoted = pool.quote_premium(&score);
        assert_eq!(quoted, 60);
        let policy_id = pool.pay_premium(&farmer, &score, &8_000u32);
        assert_eq!(policy_id, i + 1);
        farmers.push_back(farmer);
    }

    assert_eq!(pool.get_policies().len(), 50);
    assert_eq!(pool.get_outstanding_risk(), 50_000);
    assert!(pool.get_idle_deposited() > 0);

    let paid = pool.report_yield_and_payout(
        &oracle,
        &String::from_str(&env, "maize"),
        &String::from_str(&env, "north"),
        &4_000u32,
    );
    assert_eq!(paid, 25_000);
    assert_eq!(pool.get_outstanding_risk(), 0);
    assert_eq!(reinsurance.get_fund_balance(), 45_600);

    for i in 0..farmers.len() {
        let farmer = farmers.get(i).unwrap();
        assert_eq!(token::Client::new(&env, &token_id).balance(&farmer), 1_440);
    }
}

#[test]
#[should_panic(expected = "capital ratio below 110%")]
fn rejects_policy_when_capital_ratio_would_drop_below_minimum() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let farmer = Address::generate(&env);
    let funder = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(admin.clone());
    let token_id = token_contract.address();
    let token_admin = token::StellarAssetClient::new(&env, &token_id);
    let reinsurance_id = env.register(ReinsuranceModule, ());
    let pool_id = env.register(CropInsurancePool, ());
    let pool = CropInsurancePoolClient::new(&env, &pool_id);
    let reinsurance = ReinsuranceModuleClient::new(&env, &reinsurance_id);
    reinsurance.init_reinsurance(&admin, &token_id, &pool_id);
    pool.initialize(
        &admin,
        &token_id,
        &oracle,
        &reinsurance_id,
        &Address::generate(&env),
        &500u32,
    );
    token_admin.mint(&funder, &10_000);
    token_admin.mint(&farmer, &1_000);
    pool.deposit(&funder, &10_000);
    pool.pay_premium(&farmer, &risk(&env, &farmer, 10_000, 1), &8_000u32);
}

#[test]
fn parametric_oracle_median_triggers_drought_payout_and_dispute_slashes() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let farmer = Address::generate(&env);
    let funder = Address::generate(&env);
    let aave = Address::generate(&env);
    let legacy_oracle = Address::generate(&env);

    let token_contract = env.register_stellar_asset_contract_v2(admin.clone());
    let token_id = token_contract.address();
    let token_admin = token::StellarAssetClient::new(&env, &token_id);
    let reinsurance_id = env.register(ReinsuranceModule, ());
    let pool_id = env.register(CropInsurancePool, ());
    let reinsurance = ReinsuranceModuleClient::new(&env, &reinsurance_id);
    let pool = CropInsurancePoolClient::new(&env, &pool_id);

    reinsurance.init_reinsurance(&admin, &token_id, &pool_id);
    pool.initialize(
        &admin,
        &token_id,
        &legacy_oracle,
        &reinsurance_id,
        &aave,
        &500u32,
    );

    token_admin.mint(&funder, &20_000);
    token_admin.mint(&farmer, &2_000);
    pool.deposit(&funder, &20_000);

    let mut oracles = soroban_sdk::Vec::new(&env);
    for _ in 0..5 {
        let oracle = Address::generate(&env);
        token_admin.mint(&oracle, &1_000);
        pool.bond_oracle(&oracle, &500);
        oracles.push_back(oracle);
    }

    let mut trigger = soroban_sdk::Vec::new(&env);
    trigger.push_back(TriggerNode::Condition(
        WeatherMetric::Rainfall30d,
        Comparator::Lt,
        50,
    ));
    trigger.push_back(TriggerNode::Condition(
        WeatherMetric::AvgTemp,
        Comparator::Gt,
        35,
    ));
    trigger.push_back(TriggerNode::And(0, 1));

    let policy_id = pool.create_parametric_policy(
        &farmer,
        &77u64,
        &2026u64,
        &String::from_str(&env, "maize"),
        &1_000,
        &100,
        &500,
        &trigger,
        &2u32,
        &oracles,
    );

    pool.submit_weather_report(&policy_id, &oracles.get(0).unwrap(), &40, &36);
    pool.submit_weather_report(&policy_id, &oracles.get(1).unwrap(), &42, &37);
    pool.submit_weather_report(&policy_id, &oracles.get(2).unwrap(), &41, &36);
    pool.submit_weather_report(&policy_id, &oracles.get(3).unwrap(), &200, &20);
    pool.submit_weather_report(&policy_id, &oracles.get(4).unwrap(), &39, &38);

    let aggregated = pool.aggregate_weather(&policy_id);
    assert_eq!(aggregated.rainfall_30d_mm, 41);
    assert_eq!(aggregated.avg_temp_c, 37);
    assert_eq!(aggregated.accepted_reports, 4);

    let payout = pool.claim_parametric_payout(&farmer, &policy_id);
    assert_eq!(payout, 900);
    assert_eq!(token::Client::new(&env, &token_id).balance(&farmer), 2_400);

    let disputer = Address::generate(&env);
    token_admin.mint(&disputer, &1_000);
    let dispute_id = pool.submit_dispute(&disputer, &policy_id, &oracles.get(3).unwrap());
    pool.resolve_dispute(&admin, &dispute_id, &true);
    assert_eq!(
        token::Client::new(&env, &token_id).balance(&disputer),
        1_090
    );
}
