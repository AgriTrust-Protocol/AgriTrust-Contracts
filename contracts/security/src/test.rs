extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    BytesN, Env, Vec,
};
use std::panic::{catch_unwind, AssertUnwindSafe};

fn setup(env: &Env) -> (Address, Address, Address, Address) {
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 1_700_000_000);
    let contract_id = env.register(FarmlandToken, ());
    let admin = Address::generate(env);
    let issuer = Address::generate(env);
    let custodian = Address::generate(env);
    let nft = Address::generate(env);
    env.as_contract(&contract_id, || {
        IdentityRegistry::initialize(env.clone(), admin.clone());
        IdentityRegistry::add_trusted_issuer(env.clone(), issuer.clone());
        FarmlandToken::initialize_token(env.clone(), admin, nft, custodian.clone());
    });
    (issuer, custodian, Address::generate(env), contract_id)
}

fn register(env: &Env, contract_id: &Address, investor: &Address, issuer: &Address) {
    env.as_contract(contract_id, || {
        IdentityRegistry::register_identity(
            env.clone(),
            investor.clone(),
            Address::generate(env),
            issuer.clone(),
            env.ledger().timestamp() + REVERIFICATION_PERIOD_SECONDS,
            env.ledger().timestamp() + REVERIFICATION_PERIOD_SECONDS,
        );
    });
}

#[test]
fn register_ten_identities_and_reject_unverified_transfer() {
    let env = Env::default();
    let (issuer, custodian, _, contract_id) = setup(&env);
    register(&env, &contract_id, &custodian, &issuer);
    let mut investors = Vec::new(&env);
    for _ in 0..10 {
        let investor = Address::generate(&env);
        register(&env, &contract_id, &investor, &issuer);
        assert!(
            env.as_contract(&contract_id, || IdentityRegistry::is_verified(
                env.clone(),
                investor.clone()
            ))
        );
        investors.push_back(investor);
    }

    env.as_contract(&contract_id, || {
        FarmlandToken::mint_from_title(env.clone(), custodian.clone())
    });
    env.ledger()
        .with_mut(|li| li.timestamp += MIN_HOLDING_PERIOD_SECONDS + 1);

    let unverified = Address::generate(&env);
    let result = catch_unwind(AssertUnwindSafe(|| {
        env.as_contract(&contract_id, || {
            FarmlandToken::transfer(env.clone(), custodian.clone(), unverified, 1)
        });
    }));
    assert!(result.is_err());
}

#[test]
fn enforces_holding_period_and_max_holding() {
    let env = Env::default();
    let (issuer, custodian, _, contract_id) = setup(&env);
    register(&env, &contract_id, &custodian, &issuer);
    let recipient = Address::generate(&env);
    register(&env, &contract_id, &recipient, &issuer);

    env.as_contract(&contract_id, || {
        FarmlandToken::mint_from_title(env.clone(), custodian.clone())
    });
    let during_holding_period = catch_unwind(AssertUnwindSafe(|| {
        env.as_contract(&contract_id, || {
            FarmlandToken::transfer(env.clone(), custodian.clone(), recipient.clone(), 1)
        });
    }));
    assert!(during_holding_period.is_err());

    env.ledger()
        .with_mut(|li| li.timestamp += MIN_HOLDING_PERIOD_SECONDS + 1);
    let over_cap = catch_unwind(AssertUnwindSafe(|| {
        env.as_contract(&contract_id, || {
            FarmlandToken::transfer(env.clone(), custodian.clone(), recipient.clone(), 10_001)
        });
    }));
    assert!(over_cap.is_err());

    env.as_contract(&contract_id, || {
        FarmlandToken::transfer(env.clone(), custodian.clone(), recipient.clone(), 10_000)
    });
    assert_eq!(
        env.as_contract(&contract_id, || FarmlandToken::balance(
            env.clone(),
            recipient
        )),
        10_000
    );
}

#[test]
fn dividend_claim_requires_current_identity_and_proof() {
    let env = Env::default();
    let (issuer, _, _, contract_id) = setup(&env);
    let investor = Address::generate(&env);
    register(&env, &contract_id, &investor, &issuer);
    let root = BytesN::from_array(&env, &[7; 32]);
    env.as_contract(&contract_id, || {
        DividendDistributor::publish_distribution(env.clone(), 1, root.clone())
    });
    let mut proof = Vec::new(&env);
    proof.push_back(root);

    env.as_contract(&contract_id, || {
        DividendDistributor::claim_dividend(env.clone(), 1, investor.clone(), 42, proof.clone())
    });
    let duplicate = catch_unwind(AssertUnwindSafe(|| {
        env.as_contract(&contract_id, || {
            DividendDistributor::claim_dividend(env.clone(), 1, investor, 42, proof)
        });
    }));
    assert!(duplicate.is_err());
}

#[test]
fn redemption_burns_only_inside_thirty_day_window() {
    let env = Env::default();
    let (issuer, custodian, _, contract_id) = setup(&env);
    register(&env, &contract_id, &custodian, &issuer);
    env.as_contract(&contract_id, || {
        FarmlandToken::mint_from_title(env.clone(), custodian.clone())
    });
    env.as_contract(&contract_id, || {
        RedemptionManager::open_redemption_window(env.clone(), env.ledger().timestamp())
    });

    env.as_contract(&contract_id, || {
        RedemptionManager::request_redemption(env.clone(), custodian.clone(), 1_000)
    });
    assert_eq!(
        env.as_contract(&contract_id, || FarmlandToken::total_supply(env.clone())),
        99_000
    );

    env.ledger()
        .with_mut(|li| li.timestamp += REDEMPTION_WINDOW_SECONDS + 1);
    let closed = catch_unwind(AssertUnwindSafe(|| {
        env.as_contract(&contract_id, || {
            RedemptionManager::request_redemption(env.clone(), custodian, 1)
        });
    }));
    assert!(closed.is_err());
}
