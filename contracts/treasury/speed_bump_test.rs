#![cfg(test)]

use crate::speed_bump::{
    BatchReleaseResult, ReleaseItem, SpeedBumpContract, SpeedBumpContractClient, SPEED_BUMP_DELAY,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, Vec,
};

fn setup_speed_bump_test(
    env: &Env,
) -> (
    Address,
    Address,
    Address,
    Address,
    SpeedBumpContractClient<'_>,
) {
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_addr = token_sac.address();
    let token_client = token::StellarAssetClient::new(env, &token_addr);

    let escrow = Address::generate(env);
    let contract_id = env.register(SpeedBumpContract, ());
    let client = SpeedBumpContractClient::new(env, &contract_id);

    client.initialize(&admin, &token_addr, &escrow);

    // Fund the speed bump contract with tokens
    token_client.mint(&contract_id, &100_000_000_000_000); // 10,000,000 XLM

    (admin, token_addr, escrow, contract_id, client)
}

fn set_ledger_seq(env: &Env, sequence: u32) {
    env.ledger().with_mut(|li| {
        li.sequence_number = sequence;
    });
}

fn advance_ledgers(env: &Env, count: u32) {
    let cur = env.ledger().sequence();
    env.ledger().with_mut(|li| {
        li.sequence_number = cur + count;
    });
}

#[test]
fn test_single_release_below_threshold_executes_immediately() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_seq(&env, 100);

    let (admin, token_addr, _escrow, _contract_id, client) = setup_speed_bump_test(&env);
    let token = token::Client::new(&env, &token_addr);

    let caller = Address::generate(&env);
    let recipient = Address::generate(&env);

    // Whitelist caller
    client.add_allowed_caller(&admin, &caller);
    assert!(client.is_allowed_caller(&caller));

    // Release 500 XLM (below 1,000 XLM threshold)
    let amount = 5_000_000_000i128;
    let immediate = client.release(&caller, &recipient, &amount);
    assert!(immediate);

    assert_eq!(token.balance(&recipient), amount);
    assert_eq!(client.get_cumulative_released(), amount);
    assert_eq!(client.get_pending_releases().len(), 0);
}

#[test]
fn test_single_release_above_threshold_requires_delay() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_seq(&env, 100);

    let (admin, token_addr, _escrow, _contract_id, client) = setup_speed_bump_test(&env);
    let token = token::Client::new(&env, &token_addr);

    let caller = Address::generate(&env);
    let recipient = Address::generate(&env);

    client.add_allowed_caller(&admin, &caller);

    // Release 2,000 XLM (exceeds 1,000 XLM threshold)
    let amount = 20_000_000_000i128;
    let immediate = client.release(&caller, &recipient, &amount);
    assert!(!immediate); // Enforced delay

    // Token has not moved yet
    assert_eq!(token.balance(&recipient), 0);
    assert_eq!(client.get_cumulative_released(), 0);

    let pending = client.get_pending_releases();
    assert_eq!(pending.len(), 1);
    let rel = pending.get(0).unwrap();
    assert_eq!(rel.amount, amount);
    assert_eq!(rel.recipient, recipient);
    assert_eq!(rel.release_at, 100 + SPEED_BUMP_DELAY);
    assert!(!rel.executed);
    assert!(!rel.vetoed);
}

#[test]
#[should_panic(expected = "Speed bump active")]
fn test_delayed_release_cannot_execute_before_delay() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_seq(&env, 100);

    let (admin, _token_addr, _escrow, _contract_id, client) = setup_speed_bump_test(&env);
    let caller = Address::generate(&env);
    let recipient = Address::generate(&env);

    client.add_allowed_caller(&admin, &caller);

    let amount = 15_000_000_000i128; // > 1,000 XLM
    client.release(&caller, &recipient, &amount);

    let rel_id = client.get_pending_releases().get(0).unwrap().id;

    // Advance by only 500 ledgers (delay is 1440)
    advance_ledgers(&env, 500);

    // Should panic: speed bump active
    client.execute_pending_release(&caller, &rel_id);
}

#[test]
fn test_delayed_release_executes_after_delay() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_seq(&env, 100);

    let (admin, token_addr, _escrow, _contract_id, client) = setup_speed_bump_test(&env);
    let token = token::Client::new(&env, &token_addr);

    let caller = Address::generate(&env);
    let recipient = Address::generate(&env);

    client.add_allowed_caller(&admin, &caller);

    let amount = 15_000_000_000i128;
    client.release(&caller, &recipient, &amount);

    let rel_id = client.get_pending_releases().get(0).unwrap().id;

    // Advance ledgers past delay (1440 ledgers)
    advance_ledgers(&env, SPEED_BUMP_DELAY + 1);

    client.execute_pending_release(&caller, &rel_id);

    assert_eq!(token.balance(&recipient), amount);
    let rel = client.get_pending_release(&rel_id).unwrap();
    assert!(rel.executed);
}

#[test]
#[should_panic(expected = "Release has been vetoed")]
fn test_admin_can_veto_pending_release() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_seq(&env, 100);

    let (admin, _token_addr, _escrow, _contract_id, client) = setup_speed_bump_test(&env);
    let caller = Address::generate(&env);
    let recipient = Address::generate(&env);

    client.add_allowed_caller(&admin, &caller);

    let amount = 15_000_000_000i128;
    client.release(&caller, &recipient, &amount);

    let rel_id = client.get_pending_releases().get(0).unwrap().id;

    // Admin vetoes
    client.veto_release(&admin, &rel_id);
    let rel = client.get_pending_release(&rel_id).unwrap();
    assert!(rel.vetoed);

    // Advance past delay
    advance_ledgers(&env, SPEED_BUMP_DELAY + 10);

    // Attempt to execute should panic with vetoed error
    client.execute_pending_release(&caller, &rel_id);
}

