//! Token-Weighted Jury Dispute Resolution for AgriTrust Protocol
//! Implements: Filed -> Evidence -> JurySelection -> Voting -> Ruling -> Appeal -> Final
//! - Staking pool: 100 token min opt-in
//! - 11 jurors selected initially (21 on appeal 1, 41 on appeal 2)
//! - Vote stake: 10 tokens
//! - Majority voter reward: +1 token; Minority voter penalty: -2 tokens
//! - 7-day appeal window with 2x fee; max 2 appeals (3 levels total)
//! - Automatic escrow release upon final binding ruling

#![allow(clippy::too_many_arguments)]

use soroban_sdk::{contracttype, Address, Env, String, Vec};

pub const MIN_JUROR_POOL_STAKE: i128 = 100;
pub const JURY_VOTE_STAKE: i128 = 10;
pub const MAJORITY_REWARD: i128 = 1;
pub const MINORITY_PENALTY: i128 = 2;
pub const VOTING_PERIOD_SECONDS: u64 = 72 * 60 * 60; // 72 hours
pub const APPEAL_WINDOW_SECONDS: u64 = 7 * 24 * 60 * 60; // 7 days
pub const MAX_APPEAL_LEVEL: u32 = 2; // Level 0 (11), Level 1 (21), Level 2 (41)

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JuryDisputeStage {
    Filed,
    Evidence,
    JurySelection,
    Voting,
    Ruling,
    Appeal,
    Final,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JuryVoteChoice {
    None,
    PlaintiffWin,
    DefendantWin,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JuryVoteRecord {
    pub juror: Address,
    pub choice: JuryVoteChoice,
    pub stake: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JuryDisputeCase {
    pub id: u32,
    pub plaintiff: Address,
    pub defendant: Address,
    pub escrow_amount: i128,
    pub stage: JuryDisputeStage,
    pub appeal_level: u32,
    pub selected_jurors: Vec<Address>,
    pub votes: Vec<JuryVoteRecord>,
    pub ruling: JuryVoteChoice,
    pub ruling_timestamp: u64,
    pub winner: Option<Address>,
    pub evidence_hashes: Vec<String>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JurorPoolMember {
    pub juror: Address,
    pub staked_amount: i128,
    pub active: bool,
}

/// Computes panel size based on appeal level (11 -> 21 -> 41)
pub fn jury_panel_size_for_level(level: u32) -> u32 {
    match level {
        0 => 11,
        1 => 21,
        _ => 41,
    }
}

/// Calculate majority and rewards/slashing for revealed votes.
/// Returns (winning_choice, majority_count, minority_count)
pub fn tally_jury_votes(
    votes: &Vec<JuryVoteRecord>,
) -> (JuryVoteChoice, u32, u32) {
    let mut plaintiff_count = 0u32;
    let mut defendant_count = 0u32;

    for i in 0..votes.len() {
        let v = votes.get(i).unwrap();
        match v.choice {
            JuryVoteChoice::PlaintiffWin => plaintiff_count += 1,
            JuryVoteChoice::DefendantWin => defendant_count += 1,
            JuryVoteChoice::None => {}
        }
    }

    if plaintiff_count > defendant_count {
        (JuryVoteChoice::PlaintiffWin, plaintiff_count, defendant_count)
    } else if defendant_count > plaintiff_count {
        (JuryVoteChoice::DefendantWin, defendant_count, plaintiff_count)
    } else {
        (JuryVoteChoice::None, 0, 0)
    }
}

/// Compute net payout for a juror based on outcome.
/// Returns (return_amount, net_delta)
/// Majority voter: returns 10 stake + 1 reward = 11 tokens (+1 net)
/// Minority voter: returns 10 stake - 2 penalty = 8 tokens (-2 net)
/// Tie: returns 10 stake (0 net)
pub fn calculate_juror_payout(choice: JuryVoteChoice, winning_choice: JuryVoteChoice) -> (i128, i128) {
    if winning_choice == JuryVoteChoice::None {
        (JURY_VOTE_STAKE, 0)
    } else if choice == winning_choice {
        (JURY_VOTE_STAKE + MAJORITY_REWARD, MAJORITY_REWARD)
    } else {
        (JURY_VOTE_STAKE - MINORITY_PENALTY, -MINORITY_PENALTY)
    }
}

/// Deterministic pseudo-random jury selection from eligible pool.
pub fn select_panel_from_pool(
    env: &Env,
    pool: &Vec<Address>,
    panel_size: u32,
    seed: u64,
) -> Vec<Address> {
    let pool_len = pool.len();
    if pool_len < panel_size {
        panic!("Insufficient jurors in active pool");
    }

    let mut selected: Vec<Address> = Vec::new(env);
    let mut nonce = 0u64;

    while selected.len() < panel_size {
        // Pseudo-random index selection
        let pseudo_rand = (seed.wrapping_add(nonce.wrapping_mul(6364136223846793005))) % (pool_len as u64);
        let candidate = pool.get(pseudo_rand as u32).unwrap();

        // Check uniqueness
        let mut exists = false;
        for i in 0..selected.len() {
            if selected.get(i).unwrap() == candidate {
                exists = true;
                break;
            }
        }

        if !exists {
            selected.push_back(candidate);
        }
        nonce += 1;
    }

    selected
}
