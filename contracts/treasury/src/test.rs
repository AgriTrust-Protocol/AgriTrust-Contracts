extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, Vec,
};

fn setup(
    env: &Env,
) -> (
    TreasuryClient<'_>,
    token::Client<'_>,
    Address,
    Address,
    Address,
) {
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Treasury);
    let client = TreasuryClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let recipient = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(env, &token_id);
    let token_admin_client = token::StellarAssetClient::new(env, &token_id);
    token_admin_client.mint(&contract_id, &10_000_000_000);

    let mut owners = Vec::new(env);
    for _ in 0..5 {
        owners.push_back(Address::generate(env));
    }
    let mut council = Vec::new(env);
    for _ in 0..3 {
        council.push_back(Address::generate(env));
    }
    client.initialize(&admin, &token_id, &owners, &council);
    (client, token_client, admin, recipient, token_id)
}

#[test]
fn three_streams_accrue_for_seven_days() {
    let env = Env::default();
    let (client, token_client, admin, recipient, token_id) = setup(&env);
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let seven_days = 7 * 24 * 60 * 60;
    client.create_stream(
        &admin,
        &token_id,
        &recipient,
        &1,
        &1_000,
        &(1_000 + seven_days + 100),
    );
    client.create_stream(
        &admin,
        &token_id,
        &recipient,
        &2,
        &1_000,
        &(1_000 + seven_days + 100),
    );
    client.create_stream(
        &admin,
        &token_id,
        &recipient,
        &3,
        &1_000,
        &(1_000 + seven_days + 100),
    );

    env.ledger().with_mut(|l| l.timestamp = 1_000 + seven_days);
    let first = client.withdraw_accrued(&recipient, &1);
    let second = client.withdraw_accrued(&recipient, &2);
    let third = client.withdraw_accrued(&recipient, &3);

    assert_eq!(first, seven_days as i128);
    assert_eq!(second, (seven_days * 2) as i128);
    assert_eq!(third, (seven_days * 3) as i128);
    assert_eq!(token_client.balance(&recipient), (seven_days * 6) as i128);
}

#[test]
fn cliff_vesting_blocks_until_cliff_then_linear() {
    let env = Env::default();
    let (client, _token_client, admin, recipient, token_id) = setup(&env);
    env.ledger().with_mut(|l| l.timestamp = 10);
    let id = client.create_vesting(&admin, &token_id, &recipient, &1_000, &100, &1_000, &10);

    let schedule = client.get_vestings(&recipient).get(0).unwrap();
    env.ledger().with_mut(|l| l.timestamp = 109);
    assert_eq!(client.vested_amount(&schedule), 0);

    env.ledger().with_mut(|l| l.timestamp = 510);
    assert_eq!(client.withdraw_vested(&recipient, &id), 500);
}
