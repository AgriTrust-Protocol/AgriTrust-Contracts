use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short,
    Address, Bytes, BytesN, Env, Map, Vec,
};

// ── Constants ───────────────────────────────────────────────────────────────

pub const DEFAULT_COMMIT_WINDOW: u32 = 1440; // ~2 hours at nominal 5s/ledger
pub const DEFAULT_REVEAL_WINDOW: u32 = 1440; // ~2 hours at nominal 5s/ledger
pub const DEFAULT_PHASE_GRACE: u32 = 120; // 120 ledgers (~10 min) overlap grace period
pub const DEFAULT_EXPECTED_LEDGER_DURATION: u64 = 5; // 5 seconds per nominal ledger
pub const DEFAULT_PHASE_CLOCK_DRIFT: u32 = 360; // Max allowable clock drift compensation (~30 min)

// ── Data Types ──────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct AuctionConfig {
    pub admin: Address,
    pub commit_start: u32,
    pub commit_deadline: u32,
    pub reveal_deadline: u32,
    pub phase_grace: u32,
    pub expected_ledger_duration: u64,
    pub phase_clock_drift: u32,
    pub is_open: bool,
    pub finalized: bool,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RevealedBid {
    pub grantee: Address,
    pub amount: u64,
    pub min_bid: u64,
    pub milestone_costs: Map<u32, u64>,
    pub salt: Bytes,
    pub position_nonce: u64,
}

// ── Storage Keys ────────────────────────────────────────────────────────────

#[contracttype]
pub enum BidKey {
    Config,
    Commitment(Address),
    CommitmentsList,
    Reveal(Address),
    RevealsSequence,
    DriftCompensationVotes,
    DriftCompensationActive,
    WinningBid,
}

// ── Contract ────────────────────────────────────────────────────────────────

#[contract]
pub struct CommitRevealContract;

#[contractimpl]
impl CommitRevealContract {
    /// Admin opens the bidding auction with dynamic phase boundaries and congestion tolerance.
    pub fn open_bidding(
        env: Env,
        admin: Address,
        commit_window: u32,
        reveal_window: u32,
        phase_grace: u32,
    ) {
        admin.require_auth();

        let current_seq = env.ledger().sequence();
        let c_window = if commit_window == 0 { DEFAULT_COMMIT_WINDOW } else { commit_window };
        let r_window = if reveal_window == 0 { DEFAULT_REVEAL_WINDOW } else { reveal_window };
        let grace = if phase_grace == 0 { DEFAULT_PHASE_GRACE } else { phase_grace };

        let commit_deadline = current_seq.checked_add(c_window).expect("Sequence overflow");
        let reveal_deadline = commit_deadline.checked_add(r_window).expect("Sequence overflow");

        let config = AuctionConfig {
            admin,
            commit_start: current_seq,
            commit_deadline,
            reveal_deadline,
            phase_grace: grace,
            expected_ledger_duration: DEFAULT_EXPECTED_LEDGER_DURATION,
            phase_clock_drift: DEFAULT_PHASE_CLOCK_DRIFT,
            is_open: true,
            finalized: false,
        };

        env.storage().instance().set(&BidKey::Config, &config);
        let empty_commits: Vec<Address> = Vec::new(&env);
        env.storage().instance().set(&BidKey::CommitmentsList, &empty_commits);
        let empty_reveals: Vec<RevealedBid> = Vec::new(&env);
        env.storage().instance().set(&BidKey::RevealsSequence, &empty_reveals);
        let empty_votes: Vec<Address> = Vec::new(&env);
        env.storage().instance().set(&BidKey::DriftCompensationVotes, &empty_votes);
        env.storage().instance().set(&BidKey::DriftCompensationActive, &false);
    }

    /// Admin can adjust phase clock drift parameters via governance.
    pub fn set_phase_clock_drift(env: Env, admin: Address, new_drift: u32) {
        admin.require_auth();
        let mut config: AuctionConfig = env
            .storage()
            .instance()
            .get(&BidKey::Config)
            .expect("Auction not initialized");

        if config.admin != admin {
            panic!("Unauthorized admin");
        }

        config.phase_clock_drift = new_drift;
        env.storage().instance().set(&BidKey::Config, &config);
    }

