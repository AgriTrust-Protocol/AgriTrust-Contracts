use soroban_sdk::{Address, Env, String, Vec};
use soroban_sdk::contracttype;

use crate::StorageKey;

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub enum ProposalStatus {
    Draft,
    Voting,
    Queued,
    Executed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub enum ProposalType {
    TreasurySpend,
    ParameterChange,
    ContractUpgrade,
    TextProposal,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct Proposal {
    pub id: u64,
    pub proposer: Address,
    pub title: String,
    pub description: String,
    pub proposal_type: ProposalType,
    pub status: ProposalStatus,
    pub created_at: u64,
    pub voting_start: u64,
    pub voting_end: u64,
    pub queue_time: u64,
    pub executed_at: u64,
    pub current_version: u32,
    pub for_votes: i128,
    pub against_votes: i128,
    pub total_voters: u32,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct ProposalVersion {
    pub version: u32,
    pub title: String,
    pub description: String,
    pub created_at: u64,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct VoteRecord {
    pub voter: Address,
    pub proposal_id: u64,
    pub support: bool,
    pub votes: i128,
    pub cost: i128,
    pub voted_at: u64,
}

pub struct ProposalManager;

impl ProposalManager {
    pub fn create_proposal(
        env: &Env,
        proposer: &Address,
        title: String,
        description: String,
        proposal_type: ProposalType,
        voting_period_sec: u64,
    ) -> u64 {
        let counter: u64 = env.storage().instance().get(&StorageKey::ProposalIdCounter).unwrap_or(0);
        let proposal_id = counter + 1;
        env.storage().instance().set(&StorageKey::ProposalIdCounter, &proposal_id);

        let now = env.ledger().timestamp();
        let voting_start = now;
        let voting_end = now.checked_add(voting_period_sec).expect("overflow");

        let proposal = Proposal {
            id: proposal_id,
            proposer: proposer.clone(),
            title: title.clone(),
            description: description.clone(),
            proposal_type,
            status: ProposalStatus::Draft,
            created_at: now,
            voting_start,
            voting_end,
            queue_time: 0,
            executed_at: 0,
            current_version: 0,
            for_votes: 0,
            against_votes: 0,
            total_voters: 0,
        };

        let version = ProposalVersion {
            version: 0,
            title,
            description,
            created_at: now,
        };

        env.storage().instance().set(&StorageKey::Proposal(proposal_id), &proposal);
        env.storage().instance().set(&StorageKey::ProposalVersion(proposal_id, 0), &version);
        env.storage().instance().set(&StorageKey::ProposalVotes(proposal_id), &Vec::<VoteRecord>::new(env));

        let mut ids: Vec<u64> = env.storage().instance().get(&StorageKey::ProposalIds).unwrap_or_else(|| Vec::new(env));
        ids.push_back(proposal_id);
        env.storage().instance().set(&StorageKey::ProposalIds, &ids);

        env.events().publish(
            (soroban_sdk::symbol_short!("propose"), proposal_id),
            (proposer.clone(), voting_end),
        );

        proposal_id
    }

    pub fn amend_proposal(
        env: &Env,
        proposer: &Address,
        proposal_id: u64,
        new_title: String,
        new_description: String,
    ) -> u32 {
        let mut proposal = Self::get_proposal(env, proposal_id);

        if &proposal.proposer != proposer {
            panic!("only proposer can amend");
        }
        if proposal.status != ProposalStatus::Draft {
            panic!("can only amend in draft phase");
        }

        let now = env.ledger().timestamp();
        let new_version = proposal.current_version + 1;

        let version = ProposalVersion {
            version: new_version,
            title: new_title.clone(),
            description: new_description.clone(),
            created_at: now,
        };

        proposal.title = new_title;
        proposal.description = new_description;
        proposal.current_version = new_version;

        env.storage().instance().set(&StorageKey::Proposal(proposal_id), &proposal);
        env.storage().instance().set(&StorageKey::ProposalVersion(proposal_id, new_version), &version);

        env.events().publish(
            (soroban_sdk::symbol_short!("amend"), proposal_id),
            (new_version, now),
        );

        new_version
    }

    pub fn start_voting(env: &Env, proposal_id: u64) {
        let mut proposal = Self::get_proposal(env, proposal_id);
        if proposal.status != ProposalStatus::Draft {
            panic!("proposal not in draft phase");
        }
        proposal.status = ProposalStatus::Voting;
        Self::save_proposal(env, proposal_id, &proposal);
    }

    pub fn cast_vote(
        env: &Env,
        voter: &Address,
        proposal_id: u64,
        support: bool,
        votes: i128,
        cost: i128,
    ) {
        let mut proposal = Self::get_proposal(env, proposal_id);
        if proposal.status != ProposalStatus::Voting {
            panic!("not voting phase");
        }

        let vote_records_key = StorageKey::ProposalVotes(proposal_id);
        let mut vote_records: Vec<VoteRecord> = env.storage().instance().get(&vote_records_key).unwrap_or_else(|| Vec::new(env));

        for existing in vote_records.iter() {
            if existing.voter == *voter {
                panic!("already voted");
            }
        }

        let now = env.ledger().timestamp();
        let record = VoteRecord {
            voter: voter.clone(),
            proposal_id,
            support,
            votes,
            cost,
            voted_at: now,
        };

        vote_records.push_back(record);
        env.storage().instance().set(&vote_records_key, &vote_records);

        if support {
            proposal.for_votes = proposal.for_votes.checked_add(votes).expect("overflow");
        } else {
            proposal.against_votes = proposal.against_votes.checked_add(votes).expect("overflow");
        }
        proposal.total_voters = proposal.total_voters.checked_add(1).expect("overflow");

        Self::save_proposal(env, proposal_id, &proposal);

        env.events().publish(
            (soroban_sdk::symbol_short!("vote"), proposal_id),
            (voter.clone(), support, votes, cost),
        );
    }

    pub fn get_proposal(env: &Env, proposal_id: u64) -> Proposal {
        env.storage()
            .instance()
            .get(&StorageKey::Proposal(proposal_id))
            .expect("proposal not found")
    }

    pub fn get_proposal_version(env: &Env, proposal_id: u64, version: u32) -> ProposalVersion {
        env.storage()
            .instance()
            .get(&StorageKey::ProposalVersion(proposal_id, version))
            .expect("version not found")
    }

    pub fn get_all_proposal_ids(env: &Env) -> Vec<u64> {
        env.storage()
            .instance()
            .get(&StorageKey::ProposalIds)
            .unwrap_or_else(|| Vec::new(env))
    }

    pub fn save_proposal(env: &Env, proposal_id: u64, proposal: &Proposal) {
        env.storage().instance().set(&StorageKey::Proposal(proposal_id), proposal);
    }
}
