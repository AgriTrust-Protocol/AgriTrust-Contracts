#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Bytes, Env, Map,
};

fn create_revealed_bid(
    env: &Env,
    grantee: &Address,
    amount: u64,
    min_bid: u64,
    salt_val: u8,
) -> RevealedBid {
    let mut milestone_costs = Map::new(env);
    milestone_costs.set(1, amount / 2);
    milestone_costs.set(2, amount / 2);

    let mut salt = Bytes::new(env);
    salt.push_back(salt_val);

    RevealedBid {
        grantee: grantee.clone(),
        amount,
        min_bid,
        milestone_costs,
        salt,
        position_nonce: 100,
    }
}

#[test]
fn test_happy_path_auction_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    env.ledger().set_sequence_number(1000);

    let contract_id = env.register(CommitRevealContract, ());
    let client = CommitRevealContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let bidder1 = Address::generate(&env);
    let bidder2 = Address::generate(&env);

    // 1. Open bidding: commit_window = 100 ledgers, reveal_window = 100 ledgers, grace = 20 ledgers
    client.open_bidding(&admin, &100, &100, &20);
    let config = client.get_config();
    assert_eq!(config.commit_deadline, 1100);
    assert_eq!(config.reveal_deadline, 1200);
    assert_eq!(config.phase_grace, 20);

    // 2. Bidders commit in Phase 1 (at sequence 1050)
    env.ledger().set_sequence_number(1050);

    let bid1 = create_revealed_bid(&env, &bidder1, 5000, 1000, 1);
    let commit1 = client.hash_bid(&bid1);
    client.commit(&bidder1, &commit1);

    let bid2 = create_revealed_bid(&env, &bidder2, 4200, 1000, 2);
    let commit2 = client.hash_bid(&bid2);
    client.commit(&bidder2, &commit2);

    assert_eq!(client.get_commitment(&bidder1), commit1);
    assert_eq!(client.get_commitment(&bidder2), commit2);

    // 3. Move to reveal phase (sequence 1120)
    env.ledger().set_sequence_number(1120);

    client.reveal(&bidder1, &bid1);
    client.reveal(&bidder2, &bid2);

    let revealed1 = client.get_revealed_bid(&bidder1);
    assert_eq!(revealed1.amount, 5000);

    // 4. Move past reveal deadline (sequence 1230 > 1200 + 20) and finalize
    env.ledger().set_sequence_number(1230);
    let winner = client.finalize_auction(&admin);
    assert!(winner.is_some());
    assert_eq!(winner.unwrap().grantee, bidder2); // Lower bid 4200 wins
}

#[test]
fn test_clock_drift_grace_window_allows_delayed_commits() {
    let env = Env::default();
    env.mock_all_auths();

    env.ledger().set_sequence_number(1000);

    let contract_id = env.register(CommitRevealContract, ());
    let client = CommitRevealContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let bidder = Address::generate(&env);

    // commit_deadline = 1100, grace = 30 -> effective commit limit = 1130
    client.open_bidding(&admin, &100, &100, &30);

    // Congestion causes sequence to reach 1115 (> commit_deadline 1100, but < 1130)
    env.ledger().set_sequence_number(1115);

    let bid = create_revealed_bid(&env, &bidder, 3000, 500, 42);
    let commit_hash = client.hash_bid(&bid);

    // Allowed under clock drift grace window
    client.commit(&bidder, &commit_hash);
    assert_eq!(client.get_commitment(&bidder), commit_hash);
}

#[test]
#[should_panic(expected = "Commit phase deadline exceeded")]
fn test_commit_rejected_past_grace_window() {
    let env = Env::default();
    env.mock_all_auths();

    env.ledger().set_sequence_number(1000);

    let contract_id = env.register(CommitRevealContract, ());
    let client = CommitRevealContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let bidder = Address::generate(&env);

    // commit_deadline = 1100, grace = 20 -> limit = 1120
    client.open_bidding(&admin, &100, &100, &20);

    // Sequence reaches 1125 (> 1120 limit)
    env.ledger().set_sequence_number(1125);

    let bid = create_revealed_bid(&env, &bidder, 3000, 500, 42);
    let commit_hash = client.hash_bid(&bid);
    client.commit(&bidder, &commit_hash);
}

#[test]
fn test_early_reveal_allowance() {
    let env = Env::default();
    env.mock_all_auths();

    env.ledger().set_sequence_number(1000);

    let contract_id = env.register(CommitRevealContract, ());
    let client = CommitRevealContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let bidder = Address::generate(&env);

    client.open_bidding(&admin, &100, &100, &20);

    let bid = create_revealed_bid(&env, &bidder, 2500, 500, 10);
    let commit_hash = client.hash_bid(&bid);
    client.commit(&bidder, &commit_hash);

    // Right at commit_deadline 1100 (while grace overlap is still live)
    env.ledger().set_sequence_number(1100);

    // Early reveal succeeds
    client.reveal(&bidder, &bid);
    assert_eq!(client.get_revealed_bid(&bidder).amount, 2500);
}

