use soroban_sdk::Env;
use soroban_sdk::contracttype;

use crate::StorageKey;
use crate::proposal_manager::{Proposal, VoteRecord};

#[derive(Clone, Debug)]
#[contracttype]
pub struct QuorumConfig {
    pub quorum_bps: u32,
}

pub struct QuorumCalculator;

impl QuorumCalculator {
    pub fn initialize(env: &Env, total_supply: i128) {
        env.storage().instance().set(&StorageKey::QuorumBps, &400u32);
        env.storage().instance().set(&StorageKey::QuorumVotes, &(total_supply / 25));
    }

    pub fn calculate_quorum(env: &Env, proposal: &Proposal) -> i128 {
        let vote_records: Vec<VoteRecord> = env.storage()
            .instance()
            .get(&StorageKey::ProposalVotes(proposal.id))
            .unwrap_or_else(|| soroban_sdk::Vec::new(env));

        let mut quadratic_quorum: i128 = 0;

        for record in vote_records.iter() {
            let sqrt_votes = Self::integer_sqrt(record.votes);
            quadratic_quorum = quadratic_quorum.checked_add(sqrt_votes).expect("overflow");
        }

        quadratic_quorum
    }

    pub fn get_quorum_threshold(env: &Env) -> i128 {
        env.storage().instance().get(&StorageKey::QuorumVotes).expect("not initialized")
    }

    fn integer_sqrt(n: i128) -> i128 {
        if n <= 0 {
            return 0;
        }
        let mut x = n;
        let mut y = (x + 1) / 2;
        while y < x {
            x = y;
            y = (x + n / x) / 2;
        }
        x
    }
}
