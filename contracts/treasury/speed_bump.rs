use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Vec,
};

/// 1,000 XLM in 7-decimal stroops (1,000 * 10^7 = 10_000_000_000)
pub const SPEED_BUMP_THRESHOLD: i128 = 10_000_000_000;

/// 1,440 ledgers (~2 hours at ~5 seconds per ledger)
pub const SPEED_BUMP_DELAY: u32 = 1440;

/// Rolling window for cumulative threshold tracking (1,440 ledgers)
pub const ROLLING_WINDOW_LEDGERS: u32 = 1440;

// ── Storage Keys ─────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TreasuryKey {
    Admin,
    TokenContract,
    TreasuryEscrow,
    TotalTreasury,
    PendingTransfers,
    PendingReleases,
    AllowedCallers,
    CumulativeReleased,
    WindowStartSeq,
}

// ── Data Types ────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseItem {
    pub id: u64,
    pub recipient: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingRelease {
    pub id: u64,
    pub recipient: Address,
    pub amount: i128,
    pub caller: Address,
    pub created_at_seq: u32,
    pub release_at: u32,
    pub executed: bool,
    pub vetoed: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchReleaseResult {
    pub executed_count: u32,
    pub pending_count: u32,
    pub total_executed_amount: i128,
    pub total_pending_amount: i128,
}

// Backwards compatibility
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingTransfer {
    pub id: u64,
    pub recipient: Address,
    pub amount: u64,
    pub approved_at: u64,
    pub executable_after: u64,
    pub vetoed: bool,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SpeedBumpError {
    Unauthorized = 1,
    NotFound = 2,
    AlreadyExecuted = 3,
    Vetoed = 4,
    SpeedBumpActive = 5,
    InvalidAmount = 6,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct SpeedBumpContract;

#[contractimpl]
impl SpeedBumpContract {
    /// Initialize treasury speed-bump contract.
    pub fn initialize(env: Env, admin: Address, token_contract: Address, treasury_escrow: Address) {
        admin.require_auth();
        env.storage().instance().set(&TreasuryKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&TreasuryKey::TokenContract, &token_contract);
        env.storage()
            .instance()
            .set(&TreasuryKey::TreasuryEscrow, &treasury_escrow);
        env.storage()
            .instance()
            .set(&TreasuryKey::TotalTreasury, &0i128);
        env.storage()
            .instance()
            .set(&TreasuryKey::CumulativeReleased, &0i128);
        env.storage()
            .instance()
            .set(&TreasuryKey::WindowStartSeq, &env.ledger().sequence());
        env.storage().instance().set(
            &TreasuryKey::PendingReleases,
            &Vec::<PendingRelease>::new(&env),
        );
        env.storage().instance().set(
            &TreasuryKey::PendingTransfers,
            &Vec::<PendingTransfer>::new(&env),
        );
        env.storage()
            .instance()
            .set(&TreasuryKey::AllowedCallers, &Vec::<Address>::new(&env));
    }

    /// Add a whitelisted calling contract (e.g. arbitration contract).
    pub fn add_allowed_caller(env: Env, admin: Address, caller: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut callers: Vec<Address> = env
            .storage()
            .instance()
            .get(&TreasuryKey::AllowedCallers)
            .unwrap_or(Vec::new(&env));

        for i in 0..callers.len() {
            if callers.get(i).unwrap() == caller {
                return;
            }
        }
        callers.push_back(caller);
        env.storage()
            .instance()
            .set(&TreasuryKey::AllowedCallers, &callers);
    }

    /// Remove a whitelisted calling contract.
    pub fn remove_allowed_caller(env: Env, admin: Address, caller: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut callers: Vec<Address> = env
            .storage()
            .instance()
            .get(&TreasuryKey::AllowedCallers)
            .unwrap_or(Vec::new(&env));

        for i in 0..callers.len() {
            if callers.get(i).unwrap() == caller {
                callers.remove(i);
                env.storage()
                    .instance()
                    .set(&TreasuryKey::AllowedCallers, &callers);
                return;
            }
        }
    }

    /// Check whether a caller is whitelisted.
    pub fn is_allowed_caller(env: Env, caller: Address) -> bool {
        let callers: Vec<Address> = env
            .storage()
            .instance()
            .get(&TreasuryKey::AllowedCallers)
            .unwrap_or(Vec::new(&env));
        for i in 0..callers.len() {
            if callers.get(i).unwrap() == caller {
                return true;
            }
        }
        false
    }

    /// Get all whitelisted callers.
    pub fn get_allowed_callers(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&TreasuryKey::AllowedCallers)
            .unwrap_or(Vec::new(&env))
    }

    /// Public speed-bump check returning `(requires_delay, release_at_seq)`.
    pub fn check_speed_bump(env: Env, caller: Address, amount: i128) -> (bool, u32) {
        Self::_check_speed_bump(&env, &caller, amount)
    }

    /// Release funds to recipient with speed-bump enforcement.
    /// Returns `true` if immediately executed, `false` if queued for delay.
    pub fn release(env: Env, caller: Address, recipient: Address, amount: i128) -> bool {
        caller.require_auth();
        if amount <= 0 {
            panic!("Amount must be positive");
        }

        let (requires_delay, release_at) = Self::_check_speed_bump(&env, &caller, amount);
        let id = Self::generate_release_id(&env, &recipient, amount);

        if requires_delay {
            Self::queue_pending_release(&env, id, recipient.clone(), amount, caller, release_at);
            env.events().publish(
                (symbol_short!("pending"), id),
                (recipient, amount, release_at),
            );
            false
        } else {
            Self::do_transfer(&env, &recipient, amount);
            Self::record_immediate_release(&env, amount);
            env.events()
                .publish((symbol_short!("released"), id), (recipient, amount));
            true
        }
    }

    /// Cross-contract entrypoint called by arbitration or other contracts.
    pub fn release_with_speedbump(
        env: Env,
        caller: Address,
        recipient: Address,
        amount: i128,
    ) -> bool {
        Self::release(env, caller, recipient, amount)
    }

    /// Batch release: checks speed-bump for EACH item individually.
    /// If any item exceeds the threshold (or cumulative threshold) and hasn't
    /// completed its delay, it is skipped in the batch and queued with `SpeedBumpPending`.
    pub fn batch_release(env: Env, caller: Address, batch: Vec<ReleaseItem>) -> BatchReleaseResult {
        caller.require_auth();

        let mut executed_count = 0u32;
        let mut pending_count = 0u32;
        let mut total_executed_amount = 0i128;
        let mut total_pending_amount = 0i128;

        for i in 0..batch.len() {
            let item = batch.get(i).unwrap();
            if item.amount <= 0 {
                continue;
            }

            let (requires_delay, release_at) = Self::_check_speed_bump(&env, &caller, item.amount);

            if requires_delay {
                Self::queue_pending_release(
                    &env,
                    item.id,
                    item.recipient.clone(),
                    item.amount,
                    caller.clone(),
                    release_at,
                );
                env.events().publish(
                    (symbol_short!("pending"), item.id),
                    (item.recipient, item.amount, release_at),
                );
                pending_count = pending_count.saturating_add(1);
                total_pending_amount = total_pending_amount.saturating_add(item.amount);
            } else {
                Self::do_transfer(&env, &item.recipient, item.amount);
                Self::record_immediate_release(&env, item.amount);
                env.events().publish(
                    (symbol_short!("released"), item.id),
                    (item.recipient, item.amount),
                );
                executed_count = executed_count.saturating_add(1);
                total_executed_amount = total_executed_amount.saturating_add(item.amount);
            }
        }

        BatchReleaseResult {
            executed_count,
            pending_count,
            total_executed_amount,
            total_pending_amount,
        }
    }

    /// Execute a queued pending release once the speed-bump delay has elapsed.
    pub fn execute_pending_release(env: Env, caller: Address, release_id: u64) {
        caller.require_auth();

        let mut releases: Vec<PendingRelease> = env
            .storage()
            .instance()
            .get(&TreasuryKey::PendingReleases)
            .unwrap_or(Vec::new(&env));

        let current_seq = env.ledger().sequence();
        let mut found = false;

        for i in 0..releases.len() {
            let mut rel = releases.get(i).unwrap();
            if rel.id == release_id {
                found = true;
                if rel.executed {
                    panic!("Release already executed");
                }
                if rel.vetoed {
                    panic!("Release has been vetoed");
                }
                if current_seq < rel.release_at {
                    panic!(
                        "Speed bump active — release executable after sequence {}",
                        rel.release_at
                    );
                }

                Self::do_transfer(&env, &rel.recipient, rel.amount);
                rel.executed = true;
                releases.set(i, rel.clone());
                env.storage()
                    .instance()
                    .set(&TreasuryKey::PendingReleases, &releases);

                Self::record_immediate_release(&env, rel.amount);

                env.events().publish(
                    (symbol_short!("released"), release_id),
                    (rel.recipient, rel.amount),
                );
                break;
            }
        }

        if !found {
            panic!("Pending release ID not found");
        }
    }

    /// Admin veto for a pending release during the speed-bump delay.
    pub fn veto_release(env: Env, admin: Address, release_id: u64) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut releases: Vec<PendingRelease> = env
            .storage()
            .instance()
            .get(&TreasuryKey::PendingReleases)
            .unwrap_or(Vec::new(&env));

        for i in 0..releases.len() {
            let mut rel = releases.get(i).unwrap();
            if rel.id == release_id {
                if rel.executed {
                    panic!("Cannot veto already executed release");
                }
                rel.vetoed = true;
                releases.set(i, rel.clone());
                env.storage()
                    .instance()
                    .set(&TreasuryKey::PendingReleases, &releases);
                env.events()
                    .publish((symbol_short!("vetoed"), release_id), (admin, rel.amount));
                return;
            }
        }
        panic!("Pending release ID not found");
    }

    /// View all pending releases.
    pub fn get_pending_releases(env: Env) -> Vec<PendingRelease> {
        env.storage()
            .instance()
            .get(&TreasuryKey::PendingReleases)
            .unwrap_or(Vec::new(&env))
    }

    /// View a pending release by ID.
    pub fn get_pending_release(env: Env, release_id: u64) -> Option<PendingRelease> {
        let releases: Vec<PendingRelease> = env
            .storage()
            .instance()
            .get(&TreasuryKey::PendingReleases)
            .unwrap_or(Vec::new(&env));
        for i in 0..releases.len() {
            let rel = releases.get(i).unwrap();
            if rel.id == release_id {
                return Some(rel);
            }
        }
        None
    }

    /// Get cumulative amount released in current rolling window.
    pub fn get_cumulative_released(env: Env) -> i128 {
        Self::refresh_rolling_window(&env);
        env.storage()
            .instance()
            .get(&TreasuryKey::CumulativeReleased)
            .unwrap_or(0i128)
    }

    /// Reset cumulative window (admin only).
    pub fn reset_cumulative_window(env: Env, admin: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&TreasuryKey::CumulativeReleased, &0i128);
        env.storage()
            .instance()
            .set(&TreasuryKey::WindowStartSeq, &env.ledger().sequence());
    }

    // ── Internal ─────────────────────────────────────────────────────────────

    pub fn _check_speed_bump(env: &Env, caller: &Address, amount: i128) -> (bool, u32) {
        let current_seq = env.ledger().sequence();

        // 1. Cross-contract bypass check: non-whitelisted callers ALWAYS trigger speed bump
        let is_whitelisted = Self::is_caller_whitelisted(env, caller);
        if !is_whitelisted {
            let release_at = current_seq.saturating_add(SPEED_BUMP_DELAY);
            return (true, release_at);
        }

        // 2. Refresh rolling window if window elapsed
        Self::refresh_rolling_window(env);

        let cumulative: i128 = env
            .storage()
            .instance()
            .get(&TreasuryKey::CumulativeReleased)
            .unwrap_or(0i128);

        // 3. Threshold check: single item exceeds threshold OR cumulative exceeds threshold
        if amount > SPEED_BUMP_THRESHOLD || cumulative.saturating_add(amount) > SPEED_BUMP_THRESHOLD
        {
            let release_at = current_seq.saturating_add(SPEED_BUMP_DELAY);
            (true, release_at)
        } else {
            (false, 0)
        }
    }

    fn is_caller_whitelisted(env: &Env, caller: &Address) -> bool {
        let callers: Vec<Address> = env
            .storage()
            .instance()
            .get(&TreasuryKey::AllowedCallers)
            .unwrap_or(Vec::new(env));
        for i in 0..callers.len() {
            if callers.get(i).unwrap() == *caller {
                return true;
            }
        }
        false
    }

    fn refresh_rolling_window(env: &Env) {
        let current_seq = env.ledger().sequence();
        let window_start: u32 = env
            .storage()
            .instance()
            .get(&TreasuryKey::WindowStartSeq)
            .unwrap_or(0);

        if current_seq >= window_start.saturating_add(ROLLING_WINDOW_LEDGERS) {
            env.storage()
                .instance()
                .set(&TreasuryKey::CumulativeReleased, &0i128);
            env.storage()
                .instance()
                .set(&TreasuryKey::WindowStartSeq, &current_seq);
        }
    }

    fn record_immediate_release(env: &Env, amount: i128) {
        Self::refresh_rolling_window(env);
        let cumulative: i128 = env
            .storage()
            .instance()
            .get(&TreasuryKey::CumulativeReleased)
            .unwrap_or(0i128);
        let new_cumulative = cumulative.saturating_add(amount);
        env.storage()
            .instance()
            .set(&TreasuryKey::CumulativeReleased, &new_cumulative);
    }

    fn queue_pending_release(
        env: &Env,
        id: u64,
        recipient: Address,
        amount: i128,
        caller: Address,
        release_at: u32,
    ) {
        let mut releases: Vec<PendingRelease> = env
            .storage()
            .instance()
            .get(&TreasuryKey::PendingReleases)
            .unwrap_or(Vec::new(env));

        releases.push_back(PendingRelease {
            id,
            recipient,
            amount,
            caller,
            created_at_seq: env.ledger().sequence(),
            release_at,
            executed: false,
            vetoed: false,
        });

        env.storage()
            .instance()
            .set(&TreasuryKey::PendingReleases, &releases);
    }

    fn do_transfer(env: &Env, recipient: &Address, amount: i128) {
        let token_address: Address = env
            .storage()
            .instance()
            .get(&TreasuryKey::TokenContract)
            .expect("Token contract not set");

        let token = token::Client::new(env, &token_address);
        let contract_address = env.current_contract_address();
        token.transfer(&contract_address, recipient, &amount);
    }

    fn generate_release_id(env: &Env, _recipient: &Address, amount: i128) -> u64 {
        let seq = env.ledger().sequence() as u64;
        let ts = env.ledger().timestamp();
        let amt_bits = (amount as u64) & 0xFFFFFFFF;
        seq ^ (ts << 16) ^ (amt_bits << 32)
    }

    fn assert_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&TreasuryKey::Admin)
            .expect("Admin not set");
        if *caller != admin {
            panic!("Unauthorized: caller is not admin");
        }
    }
}
