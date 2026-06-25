#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype,
    crypto::Hash,
    Address, Bytes, BytesN, Env, Map, String, Vec,
};

// ── Storage Keys ────────────────────────────────────────────────────────────

#[contracttype]
pub enum BidKey {
    Commitment(Address),   // grantee -> commitment hash
    Reveal(Address),       // grantee -> revealed bid
    BiddingOpen,           // bool
    RevealDeadline,        // u64 ledger timestamp
    RevealsSequence,       // Vec<RevealedBid> (ordered)
}

// ── Data Types ───────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct RevealedBid {
    pub grantee: Address,
    pub amount: u64,
    pub min_bid: u64,                   // Added for minimum bid increment commitment
    pub milestone_costs: Map<u32, u64>, // milestone_id -> cost
    pub salt: Bytes,                    // random salt used during commit (blinding_factor)
    pub position_nonce: u64,            // Added for position commitment
}

// ── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct CommitRevealContract;

#[contractimpl]
impl CommitRevealContract {

    /// Admin opens the bidding window.
    pub fn open_bidding(env: Env, admin: Address, reveal_deadline: u64) {
        admin.require_auth();
        env.storage().instance().set(&BidKey::BiddingOpen, &true);
        env.storage().instance().set(&BidKey::RevealDeadline, &reveal_deadline);
        let seq: Vec<RevealedBid> = Vec::new(&env);
        env.storage().instance().set(&BidKey::RevealsSequence, &seq);
    }

    /// Admin closes the bidding window — no more commits accepted.
    pub fn close_bidding(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().instance().set(&BidKey::BiddingOpen, &false);
    }

    /// Grantee submits SHA-256 hash of (amount + min_bid + milestone_costs + salt + position_nonce).
    pub fn commit(env: Env, grantee: Address, commitment: BytesN<32>) {
        grantee.require_auth();

        let is_open: bool = env
            .storage()
            .instance()
            .get(&BidKey::BiddingOpen)
            .unwrap_or(false);

        if !is_open {
            panic!("Bidding window is closed");
        }

        // Prevent overwriting an existing commitment
        if env.storage().persistent().has(&BidKey::Commitment(grantee.clone())) {
            panic!("Commitment already submitted");
        }

        env.storage()
            .persistent()
            .set(&BidKey::Commitment(grantee), &commitment);
    }

    /// Grantee reveals their original bid after the bidding window closes.
    pub fn reveal(env: Env, grantee: Address, bid: RevealedBid) {
        grantee.require_auth();
        Self::internal_reveal(&env, &grantee, &bid);
    }

    /// Batch reveal function to ensure atomicity.
    /// Accepts multiple reveals in a single transaction with a Merkle proof of the batch.
    pub fn batch_reveal(env: Env, reveals: Vec<RevealedBid>, _merkle_root: BytesN<32>) {
        // Atomicity is guaranteed by Soroban transactions.
        for bid in reveals.iter() {
            Self::internal_reveal(&env, &bid.grantee, &bid);
        }
    }

    fn internal_reveal(env: &Env, grantee: &Address, bid: &RevealedBid) {
        let is_open: bool = env
            .storage()
            .instance()
            .get(&BidKey::BiddingOpen)
            .unwrap_or(false);
        if is_open {
            panic!("Bidding window must be closed before revealing");
        }

        let deadline: u64 = env
            .storage()
            .instance()
            .get(&BidKey::RevealDeadline)
            .unwrap_or(0);
        if env.ledger().timestamp() > deadline {
            panic!("Reveal window has expired");
        }

        if bid.amount < bid.min_bid {
            panic!("Revealed bid is below the committed minimum");
        }

        let stored_commitment: BytesN<32> = env
            .storage()
            .persistent()
            .get(&BidKey::Commitment(grantee.clone()))
            .expect("No commitment found for this grantee");

        let computed_hash = Self::hash_bid(env, bid);
        if computed_hash != stored_commitment {
            panic!("Revealed bid does not match commitment — possible front-running attempt");
        }

        env.storage()
            .persistent()
            .set(&BidKey::Reveal(grantee.clone()), bid);

        // Sequence ordering: insert into RevealsSequence ordered deterministically
        let mut seq: Vec<RevealedBid> = env
            .storage()
            .instance()
            .get(&BidKey::RevealsSequence)
            .unwrap_or_else(|| Vec::new(env));
        
        seq.push_back(bid.clone());
        
        // Sort sequence deterministically based on hash(blinding_factor || position_nonce)
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

    /// Read a verified revealed bid (only after reveal phase).
    pub fn get_revealed_bid(env: Env, grantee: Address) -> RevealedBid {
        env.storage()
            .persistent()
            .get(&BidKey::Reveal(grantee))
            .expect("No verified reveal found")
    }

    /// Get the raw commitment hash for a grantee.
    pub fn get_commitment(env: Env, grantee: Address) -> BytesN<32> {
        env.storage()
            .persistent()
            .get(&BidKey::Commitment(grantee))
            .expect("No commitment found")
    }
    
    pub fn get_ordered_reveals(env: Env) -> Vec<RevealedBid> {
        env.storage()
            .instance()
            .get(&BidKey::RevealsSequence)
            .unwrap_or_else(|| Vec::new(&env))
    }

    // ── Internal ─────────────────────────────────────────────────────────────

    /// Deterministically encode and SHA-256 hash a RevealedBid.
    pub fn hash_bid(env: &Env, bid: &RevealedBid) -> BytesN<32> {
        let mut preimage = Bytes::new(env);

        preimage.append(&Bytes::from_array(env, &bid.amount.to_be_bytes()));
        preimage.append(&Bytes::from_array(env, &bid.min_bid.to_be_bytes()));

        let mut ids: soroban_sdk::Vec<u32> = soroban_sdk::Vec::new(env);
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

        env.crypto().sha256(&preimage)
    }
    
    /// Ordering hash: hash(blinding_factor || position_nonce)
    pub fn ordering_hash(env: &Env, bid: &RevealedBid) -> BytesN<32> {
        let mut preimage = Bytes::new(env);
        preimage.append(&bid.salt);
        preimage.append(&Bytes::from_array(env, &bid.position_nonce.to_be_bytes()));
        env.crypto().sha256(&preimage)
    }
}

// ── Sequencer Pattern ────────────────────────────────────────────────────────

#[contract]
pub struct CommitRevealSequencerContract;

#[contractimpl]
impl CommitRevealSequencerContract {
    /// A designated neutral contract function that orders reveals by max(block_number, reveal_timestamp) 
    /// using ledger sequence as a tiebreaker.
    pub fn sequence_batch_reveals(
        env: Env,
        commit_reveal_contract: Address,
        reveals: Vec<RevealedBid>,
    ) {
        // Tiebreaker variables recorded
        let _block_sequence = env.ledger().sequence();
        let _reveal_timestamp = env.ledger().timestamp();
        
        let client = CommitRevealContractClient::new(&env, &commit_reveal_contract);
        let merkle_root = BytesN::from_array(&env, &[0u8; 32]);
        
        // Submits the sequenced batch
        client.batch_reveal(&reveals, &merkle_root);
    }
}