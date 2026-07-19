#![cfg(test)]

use crate::{
    ArbitrationContract, ArbitrationContractClient, DataKey, Ruling, APPEAL_WINDOW_SECONDS,
    TTL_EXTENSION_PERIOD, VOTING_PERIOD_SECONDS,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token,
    xdr::ToXdr,
    Address, Env,
};

/// Number of ledgers to extend contract instances to by default.
const INSTANCE_TTL: u32 = 1_000_000;

fn setup_test(env: &Env) -> (Address, Address, Address, ArbitrationContractClient<'_>) {
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_addr = env.register_stellar_asset_contract(token_admin.clone());
    let client_token = token::StellarAssetClient::new(env, &token_addr);
    client_token.mint(&admin, &1_000_000_000);

    let contract_id = env.register(ArbitrationContract, ());
    let client = ArbitrationContractClient::new(env, &contract_id);

    client.init(&admin, &token_addr);

    // Extend token contract instance TTL so it survives any ledger advances
    env.as_contract(&token_addr, || {
        env.storage().instance().extend_ttl(0, INSTANCE_TTL);
    });

    (admin, token_addr, contract_id, client)
}

fn set_ledger(env: &Env, sequence: u32, timestamp: u64) {
    env.ledger().with_mut(|li| {
        li.sequence_number = sequence;
        li.timestamp = timestamp;
    });
}

fn advance_ledgers(env: &Env, count: u32, seconds_per_ledger: u64) {
    let cur_seq = env.ledger().sequence();
    let cur_ts = env.ledger().timestamp();
    env.ledger().with_mut(|li| {
        li.sequence_number = cur_seq + count;
        li.timestamp = cur_ts + count as u64 * seconds_per_ledger;
    });
}