    /// Admin can adjust phase grace period via governance.
    pub fn set_phase_grace(env: Env, admin: Address, new_grace: u32) {
        admin.require_auth();
        let mut config: AuctionConfig = env
            .storage()
            .instance()
            .get(&BidKey::Config)
            .expect("Auction not initialized");

        if config.admin != admin {
            panic!("Unauthorized admin");
        }

        config.phase_grace = new_grace;
        env.storage().instance().set(&BidKey::Config, &config);
    }

    /// Grantee submits SHA-256 hash commitment.
    /// Under network congestion, allows submission within `commit_deadline + PHASE_GRACE`.
    pub fn commit(env: Env, grantee: Address, commitment: BytesN<32>) {
        grantee.require_auth();

        let config: AuctionConfig = env
            .storage()
            .instance()
            .get(&BidKey::Config)
            .expect("Auction not initialized");

        if !config.is_open || config.finalized {
            panic!("Bidding window is closed");
        }

        let current_seq = env.ledger().sequence();
        let commit_limit = config.commit_deadline.checked_add(config.phase_grace).expect("Overflow");

        if current_seq >= commit_limit {
            panic!("Commit phase deadline exceeded");
        }

        // Emit warning if committing during the clock drift grace window
        if current_seq >= config.commit_deadline {
            env.events().publish(
                (symbol_short!("drift_wrn"), symbol_short!("cmt_grace")),
                (grantee.clone(), current_seq, config.commit_deadline),
            );
        }

        // Prevent duplicate commitments from same grantee
        if env.storage().persistent().has(&BidKey::Commitment(grantee.clone())) {
            panic!("Commitment already submitted");
        }

        env.storage().persistent().set(&BidKey::Commitment(grantee.clone()), &commitment);

        let mut commits: Vec<Address> = env
            .storage()
            .instance()
            .get(&BidKey::CommitmentsList)
            .unwrap_or_else(|| Vec::new(&env));
        commits.push_back(grantee);
        env.storage().instance().set(&BidKey::CommitmentsList, &commits);
    }

    /// Grantee reveals bid. Allowed as soon as `sequence >= commit_deadline` (early reveal allowance)
    /// up to `effective_reveal_deadline` (accounting for drift compensation).
    pub fn reveal(env: Env, grantee: Address, bid: RevealedBid) {
        grantee.require_auth();
        Self::internal_reveal(&env, &grantee, &bid);
    }

    /// Batch reveal function to ensure atomicity.
    pub fn batch_reveal(env: Env, reveals: Vec<RevealedBid>, _merkle_root: BytesN<32>) {
        for bid in reveals.iter() {
            Self::internal_reveal(&env, &bid.grantee, &bid);
        }
    }