#[test]
fn test_cross_contract_bypass_detection_unauthorized_caller() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_seq(&env, 100);

    let (_admin, token_addr, _escrow, _contract_id, client) = setup_speed_bump_test(&env);
    let token = token::Client::new(&env, &token_addr);

    let unauthorized_caller = Address::generate(&env);
    let recipient = Address::generate(&env);

    // Do NOT whitelist unauthorized_caller.
    // Even for tiny 10 XLM amount, speed bump MUST trigger!
    let tiny_amount = 100_000_000i128; // 10 XLM
    let immediate = client.release(&unauthorized_caller, &recipient, &tiny_amount);

    // Bypassed denied!
    assert!(!immediate);
    assert_eq!(token.balance(&recipient), 0);

    let pending = client.get_pending_releases();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending.get(0).unwrap().amount, tiny_amount);
}

#[test]
fn test_batch_release_per_item_individual_check() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_seq(&env, 100);

    let (admin, token_addr, _escrow, _contract_id, client) = setup_speed_bump_test(&env);
    let token = token::Client::new(&env, &token_addr);

    let caller = Address::generate(&env);
    client.add_allowed_caller(&admin, &caller);

    let r1 = Address::generate(&env);
    let r2 = Address::generate(&env);
    let r3 = Address::generate(&env);

    let mut batch = Vec::<ReleaseItem>::new(&env);
    // Item 1: 200 XLM (under threshold)
    batch.push_back(ReleaseItem {
        id: 1,
        recipient: r1.clone(),
        amount: 2_000_000_000,
    });
    // Item 2: 1,500 XLM (exceeds threshold individually)
    batch.push_back(ReleaseItem {
        id: 2,
        recipient: r2.clone(),
        amount: 15_000_000_000,
    });
    // Item 3: 300 XLM (under threshold: 200 + 300 = 500 <= 1000)
    batch.push_back(ReleaseItem {
        id: 3,
        recipient: r3.clone(),
        amount: 3_000_000_000,
    });

    let result: BatchReleaseResult = client.batch_release(&caller, &batch);

    // Item 1 & 3 execute immediately; Item 2 skipped and queued as pending
    assert_eq!(result.executed_count, 2);
    assert_eq!(result.pending_count, 1);
    assert_eq!(result.total_executed_amount, 5_000_000_000);
    assert_eq!(result.total_pending_amount, 15_000_000_000);

    assert_eq!(token.balance(&r1), 2_000_000_000);
    assert_eq!(token.balance(&r2), 0); // delayed!
    assert_eq!(token.balance(&r3), 3_000_000_000);

    let pending = client.get_pending_releases();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending.get(0).unwrap().id, 2);
}

#[test]
fn test_batch_20_releases_cumulative_tracking_blueprint_requirement() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_seq(&env, 100);

    let (admin, token_addr, _escrow, _contract_id, client) = setup_speed_bump_test(&env);
    let token = token::Client::new(&env, &token_addr);

    let caller = Address::generate(&env);
    client.add_allowed_caller(&admin, &caller);

    // 20 releases of 100 XLM each (1_000_000_000 stroops each, 2,000 XLM total)
    let item_amount = 1_000_000_000i128; // 100 XLM
    let mut batch = Vec::<ReleaseItem>::new(&env);
    let mut recipients = Vec::<Address>::new(&env);

    for i in 1..=20u64 {
        let rec = Address::generate(&env);
        recipients.push_back(rec.clone());
        batch.push_back(ReleaseItem {
            id: i,
            recipient: rec,
            amount: item_amount,
        });
    }

    let result: BatchReleaseResult = client.batch_release(&caller, &batch);

    // SPEED_BUMP_THRESHOLD is 1000 XLM (10 items of 100 XLM).
    // The first 10 items pass individually and cumulative amount reaches 1,000 XLM.
    // The subsequent 10 items exceed cumulative threshold and are safely held for delay!
    assert_eq!(result.executed_count, 10);
    assert_eq!(result.pending_count, 10);
    assert_eq!(result.total_executed_amount, 10_000_000_000); // 1,000 XLM
    assert_eq!(result.total_pending_amount, 10_000_000_000); // 1,000 XLM

    // Verify first 10 recipients received funds
    for i in 0..10 {
        let rec = recipients.get(i).unwrap();
        assert_eq!(token.balance(&rec), item_amount);
    }

    // Verify last 10 recipients did not receive immediate funds (held in speed bump)
    for i in 10..20 {
        let rec = recipients.get(i).unwrap();
        assert_eq!(token.balance(&rec), 0);
    }

    // Confirm pending queue has exactly 10 releases
    let pending = client.get_pending_releases();
    assert_eq!(pending.len(), 10);

    // Now advance ledger past delay
    advance_ledgers(&env, SPEED_BUMP_DELAY + 1);

    // Reset cumulative window so executions proceed
    client.reset_cumulative_window(&admin);

    // Execute pending items
    for i in 0..pending.len() {
        let rel_id = pending.get(i).unwrap().id;
        client.execute_pending_release(&caller, &rel_id);
    }

    // Now all 20 have received funds
    for i in 0..20 {
        let rec = recipients.get(i).unwrap();
        assert_eq!(token.balance(&rec), item_amount);
    }
}