#[test]
fn test_drift_compensation_vote_extends_reveal_window() {
    let env = Env::default();
    env.mock_all_auths();

    env.ledger().set_sequence_number(1000);

    let contract_id = env.register(CommitRevealContract, ());
    let client = CommitRevealContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let bidder1 = Address::generate(&env);
    let bidder2 = Address::generate(&env);

    // commit = 100, reveal = 100, grace = 20
    // commit_deadline = 1100, reveal_deadline = 1200
    client.open_bidding(&admin, &100, &100, &20);
    client.set_phase_clock_drift(&admin, &200); // Admin sets drift compensation = 200 ledgers

    let bid1 = create_revealed_bid(&env, &bidder1, 6000, 1000, 1);
    let bid2 = create_revealed_bid(&env, &bidder2, 4500, 1000, 2);

    client.commit(&bidder1, &client.hash_bid(&bid1));
    client.commit(&bidder2, &client.hash_bid(&bid2));

    // Severe congestion desync: sequence passes reveal_deadline + grace (1200 + 20 = 1220)
    // Sequence is 1250!
    env.ledger().set_sequence_number(1250);

    // Bidders vote for drift compensation
    assert_eq!(client.is_drift_compensation_active(), false);
    client.drift_compensation_vote(&bidder1);
    assert_eq!(client.is_drift_compensation_active(), true);

    // Now reveal window is extended by phase_clock_drift (1200 + 200 = 1400)
    // Reveal at sequence 1250 succeeds!
    client.reveal(&bidder1, &bid1);
    client.reveal(&bidder2, &bid2);

    assert_eq!(client.get_revealed_bid(&bidder2).amount, 4500);

    // Finalize after extended window (sequence 1410 > 1400)
    env.ledger().set_sequence_number(1410);
    let winner = client.finalize_auction(&admin);
    assert_eq!(winner.unwrap().grantee, bidder2);
}

#[test]
fn test_congestion_simulation_over_variable_ledger_intervals() {
    let env = Env::default();
    env.mock_all_auths();

    env.ledger().set_sequence_number(5000);

    let contract_id = env.register(CommitRevealContract, ());
    let client = CommitRevealContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let bidder = Address::generate(&env);

    client.open_bidding(&admin, &1440, &1440, &120);

    // Simulate 60s ledger interval bursts (network congestion)
    for step in 1..=10 {
        let seq = 5000 + step * 100;
        env.ledger().set_sequence_number(seq);
    }

    // Sequence is 6000 (< 6440 commit deadline)
    let bid = create_revealed_bid(&env, &bidder, 8000, 1000, 99);
    client.commit(&bidder, &client.hash_bid(&bid));

    // Jump to reveal phase
    env.ledger().set_sequence_number(6500);
    client.reveal(&bidder, &bid);

    // Jump to close past effective deadline (7880 + 120 = 8000)
    env.ledger().set_sequence_number(8050);
    let winner = client.finalize_auction(&admin);
    assert!(winner.is_some());
    assert_eq!(winner.unwrap().grantee, bidder);
}

#[test]
fn test_fuzz_congestion_drift_over_10000_ledgers() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(CommitRevealContract, ());
    let client = CommitRevealContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let bidder = Address::generate(&env);

    let mut current_seq: u32 = 10_000;
    env.ledger().set_sequence_number(current_seq);

    // Open bidding: commit = 1440, reveal = 1440, grace = 120
    client.open_bidding(&admin, &1440, &1440, &120);

    // Simulate 10,000 ledgers of variable congestion (5s to 120s simulated durations)
    let mut rng_seed: u32 = 12345;
    let mut committed = false;
    let mut revealed = false;

    let bid = create_revealed_bid(&env, &bidder, 12000, 1000, 77);
    let commit_hash = client.hash_bid(&bid);

    for _i in 0..10_000 {
        // Linear congruential generator for pseudo-random delta
        rng_seed = (rng_seed.wrapping_mul(1103515245).wrapping_add(12345)) & 0x7fffffff;
        let delta = 1 + (rng_seed % 5); // advance 1-5 ledgers each tick
        current_seq += delta;
        env.ledger().set_sequence_number(current_seq);

        // Commit during commit phase (nominal commit deadline = 11440, effective = 11560)
        if current_seq >= 10500 && current_seq < 11440 && !committed {
            client.commit(&bidder, &commit_hash);
            committed = true;
        }

        // Reveal during reveal phase (nominal reveal deadline = 12880)
        if current_seq >= 11560 && current_seq < 12880 && committed && !revealed {
            client.reveal(&bidder, &bid);
            revealed = true;
        }

        if current_seq > 13500 {
            break;
        }
    }

    assert!(committed, "Bidder must successfully commit during congestion");
    assert!(revealed, "Bidder must successfully reveal during congestion");

    // Advance to finalization
    env.ledger().set_sequence_number(13500);
    let winner = client.finalize_auction(&admin);
    assert!(winner.is_some());
    assert_eq!(winner.unwrap().grantee, bidder);
}