    fn internal_reveal(env: &Env, grantee: &Address, bid: &RevealedBid) {
        let config: AuctionConfig = env
            .storage()
            .instance()
            .get(&BidKey::Config)
            .expect("Auction not initialized");

        if config.finalized {
            panic!("Auction already finalized");
        }

        let current_seq = env.ledger().sequence();

        // 1. Early reveal allowance: Reveal opens strictly when sequence reaches nominal commit_deadline
        if current_seq < config.commit_deadline {
            panic!("Reveal phase has not opened yet");
        }

        // 2. Compute effective reveal deadline taking drift compensation into account
        let drift_active: bool = env
            .storage()
            .instance()
            .get(&BidKey::DriftCompensationActive)
            .unwrap_or(false);

        let drift_allowance = if drift_active {
            config.phase_clock_drift
        } else {
            config.phase_grace
        };

        let effective_reveal_limit = config
            .reveal_deadline
            .checked_add(drift_allowance)
            .expect("Overflow");

        if current_seq > effective_reveal_limit {
            panic!("Reveal window has expired");
        }

        // 3. Bid amount must respect minimum bid increment
        if bid.amount < bid.min_bid {
            panic!("Revealed bid is below the committed minimum");
        }

        // 4. Commitment hash verification
        let stored_commitment: BytesN<32> = env
            .storage()
            .persistent()
            .get(&BidKey::Commitment(grantee.clone()))
            .expect("No commitment found for this grantee");

        let computed_hash = Self::hash_bid(env, bid);
        if computed_hash != stored_commitment {
            panic!("Revealed bid does not match commitment — possible front-running attempt");
        }

        // 5. Save reveal record
        env.storage().persistent().set(&BidKey::Reveal(grantee.clone()), bid);

        // 6. Insert into ordered sequence
        let mut seq: Vec<RevealedBid> = env
            .storage()
            .instance()
            .get(&BidKey::RevealsSequence)
            .unwrap_or_else(|| Vec::new(env));

        seq.push_back(bid.clone());

        // Sort deterministically based on ordering_hash
        let len = seq.len();
        for i in 0..len {
            for j in 0..len - i - 1 {
                let a = seq.get(j).unwrap();
                let b = seq.get(j + 1).unwrap();

                let hash_a = Self::ordering_hash(env, &a);
                let hash_b = Self::ordering_hash(env, &b);

                if hash_a > hash_b {
                    seq.set(j, b);
                    seq.set(j + 1, a);
                }
            }
        }

        env.storage().instance().set(&BidKey::RevealsSequence, &seq);
    }

    /// Committed bidders vote to attest to network congestion and activate drift compensation.
    pub fn drift_compensation_vote(env: Env, bidder: Address) {
        bidder.require_auth();

        // Must have committed in phase 1
        if !env.storage().persistent().has(&BidKey::Commitment(bidder.clone())) {
            panic!("Only committed bidders can vote for drift compensation");
        }

        let config: AuctionConfig = env
            .storage()
            .instance()
            .get(&BidKey::Config)
            .expect("Auction not initialized");

        if config.finalized {
            panic!("Auction already finalized");
        }

        let mut votes: Vec<Address> = env
            .storage()
            .instance()
            .get(&BidKey::DriftCompensationVotes)
            .unwrap_or_else(|| Vec::new(&env));

        // Prevent double voting
        for existing in votes.iter() {
            if existing == bidder {
                panic!("Already voted for drift compensation");
            }
        }

        votes.push_back(bidder);
        env.storage().instance().set(&BidKey::DriftCompensationVotes, &votes);

        // If at least 2 bidders (or >= 50% of committed bidders) attest to congestion, activate drift window
        let commits: Vec<Address> = env
            .storage()
            .instance()
            .get(&BidKey::CommitmentsList)
            .unwrap_or_else(|| Vec::new(&env));

        let required_votes = if commits.len() <= 2 { 1 } else { commits.len() / 2 };

        if votes.len() >= required_votes {
            env.storage().instance().set(&BidKey::DriftCompensationActive, &true);
            env.events().publish(
                (symbol_short!("drift_wrn"), symbol_short!("comp_actv")),
                (votes.len(), config.phase_clock_drift),
            );
        }
    }