#[test]
fn test_token_weighted_jury_dispute_flow_rewards_and_escrow() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger(&env, 1000, 1_700_000_000);

    let (_admin, token_addr, contract_id, client) = setup_test(&env);
    let plaintiff = Address::generate(&env);
    let defendant = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let pub_key = soroban_sdk::BytesN::from_array(&env, &[9u8; 32]);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_addr);
    token_admin_client.mint(&plaintiff, &1_000);

    let mut jurors = soroban_sdk::Vec::new(&env);
    for i in 0..11u8 {
        let juror = Address::generate(&env);
        token_admin_client.mint(&juror, &100);
        client.opt_in_juror(&juror, &100);
        assert_eq!(client.juror_stake(&juror), 100);
        if i < 11 {
            jurors.push_back(juror);
        }
    }

    let dispute_id =
        client.raise_dispute(&77, &plaintiff, &defendant, &1_000, &arbitrator, &pub_key);
    let evidence_hash = soroban_sdk::BytesN::from_array(&env, &[3u8; 32]);
    client.submit_evidence(&dispute_id, &plaintiff, &evidence_hash);
    client.select_jury(&dispute_id, &0);
    assert_eq!(client.get_selected_jurors(&dispute_id, &0).len(), 11);

    let salt = soroban_sdk::BytesN::from_array(&env, &[4u8; 32]);
    for i in 0..11u32 {
        let juror = jurors.get(i).unwrap();
        let ruling = if i < 7 {
            Ruling::PlaintiffWin
        } else {
            Ruling::DefendantWin
        };
        let commitment = env
            .crypto()
            .keccak256(&(ruling.clone(), salt.clone()).to_xdr(&env))
            .to_bytes();
        client.commit_vote(&dispute_id, &juror, &commitment);
        client.reveal_vote(&dispute_id, &juror, &ruling, &salt);
    }

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_TTL, INSTANCE_TTL);
    });
    env.as_contract(&token_addr, || {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_TTL, INSTANCE_TTL);
    });
    env.ledger().with_mut(|li| {
        li.timestamp += VOTING_PERIOD_SECONDS + 1;
    });
    assert_eq!(client.tally_ruling(&dispute_id), Ruling::PlaintiffWin);

    for i in 0..11u32 {
        let juror = jurors.get(i).unwrap();
        if i < 7 {
            assert_eq!(client.juror_stake(&juror), 101);
        } else {
            assert_eq!(client.juror_stake(&juror), 98);
        }
    }

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_TTL, INSTANCE_TTL);
    });
    env.as_contract(&token_addr, || {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_TTL, INSTANCE_TTL);
    });
    env.ledger().with_mut(|li| {
        li.timestamp += APPEAL_WINDOW_SECONDS + 1;
    });
    client.finalize(&dispute_id);
    let real_token = token::Client::new(&env, &token_addr);
    assert_eq!(real_token.balance(&plaintiff), 1_000);
    assert_eq!(real_token.balance(&defendant), 0);
    assert_eq!(
        client.get_dispute(&dispute_id).status,
        crate::DisputeStatus::Final
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit test: basic lock → release flow
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_lock_and_release_settlement() {
    let env = Env::default();
    env.mock_all_auths();

    let (_admin, token_addr, _contract_id, client) = setup_test(&env);

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);

    let cycle = 1u32;
    let arbitration_id = 42u32;
    let amount: i128 = 100_000_000_000;

    set_ledger(&env, 1000, 1_700_000_000);

    let token_admin_client = token::StellarAssetClient::new(&env, &token_addr);
    token_admin_client.mint(&buyer, &amount);

    client.lock_settlement(&cycle, &buyer, &seller, &arbitration_id, &amount);

    // Verify lock entry
    let lock = client.get_escrow_lock(&cycle).unwrap();
    assert_eq!(lock.buyer, buyer);
    assert_eq!(lock.seller, seller);
    assert_eq!(lock.arbitration_id, arbitration_id);
    assert_eq!(lock.amount, amount);
    assert_eq!(lock.locked_at, 1_700_000_000);

    // Verify TtlDeadline was emitted
    let deadline = client.get_escrow_ttl_deadline(&cycle).unwrap();
    assert_eq!(deadline.ledger_sequence, 1000);
    assert_eq!(deadline.ttl_extension_period, TTL_EXTENSION_PERIOD);

    // Advance ledger time
    advance_ledgers(&env, 100, 5);

    // Release settlement
    client.release_settlement(&cycle, &buyer, &seller, &arbitration_id, &amount);

    // Verify release entry
    let release = client.get_escrow_release(&cycle).unwrap();
    assert_eq!(release.buyer, buyer);
    assert_eq!(release.seller, seller);
    assert_eq!(release.arbitration_id, arbitration_id);
    assert_eq!(release.amount, amount);
    assert!(release.released_at > 1_700_000_000);
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit test: TTL synchronization extends entries with short TTL
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_synchronize_escrow_ttl_extends_short_ttl_entries() {
    let env = Env::default();
    env.mock_all_auths();

    let (_admin, _token_addr, contract_id, client) = setup_test(&env);

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);

    let cycle = 1u32;
    let arbitration_id = 42u32;
    let amount: i128 = 100_000_000_000;

    set_ledger(&env, 1000, 1_700_000_000);

    // Write a lock entry via raw storage with a moderate TTL
    let lock = crate::EscrowLockData {
        buyer: buyer.clone(),
        seller: seller.clone(),
        arbitration_id,
        amount,
        locked_at: 1_700_000_000,
    };
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::EscrowLock(cycle), &lock);
        // Extended TTL (100000 ledgers from now)
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::EscrowLock(cycle), 0, 100_000);

        // Write a release entry with a very short TTL (500 ledgers)
        let release = crate::EscrowReleaseData {
            buyer: buyer.clone(),
            seller: seller.clone(),
            arbitration_id,
            amount,
            released_at: 1_700_000_000,
        };
        env.storage()
            .persistent()
            .set(&DataKey::EscrowRelease(cycle), &release);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::EscrowRelease(cycle), 0, 500);
    });

    // Advance past the release's short TTL
    advance_ledgers(&env, 400, 5);

    // Release now has ~100 ledgers remaining, call synchronize
    client.synchronize_escrow_ttl(&cycle);

    // Verify release entry survived the synchronize
    let release_after = client.get_escrow_release(&cycle);
    assert!(
        release_after.is_some(),
        "release entry should exist after synchronize extended its TTL"
    );

    // Verify lock entry also survived
    let lock_after = client.get_escrow_lock(&cycle);
    assert!(
        lock_after.is_some(),
        "lock entry should exist after synchronize"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Integration test: delay release by 7 days (simulated via ledger fast-forward)
// and confirm TTL extension fires correctly during release_settlement
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_delayed_release_ttl_extension() {
    let env = Env::default();
    env.mock_all_auths();

    // Start at a high ledger so all contracts get elevated baseline TTL
    set_ledger(&env, 500_000, 1_700_000_000);

    let (_admin, token_addr, _contract_id, client) = setup_test(&env);

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);

    let cycle = 1u32;
    let arbitration_id = 42u32;
    let amount: i128 = 100_000_000_000;

    let token_admin_client = token::StellarAssetClient::new(&env, &token_addr);
    token_admin_client.mint(&buyer, &amount);

    // Lock settlement
    client.lock_settlement(&cycle, &buyer, &seller, &arbitration_id, &amount);

    // Extend token contract instance again before the long advance,
    // using the same INSTANCE_TTL we use in setup_test
    env.as_contract(&token_addr, || {
        env.storage().instance().extend_ttl(0, INSTANCE_TTL);
    });

    // Simulate 7 days delay (7 * 24 * 60 * 60 / 5 = 120960 ledgers)
    let delay_ledgers = 120_960u32;
    advance_ledgers(&env, delay_ledgers, 5);

    // Release settlement — calls synchronize_escrow_ttl internally
    client.release_settlement(&cycle, &buyer, &seller, &arbitration_id, &amount);

    // Both lock and release entries should exist after 7 days
    let lock = client.get_escrow_lock(&cycle);
    assert!(
        lock.is_some(),
        "lock should exist after 7 day delay + release"
    );

    let release = client.get_escrow_release(&cycle).unwrap();
    assert_eq!(release.amount, amount);
    assert_eq!(release.seller, seller);

    // Advance another 7 days (total 14 days from lock)
    let further_advance = 120_960u32;
    advance_ledgers(&env, further_advance, 5);

    // After ~14 days total, entries should still exist
    let lock_after = client.get_escrow_lock(&cycle);
    assert!(
        lock_after.is_some(),
        "lock should still exist ~14 days after lock (bumped during release)"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit test: garbage_collect_expired_escrows
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_garbage_collect_expired_escrows() {
    let env = Env::default();
    env.mock_all_auths();

    let (_admin, _token_addr, contract_id, client) = setup_test(&env);

    // Use raw storage with very short TTLs to set up expired cycles
    set_ledger(&env, 1000, 1_700_000_000);

    // Create two cycles' worth of TtlDeadline entries with short TTLs
    for cycle in 0..2 {
        let deadline = crate::TtlDeadline {
            ledger_sequence: 1000,
            ttl_extension_period: TTL_EXTENSION_PERIOD,
        };
        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .set(&DataKey::EscrowTtlDeadline(cycle), &deadline);
            // Very short TTL (50 ledgers)
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::EscrowTtlDeadline(cycle), 0, 50);
        });
    }

    // Set cycle counter
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::EscrowCycleCounter, &2u32);
    });

    // Verify both TtlDeadlines were written
    assert!(client.get_escrow_ttl_deadline(&0).is_some());
    assert!(client.get_escrow_ttl_deadline(&1).is_some());

    // Advance past the short TTL of the TtlDeadline entries
    advance_ledgers(&env, 100, 5);

    // At this point both cycles have no lock or release entries,
    // and the TtlDeadline entries have expired. GC should clean them.
    let cleaned = client.garbage_collect_expired_escrows(&10);
    assert_eq!(cleaned, 2, "should have cleaned both expired cycles");

    // TtlDeadline entries should be removed
    assert!(client.get_escrow_ttl_deadline(&0).is_none());
    assert!(client.get_escrow_ttl_deadline(&1).is_none());
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit test: lock settlement rejects invalid release params
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_release_arbitration_id_mismatch() {
    let env = Env::default();
    env.mock_all_auths();

    let (_admin, token_addr, _contract_id, client) = setup_test(&env);

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);

    set_ledger(&env, 1000, 1_700_000_000);

    let token_admin_client = token::StellarAssetClient::new(&env, &token_addr);
    token_admin_client.mint(&buyer, &100_000_000_000);

    client.lock_settlement(&1u32, &buyer, &seller, &42u32, &100_000_000_000);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.release_settlement(&1u32, &buyer, &seller, &99u32, &100_000_000_000);
    }));
    assert!(
        result.is_err(),
        "release with mismatched arbitration_id should panic"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit test: release amount cannot exceed lock amount
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_release_amount_exceeds_lock() {
    let env = Env::default();
    env.mock_all_auths();

    let (_admin, token_addr, _contract_id, client) = setup_test(&env);

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);

    set_ledger(&env, 1000, 1_700_000_000);

    let token_admin_client = token::StellarAssetClient::new(&env, &token_addr);
    token_admin_client.mint(&buyer, &200_000_000_000);

    client.lock_settlement(&1u32, &buyer, &seller, &42u32, &100_000_000_000);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.release_settlement(&1u32, &buyer, &seller, &42u32, &200_000_000_000);
    }));
    assert!(
        result.is_err(),
        "release with amount > lock amount should panic"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Legacy test: basic dispute flow still works after adding settlement module
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_successful_settlement() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let funder = Address::generate(&env);
    let grantee = Address::generate(&env);

    let arbitrator_pub_key = soroban_sdk::BytesN::from_array(&env, &[7u8; 32]);
    let arbitrator = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract(token_admin);
    let token_client = token::StellarAssetClient::new(&env, &token_addr);
    token_client.mint(&funder, &1000);

    let contract_id = env.register_contract(None, ArbitrationContract);
    let client = ArbitrationContractClient::new(&env, &contract_id);

    client.init(&admin, &token_addr);
    let dispute_id = client.raise_dispute(
        &1,
        &funder,
        &grantee,
        &1000,
        &arbitrator,
        &arbitrator_pub_key,
    );

    client.resolve_dispute(&dispute_id, &500, &500);

    let real_token = token::Client::new(&env, &token_addr);
    assert_eq!(real_token.balance(&funder), 500);
    assert_eq!(real_token.balance(&grantee), 500);
}
