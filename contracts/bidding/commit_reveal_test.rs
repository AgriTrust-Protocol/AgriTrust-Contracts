#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Env, Map, Bytes, Vec};

fn setup() -> (Env, CommitRevealContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, CommitRevealContract);
    let client = CommitRevealContractClient::new(&env, &contract_id);
    (env, client)
}

#[test]
fn test_commit_and_reveal_success() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);

    client.open_bidding(&admin, &(env.ledger().timestamp() + 86400));

    let mut milestone_costs = Map::new(&env);
    milestone_costs.set(1u32, 500u64);
    milestone_costs.set(2u32, 300u64);

    let salt = Bytes::from_array(&env, &[42u8; 16]);
    let bid = RevealedBid {
        grantee: grantee.clone(),
        amount: 1000,
        min_bid: 500,
        milestone_costs: milestone_costs.clone(),
        salt: salt.clone(),
        position_nonce: 1,
    };

    let commitment = CommitRevealContract::hash_bid(&env, &bid);

    client.commit(&grantee, &commitment);
    client.close_bidding(&admin);
    client.reveal(&grantee, &bid);

    let revealed = client.get_revealed_bid(&grantee);
    assert_eq!(revealed.amount, 1000);
}

#[test]
#[should_panic(expected = "does not match commitment")]
fn test_tampered_reveal_rejected() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);

    client.open_bidding(&admin, &(env.ledger().timestamp() + 86400));

    let salt = Bytes::from_array(&env, &[1u8; 16]);
    let real_bid = RevealedBid {
        grantee: grantee.clone(),
        amount: 1000,
        min_bid: 500,
        milestone_costs: Map::new(&env),
        salt: salt.clone(),
        position_nonce: 1,
    };
    let commitment = CommitRevealContract::hash_bid(&env, &real_bid);
    client.commit(&grantee, &commitment);
    client.close_bidding(&admin);

    let tampered_bid = RevealedBid {
        grantee: grantee.clone(),
        amount: 1,
        min_bid: 500,
        milestone_costs: Map::new(&env),
        salt: salt,
        position_nonce: 1,
    };
    client.reveal(&grantee, &tampered_bid);
}

#[test]
#[should_panic(expected = "Revealed bid is below the committed minimum")]
fn test_below_minimum_bid_rejected() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let grantee = Address::generate(&env);

    client.open_bidding(&admin, &(env.ledger().timestamp() + 86400));

    let salt = Bytes::from_array(&env, &[1u8; 16]);
    let real_bid = RevealedBid {
        grantee: grantee.clone(),
        amount: 400,
        min_bid: 500,
        milestone_costs: Map::new(&env),
        salt: salt.clone(),
        position_nonce: 1,
    };
    let commitment = CommitRevealContract::hash_bid(&env, &real_bid);
    client.commit(&grantee, &commitment);
    client.close_bidding(&admin);

    client.reveal(&grantee, &real_bid);
}

#[test]
fn test_mempool_frontrunning_defeated_by_ordering() {
    // Unit test: simulate mempool observation where attacker sees a valid commitment and tries to front-run
    // verify the position commitment ordering defeats it.
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let victim = Address::generate(&env);
    let attacker = Address::generate(&env);

    client.open_bidding(&admin, &(env.ledger().timestamp() + 86400));

    let victim_salt = Bytes::from_array(&env, &[1u8; 16]);
    let victim_bid = RevealedBid {
        grantee: victim.clone(),
        amount: 1000,
        min_bid: 500,
        milestone_costs: Map::new(&env),
        salt: victim_salt,
        position_nonce: 100, // Victims position nonce
    };
    let victim_commitment = CommitRevealContract::hash_bid(&env, &victim_bid);
    client.commit(&victim, &victim_commitment);

    let attacker_salt = Bytes::from_array(&env, &[2u8; 16]);
    let attacker_bid = RevealedBid {
        grantee: attacker.clone(),
        amount: 1100,
        min_bid: 500,
        milestone_costs: Map::new(&env),
        salt: attacker_salt,
        position_nonce: 200, 
    };
    let attacker_commitment = CommitRevealContract::hash_bid(&env, &attacker_bid);
    client.commit(&attacker, &attacker_commitment);

    client.close_bidding(&admin);

    // Attacker submits their reveal FIRST (simulating reordering by validator)
    client.reveal(&attacker, &attacker_bid);
    // Victim submits SECOND
    client.reveal(&victim, &victim_bid);

    // Get ordered reveals and verify the ordering is deterministic based on hash, NOT ledger order
    let ordered_reveals = client.get_ordered_reveals();
    
    let hash_victim = CommitRevealContract::ordering_hash(&env, &victim_bid);
    let hash_attacker = CommitRevealContract::ordering_hash(&env, &attacker_bid);
    
    // We expect the one with smaller hash to be first
    if hash_victim < hash_attacker {
        assert_eq!(ordered_reveals.get(0).unwrap().grantee, victim);
        assert_eq!(ordered_reveals.get(1).unwrap().grantee, attacker);
    } else {
        assert_eq!(ordered_reveals.get(0).unwrap().grantee, attacker);
        assert_eq!(ordered_reveals.get(1).unwrap().grantee, victim);
    }
}

#[test]
fn test_integration_10_bidders_deterministic_ordering() {
    let (env, client) = setup();
    let admin = Address::generate(&env);

    client.open_bidding(&admin, &(env.ledger().timestamp() + 86400));

    let mut bids = std::vec::Vec::new();
    for i in 0..10 {
        let grantee = Address::generate(&env);
        let salt = Bytes::from_array(&env, &[i as u8; 16]);
        let bid = RevealedBid {
            grantee: grantee.clone(),
            amount: 1000 + i as u64,
            min_bid: 500,
            milestone_costs: Map::new(&env),
            salt: salt.clone(),
            position_nonce: i as u64 * 10,
        };
        let commitment = CommitRevealContract::hash_bid(&env, &bid);
        client.commit(&grantee, &commitment);
        bids.push(bid);
    }

    client.close_bidding(&admin);

    // Submit reveals in reverse order
    for bid in bids.iter().rev() {
        client.reveal(&bid.grantee, bid);
    }

    // Now test batch submission atomicity (as an alternative, we could have submitted all via batch)
    // Here we just test the deterministic ordering
    let ordered_reveals = client.get_ordered_reveals();
    assert_eq!(ordered_reveals.len(), 10);
    
    // Verify they are sorted by ordering_hash correctly
    for i in 0..9 {
        let a = ordered_reveals.get(i).unwrap();
        let b = ordered_reveals.get(i + 1).unwrap();
        let hash_a = CommitRevealContract::ordering_hash(&env, &a);
        let hash_b = CommitRevealContract::ordering_hash(&env, &b);
        assert!(hash_a <= hash_b, "Reveals are not deterministically ordered!");
    }
}