#![cfg(test)]

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _},
    token, Address, Env, Map, String, Vec,
};

use crate::{GrantContract, GrantContractClient, GrantStatus};

const DAY: u64 = 24 * 60 * 60;

fn set_timestamp(env: &Env, timestamp: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = timestamp;
    });
}

fn build_grantees(env: &Env, grantee: &Address) -> Map<Address, u32> {
    let mut grantees = Map::new(env);
    grantees.set(grantee.clone(), 10_000);
    grantees
}

fn build_council(env: &Env, members: &[Address]) -> Vec<Address> {
    let mut council = Vec::new(env);
    for member in members {
        council.push_back(member.clone());
    }
    council
}

fn setup_token(env: &Env, admin: &Address, amount: i128) -> Address {
    let token_address = env.register_stellar_asset_contract(admin.clone());
    token::StellarAssetClient::new(env, &token_address).mint(admin, &amount);
    token_address
}

#[test]
fn milestone_speed_bonus_doubles_flow_for_30_days() {
    let env = Env::default();
    env.mock_all_auths();
    set_timestamp(&env, 0);

    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let token_address = setup_token(&env, &admin, 1_000_000);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id = symbol_short!("gbonus1");
    let milestone_id = symbol_short!("mile1");
    client.create_grant(
        &grant_id,
        &admin,
        &build_grantees(&env, &grantee),
        &1_000_000u128,
        &token_address,
        &0u64,
        &build_council(&env, &[admin.clone()]),
        &1u32,
    );
    client.configure_stream(&grant_id, &0u64, &(100 * DAY));
    client.add_milestone(
        &grant_id,
        &milestone_id,
        &1_000_000u128,
        &String::from_str(&env, "Milestone 1"),
        &(40 * DAY),
    );
    client.configure_milestone_acceleration(&grant_id, &milestone_id, &10_000u32, &(30 * DAY));
    client.approve_milestone(&grant_id, &milestone_id);

    set_timestamp(&env, 15 * DAY);
    assert_eq!(
        client.get_withdrawable_amount(&grant_id, &grantee),
        300_000u128
    );

    let withdrawn = client.withdraw(&grant_id, &grantee);
    assert_eq!(withdrawn, 300_000u128);
    assert_eq!(
        token::Client::new(&env, &token_address).balance(&grantee),
        300_000i128
    );

    set_timestamp(&env, 40 * DAY);
    assert_eq!(
        client.get_withdrawable_amount(&grant_id, &grantee),
        400_000u128
    );

    set_timestamp(&env, 100 * DAY);
    assert_eq!(
        client.get_withdrawable_amount(&grant_id, &grantee),
        700_000u128
    );
}

#[test]
fn speed_bonus_never_exceeds_released_milestone_funding() {
    let env = Env::default();
    env.mock_all_auths();
    set_timestamp(&env, 0);

    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);
    let token_address = setup_token(&env, &admin, 1_000_000);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id = symbol_short!("grantcap");
    let milestone_id = symbol_short!("cap1");
    client.create_grant(
        &grant_id,
        &admin,
        &build_grantees(&env, &grantee),
        &1_000_000u128,
        &token_address,
        &0u64,
        &build_council(&env, &[admin.clone()]),
        &1u32,
    );
    client.configure_stream(&grant_id, &0u64, &(100 * DAY));
    client.add_milestone(
        &grant_id,
        &milestone_id,
        &200_000u128,
        &String::from_str(&env, "Seed funding"),
        &(10 * DAY),
    );
    client.configure_milestone_acceleration(&grant_id, &milestone_id, &10_000u32, &(30 * DAY));
    client.approve_milestone(&grant_id, &milestone_id);

    set_timestamp(&env, 30 * DAY);
    assert_eq!(
        client.get_withdrawable_amount(&grant_id, &grantee),
        200_000u128
    );
}

#[test]
fn council_threshold_controls_when_acceleration_starts() {
    let env = Env::default();
    env.mock_all_auths();
    set_timestamp(&env, 0);

    let admin = Address::generate(&env);
    let reviewer = Address::generate(&env);
    let grantee = Address::generate(&env);
    let token_address = setup_token(&env, &admin, 1_000_000);

    let contract_id = env.register_contract(None, GrantContract);
    let client = GrantContractClient::new(&env, &contract_id);

    let grant_id = symbol_short!("grantvote");
    let milestone_id = symbol_short!("vote1");
    client.create_grant(
        &grant_id,
        &admin,
        &build_grantees(&env, &grantee),
        &1_000_000u128,
        &token_address,
        &0u64,
        &build_council(&env, &[admin.clone(), reviewer.clone()]),
        &2u32,
    );
    client.configure_stream(&grant_id, &0u64, &(100 * DAY));
    client.add_milestone(
        &grant_id,
        &milestone_id,
        &1_000_000u128,
        &String::from_str(&env, "Council gated"),
        &(20 * DAY),
    );
    client.configure_milestone_acceleration(&grant_id, &milestone_id, &5_000u32, &(30 * DAY));

    client.vote_milestone(&grant_id, &milestone_id, &admin, &true);
    let milestone = client.get_milestone(&grant_id, &milestone_id);
    assert_eq!(milestone.votes_for, 1);
    assert!(!milestone.approved);
    assert_eq!(client.get_grant(&grant_id).released_amount, 0u128);

    set_timestamp(&env, 5 * DAY);
    client.vote_milestone(&grant_id, &milestone_id, &reviewer, &true);

    let grant = client.get_grant(&grant_id);
    assert_eq!(grant.released_amount, 1_000_000u128);
    assert_eq!(grant.acceleration_windows.len(), 1);
    assert_eq!(grant.status, GrantStatus::Completed);

    set_timestamp(&env, 15 * DAY);
    assert_eq!(
        client.get_withdrawable_amount(&grant_id, &grantee),
        200_000u128
    );
}
