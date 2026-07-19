#![no_std]
#[cfg(test)]
extern crate std;
use soroban_sdk::{
    contract, contractimpl, contracttype, token, xdr::ToXdr, Address, BytesN, Env, Vec,
};

#[cfg(test)]
mod test;

pub mod settlement;

pub const TTL_EXTENSION_PERIOD: u32 = 518_400; // 30 days in ledgers (~5s per ledger)
pub const MAX_SETTLEMENT_WINDOW: u64 = 30 * 24 * 60 * 60; // 30 days in seconds
pub const MIN_JUROR_STAKE: i128 = 100;
pub const VOTE_STAKE: i128 = 10;
pub const MAJORITY_REWARD: i128 = 1;
pub const MINORITY_SLASH: i128 = 2;
pub const VOTING_PERIOD_SECONDS: u64 = 72 * 60 * 60;
pub const APPEAL_WINDOW_SECONDS: u64 = 7 * 24 * 60 * 60;
pub const MAX_APPEALS: u32 = 2;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Token,
    DisputeCounter,
    Dispute(u32),
    Evidence(u32),
    JurorStake(Address),
    JurorPool,
    SelectedJurors(u32, u32),
    Vote(u32, u32, Address),
    EscrowLock(u32),
    EscrowRelease(u32),
    EscrowTtlDeadline(u32),
    EscrowCycleCounter,
    ExpiredEscrows,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowLockData {
    pub buyer: Address,
    pub seller: Address,
    pub arbitration_id: u32,
    pub amount: i128,
    pub locked_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowReleaseData {
    pub buyer: Address,
    pub seller: Address,
    pub arbitration_id: u32,
    pub amount: i128,
    pub released_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtlDeadline {
    pub ledger_sequence: u32,
    pub ttl_extension_period: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    Filed,
    Evidence,
    JurySelection,
    Voting,
    Ruling,
    Appeal,
    Final,
    Pending,
    InArbitration,
    Resolved,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Ruling {
    PlaintiffWin,
    DefendantWin,
    None,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dispute {
    pub grant_id: u32,
    pub funder: Address,
    pub grantee: Address,
    pub amount: i128,
    pub status: DisputeStatus,
    pub arbitrator: Address,
    pub arbitrator_public_key: BytesN<32>,
    pub filed_at: u64,
    pub voting_deadline: u64,
    pub ruling_at: u64,
    pub appeal_level: u32,
    pub ruling: Ruling,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoteRecord {
    pub commit: BytesN<32>,
    pub revealed: bool,
    pub ruling: Ruling,
}

#[contract]
pub struct ArbitrationContract;

#[contractimpl]
impl ArbitrationContract {
    pub fn init(env: Env, admin: Address, token: Address) {
        admin.require_auth();
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage()
            .instance()
            .set(&DataKey::DisputeCounter, &0u32);
        env.storage()
            .instance()
            .set(&DataKey::JurorPool, &Vec::<Address>::new(&env));
        env.storage().instance().extend_ttl(0, 518_400);
    }

    pub fn opt_in_juror(env: Env, juror: Address, stake: i128) {
        juror.require_auth();
        if stake < MIN_JUROR_STAKE {
            panic!("minimum stake is 100 AGRI");
        }
        let token_addr: Address = env.storage().instance().get(&DataKey::Token).unwrap();
        token::Client::new(&env, &token_addr).transfer(
            &juror,
            &env.current_contract_address(),
            &stake,
        );
        let previous = Self::juror_stake(env.clone(), juror.clone());
        env.storage()
            .persistent()
            .set(&DataKey::JurorStake(juror.clone()), &(previous + stake));
        let mut pool: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::JurorPool)
            .unwrap_or(Vec::new(&env));
        if previous == 0 {
            pool.push_back(juror);
            env.storage().instance().set(&DataKey::JurorPool, &pool);
        }
    }

    pub fn raise_dispute(
        env: Env,
        grant_id: u32,
        funder: Address,
        grantee: Address,
        amount: i128,
        arbitrator: Address,
        arbitrator_public_key: BytesN<32>,
    ) -> u32 {
        funder.require_auth();
        let token_addr: Address = env.storage().instance().get(&DataKey::Token).unwrap();
        token::Client::new(&env, &token_addr).transfer(
            &funder,
            &env.current_contract_address(),
            &amount,
        );
        let mut counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey::DisputeCounter)
            .unwrap();
        counter += 1;
        env.storage()
            .instance()
            .set(&DataKey::DisputeCounter, &counter);
        let dispute = Dispute {
            grant_id,
            funder,
            grantee,
            amount,
            status: DisputeStatus::Filed,
            arbitrator,
            arbitrator_public_key,
            filed_at: env.ledger().timestamp(),
            voting_deadline: 0,
            ruling_at: 0,
            appeal_level: 0,
            ruling: Ruling::None,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Dispute(counter), &dispute);
        counter
    }

    pub fn submit_evidence(env: Env, dispute_id: u32, submitter: Address, ipfs_hash: BytesN<32>) {
        submitter.require_auth();
        let mut dispute: Dispute = env
            .storage()
            .persistent()
            .get(&DataKey::Dispute(dispute_id))
            .unwrap();
        if submitter != dispute.funder && submitter != dispute.grantee {
            panic!("not a party");
        }
        if dispute.status != DisputeStatus::Filed && dispute.status != DisputeStatus::Evidence {
            panic!("evidence closed");
        }
        let mut evidence: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&DataKey::Evidence(dispute_id))
            .unwrap_or(Vec::new(&env));
        evidence.push_back(ipfs_hash);
        dispute.status = DisputeStatus::Evidence;
        env.storage()
            .persistent()
            .set(&DataKey::Evidence(dispute_id), &evidence);
        env.storage()
            .persistent()
            .set(&DataKey::Dispute(dispute_id), &dispute);
    }

    pub fn select_jury(env: Env, dispute_id: u32, randomness: u64) {
        let mut dispute: Dispute = env
            .storage()
            .persistent()
            .get(&DataKey::Dispute(dispute_id))
            .unwrap();
        if dispute.status != DisputeStatus::Filed
            && dispute.status != DisputeStatus::Evidence
            && dispute.status != DisputeStatus::Appeal
        {
            panic!("jury selection unavailable");
        }
        let panel_size = Self::panel_size(dispute.appeal_level);
        let pool: Vec<Address> = env.storage().instance().get(&DataKey::JurorPool).unwrap();
        if pool.len() < panel_size {
            panic!("insufficient jurors");
        }
        let mut selected = Vec::new(&env);
        let mut cursor = (randomness as u32) % pool.len();
        while selected.len() < panel_size {
            let candidate = pool.get(cursor).unwrap();
            if !selected.contains(candidate.clone()) {
                selected.push_back(candidate);
            }
            cursor = (cursor + 1) % pool.len();
        }
        dispute.status = DisputeStatus::Voting;
        dispute.voting_deadline = env.ledger().timestamp() + VOTING_PERIOD_SECONDS;
        env.storage().persistent().set(
            &DataKey::SelectedJurors(dispute_id, dispute.appeal_level),
            &selected,
        );
        env.storage()
            .persistent()
            .set(&DataKey::Dispute(dispute_id), &dispute);
    }

    pub fn commit_vote(env: Env, dispute_id: u32, juror: Address, commitment: BytesN<32>) {
        juror.require_auth();
        let dispute: Dispute = env
            .storage()
            .persistent()
            .get(&DataKey::Dispute(dispute_id))
            .unwrap();
        if dispute.status != DisputeStatus::Voting {
            panic!("not voting");
        }
        if env.ledger().timestamp() > dispute.voting_deadline {
            panic!("voting closed");
        }
        Self::require_selected(&env, dispute_id, dispute.appeal_level, &juror);
        let stake = Self::juror_stake(env.clone(), juror.clone());
        if stake < VOTE_STAKE {
            panic!("insufficient vote stake");
        }
        env.storage()
            .persistent()
            .set(&DataKey::JurorStake(juror.clone()), &(stake - VOTE_STAKE));
        env.storage().persistent().set(
            &DataKey::Vote(dispute_id, dispute.appeal_level, juror),
            &VoteRecord {
                commit: commitment,
                revealed: false,
                ruling: Ruling::None,
            },
        );
    }

    pub fn reveal_vote(
        env: Env,
        dispute_id: u32,
        juror: Address,
        ruling: Ruling,
        salt: BytesN<32>,
    ) {
        juror.require_auth();
        let dispute: Dispute = env
            .storage()
            .persistent()
            .get(&DataKey::Dispute(dispute_id))
            .unwrap();
        if dispute.status != DisputeStatus::Voting {
            panic!("not voting");
        }
        if ruling == Ruling::None {
            panic!("invalid ruling");
        }
        let key = DataKey::Vote(dispute_id, dispute.appeal_level, juror.clone());
        let mut vote: VoteRecord = env.storage().persistent().get(&key).unwrap();
        if vote.revealed {
            panic!("already revealed");
        }
        if env
            .crypto()
            .keccak256(&(ruling.clone(), salt).to_xdr(&env))
            .to_bytes()
            != vote.commit
        {
            panic!("bad reveal");
        }
        vote.revealed = true;
        vote.ruling = ruling;
        env.storage().persistent().set(&key, &vote);
    }

    pub fn tally_ruling(env: Env, dispute_id: u32) -> Ruling {
        let mut dispute: Dispute = env
            .storage()
            .persistent()
            .get(&DataKey::Dispute(dispute_id))
            .unwrap();
        if dispute.status != DisputeStatus::Voting {
            panic!("not voting");
        }
        if env.ledger().timestamp() <= dispute.voting_deadline {
            panic!("voting active");
        }
        let jurors: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::SelectedJurors(dispute_id, dispute.appeal_level))
            .unwrap();
        let mut plaintiff = 0u32;
        let mut defendant = 0u32;
        for juror in jurors.iter() {
            if let Some(vote) =
                env.storage()
                    .persistent()
                    .get::<DataKey, VoteRecord>(&DataKey::Vote(
                        dispute_id,
                        dispute.appeal_level,
                        juror.clone(),
                    ))
            {
                if vote.revealed && vote.ruling == Ruling::PlaintiffWin {
                    plaintiff += 1;
                }
                if vote.revealed && vote.ruling == Ruling::DefendantWin {
                    defendant += 1;
                }
            }
        }
        let majority = if plaintiff >= defendant {
            Ruling::PlaintiffWin
        } else {
            Ruling::DefendantWin
        };
        for juror in jurors.iter() {
            if let Some(vote) =
                env.storage()
                    .persistent()
                    .get::<DataKey, VoteRecord>(&DataKey::Vote(
                        dispute_id,
                        dispute.appeal_level,
                        juror.clone(),
                    ))
            {
                let current = Self::juror_stake(env.clone(), juror.clone());
                let payout = if vote.revealed && vote.ruling == majority {
                    current + VOTE_STAKE + MAJORITY_REWARD
                } else {
                    current + VOTE_STAKE.saturating_sub(MINORITY_SLASH)
                };
                env.storage()
                    .persistent()
                    .set(&DataKey::JurorStake(juror), &payout);
            }
        }
        dispute.ruling = majority.clone();
        dispute.status = DisputeStatus::Ruling;
        dispute.ruling_at = env.ledger().timestamp();
        env.storage()
            .persistent()
            .set(&DataKey::Dispute(dispute_id), &dispute);
        majority
    }

    pub fn appeal(env: Env, dispute_id: u32, appellant: Address) {
        appellant.require_auth();
        let mut dispute: Dispute = env
            .storage()
            .persistent()
            .get(&DataKey::Dispute(dispute_id))
            .unwrap();
        if dispute.status != DisputeStatus::Ruling {
            panic!("not appealable");
        }
        if env.ledger().timestamp() > dispute.ruling_at + APPEAL_WINDOW_SECONDS {
            panic!("appeal window closed");
        }
        if dispute.appeal_level >= MAX_APPEALS {
            panic!("max appeals reached");
        }
        let fee = dispute.amount * 2;
        let token_addr: Address = env.storage().instance().get(&DataKey::Token).unwrap();
        token::Client::new(&env, &token_addr).transfer(
            &appellant,
            &env.current_contract_address(),
            &fee,
        );
        dispute.amount += fee;
        dispute.appeal_level += 1;
        dispute.status = DisputeStatus::Appeal;
        dispute.ruling = Ruling::None;
        env.storage()
            .persistent()
            .set(&DataKey::Dispute(dispute_id), &dispute);
    }

    pub fn finalize(env: Env, dispute_id: u32) {
        let mut dispute: Dispute = env
            .storage()
            .persistent()
            .get(&DataKey::Dispute(dispute_id))
            .unwrap();
        if dispute.status != DisputeStatus::Ruling {
            panic!("not finalizable");
        }
        if dispute.appeal_level < MAX_APPEALS
            && env.ledger().timestamp() <= dispute.ruling_at + APPEAL_WINDOW_SECONDS
        {
            panic!("appeal window active");
        }
        let token_addr: Address = env.storage().instance().get(&DataKey::Token).unwrap();
        let winner = if dispute.ruling == Ruling::PlaintiffWin {
            dispute.funder.clone()
        } else {
            dispute.grantee.clone()
        };
        token::Client::new(&env, &token_addr).transfer(
            &env.current_contract_address(),
            &winner,
            &dispute.amount,
        );
        dispute.status = DisputeStatus::Final;
        env.storage()
            .persistent()
            .set(&DataKey::Dispute(dispute_id), &dispute);
    }

    pub fn resolve_dispute(env: Env, dispute_id: u32, funder_award: i128, grantee_award: i128) {
        let mut dispute: Dispute = env
            .storage()
            .persistent()
            .get(&DataKey::Dispute(dispute_id))
            .unwrap();
        dispute.arbitrator.require_auth();
        if dispute.status == DisputeStatus::Resolved {
            panic!("Already resolved");
        }
        if funder_award + grantee_award > dispute.amount {
            panic!("Awards exceed amount");
        }
        dispute.status = DisputeStatus::Resolved;
        let token_addr: Address = env.storage().instance().get(&DataKey::Token).unwrap();
        let token_client = token::Client::new(&env, &token_addr);
        if funder_award > 0 {
            token_client.transfer(
                &env.current_contract_address(),
                &dispute.funder,
                &funder_award,
            );
        }
        if grantee_award > 0 {
            token_client.transfer(
                &env.current_contract_address(),
                &dispute.grantee,
                &grantee_award,
            );
        }
        env.storage()
            .persistent()
            .set(&DataKey::Dispute(dispute_id), &dispute);
    }

    pub fn get_dispute(env: Env, dispute_id: u32) -> Dispute {
        env.storage()
            .persistent()
            .get(&DataKey::Dispute(dispute_id))
            .unwrap()
    }
    pub fn get_evidence(env: Env, dispute_id: u32) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&DataKey::Evidence(dispute_id))
            .unwrap_or(Vec::new(&env))
    }
    pub fn get_selected_jurors(env: Env, dispute_id: u32, level: u32) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::SelectedJurors(dispute_id, level))
            .unwrap()
    }
    pub fn juror_stake(env: Env, juror: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::JurorStake(juror))
            .unwrap_or(0)
    }

    pub fn lock_settlement(
        env: Env,
        cycle: u32,
        buyer: Address,
        seller: Address,
        arbitration_id: u32,
        amount: i128,
    ) {
        settlement::lock_settlement(&env, cycle, &buyer, &seller, arbitration_id, amount);
    }
    pub fn release_settlement(
        env: Env,
        cycle: u32,
        buyer: Address,
        seller: Address,
        arbitration_id: u32,
        amount: i128,
    ) {
        settlement::release_settlement(&env, cycle, &buyer, &seller, arbitration_id, amount);
    }
    pub fn synchronize_escrow_ttl(env: Env, cycle: u32) {
        settlement::synchronize_escrow_ttl(&env, cycle);
    }
    pub fn garbage_collect_expired_escrows(env: Env, max_cycles: u32) -> u32 {
        settlement::garbage_collect_expired_escrows(&env, max_cycles)
    }
    pub fn get_escrow_lock(env: Env, cycle: u32) -> Option<EscrowLockData> {
        env.storage().persistent().get(&DataKey::EscrowLock(cycle))
    }
    pub fn get_escrow_release(env: Env, cycle: u32) -> Option<EscrowReleaseData> {
        env.storage()
            .persistent()
            .get(&DataKey::EscrowRelease(cycle))
    }
    pub fn get_escrow_ttl_deadline(env: Env, cycle: u32) -> Option<TtlDeadline> {
        env.storage()
            .persistent()
            .get(&DataKey::EscrowTtlDeadline(cycle))
    }

    fn panel_size(level: u32) -> u32 {
        if level == 0 {
            11
        } else if level == 1 {
            21
        } else {
            41
        }
    }

    fn require_selected(env: &Env, dispute_id: u32, level: u32, juror: &Address) {
        let jurors: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::SelectedJurors(dispute_id, level))
            .unwrap();
        if !jurors.contains(juror.clone()) {
            panic!("not selected");
        }
    }
}