    /// Finalize auction after reveal deadline (plus applicable drift allowance) has passed.
    pub fn finalize_auction(env: Env, admin: Address) -> Option<RevealedBid> {
        admin.require_auth();

        let mut config: AuctionConfig = env
            .storage()
            .instance()
            .get(&BidKey::Config)
            .expect("Auction not initialized");

        if config.finalized {
            panic!("Auction already finalized");
        }

        let drift_active: bool = env
            .storage()
            .instance()
            .get(&BidKey::DriftCompensationActive)
            .unwrap_or(false);

        let drift_allowance = if drift_active {
            config.phase_clock_drift
        } else {
            config.phase_grace
        };

        let effective_reveal_limit = config
            .reveal_deadline
            .checked_add(drift_allowance)
            .expect("Overflow");

        let current_seq = env.ledger().sequence();
        if current_seq <= effective_reveal_limit {
            panic!("Reveal phase is still active; cannot finalize yet");
        }

        config.finalized = true;
        config.is_open = false;
        env.storage().instance().set(&BidKey::Config, &config);

        let reveals: Vec<RevealedBid> = env
            .storage()
            .instance()
            .get(&BidKey::RevealsSequence)
            .unwrap_or_else(|| Vec::new(&env));

        // Select the winning bid (lowest valid cost proposal for grant allocation)
        let mut winner: Option<RevealedBid> = None;
        for bid in reveals.iter() {
            match &winner {
                None => winner = Some(bid),
                Some(current_winner) => {
                    if bid.amount < current_winner.amount {
                        winner = Some(bid);
                    }
                }
            }
        }

        if let Some(ref w) = winner {
            env.storage().instance().set(&BidKey::WinningBid, w);
            env.events().publish(
                (symbol_short!("auction"), symbol_short!("finalized")),
                (w.grantee.clone(), w.amount),
            );
        }

        winner
    }

    /// Read auction configuration.
    pub fn get_config(env: Env) -> AuctionConfig {
        env.storage()
            .instance()
            .get(&BidKey::Config)
            .expect("Auction not initialized")
    }

    /// Check if drift compensation is currently active.
    pub fn is_drift_compensation_active(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&BidKey::DriftCompensationActive)
            .unwrap_or(false)
    }

    /// Read a verified revealed bid.
    pub fn get_revealed_bid(env: Env, grantee: Address) -> RevealedBid {
        env.storage()
            .persistent()
            .get(&BidKey::Reveal(grantee))
            .expect("No verified reveal found")
    }

    /// Read raw commitment.
    pub fn get_commitment(env: Env, grantee: Address) -> BytesN<32> {
        env.storage()
            .persistent()
            .get(&BidKey::Commitment(grantee))
            .expect("No commitment found")
    }

    /// Read all ordered reveals.
    pub fn get_ordered_reveals(env: Env) -> Vec<RevealedBid> {
        env.storage()
            .instance()
            .get(&BidKey::RevealsSequence)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Read winning bid after auction finalization.
    pub fn get_winning_bid(env: Env) -> Option<RevealedBid> {
        env.storage().instance().get(&BidKey::WinningBid)
    }

    // ── Internal Helpers ────────────────────────────────────────────────────

    pub fn hash_bid(env: &Env, bid: &RevealedBid) -> BytesN<32> {
        let mut preimage = Bytes::new(env);

        preimage.append(&Bytes::from_array(env, &bid.amount.to_be_bytes()));
        preimage.append(&Bytes::from_array(env, &bid.min_bid.to_be_bytes()));

        let mut ids: Vec<u32> = Vec::new(env);
        for key in bid.milestone_costs.keys() {
            ids.push_back(key);
        }
        let len = ids.len();
        for i in 0..len {
            for j in 0..len - i - 1 {
                if ids.get(j).unwrap() > ids.get(j + 1).unwrap() {
                    let a = ids.get(j).unwrap();
                    let b = ids.get(j + 1).unwrap();
                    ids.set(j, b);
                    ids.set(j + 1, a);
                }
            }
        }
        for id in ids.iter() {
            preimage.append(&Bytes::from_array(env, &id.to_be_bytes()));
            let cost = bid.milestone_costs.get(id).unwrap();
            preimage.append(&Bytes::from_array(env, &cost.to_be_bytes()));
        }

        preimage.append(&bid.salt);
        preimage.append(&Bytes::from_array(env, &bid.position_nonce.to_be_bytes()));

        env.crypto().sha256(&preimage).into()
    }

    pub fn ordering_hash(env: &Env, bid: &RevealedBid) -> BytesN<32> {
        let mut preimage = Bytes::new(env);
        preimage.append(&bid.salt);
        preimage.append(&Bytes::from_array(env, &bid.position_nonce.to_be_bytes()));
        env.crypto().sha256(&preimage).into()
    }
}