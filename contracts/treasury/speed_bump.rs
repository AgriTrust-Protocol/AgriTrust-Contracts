#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype,
    token, Address, Env, Vec,
};

const DELAY_SECONDS: u64 = 72 * 60 * 60;   // 72 hours
const THRESHOLD_BPS: u64 = 1_000;           // 10% in basis points (10% = 1000/10000)

// ── Storage Keys ─────────────────────────────────────────────────────────────

#[contracttype]
pub enum TreasuryKey {
    TotalTreasury,          // u64 — total treasury balance snapshot
    PendingTransfers,       // Vec<PendingTransfer>
    Admin,                  // Address
    TokenContract,          // Address
}

// ── Data Types ────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct PendingTransfer {
    pub id: u64,
    pub recipient: Address,
    pub amount: u64,
    pub approved_at: u64,       // ledger timestamp when approved
    pub executable_after: u64,  // approved_at + 72h
    pub vetoed: bool,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct SpeedBumpContract;

#[contractimpl]
impl SpeedBumpContract {

    pub fn initialize(env: Env, admin: Address, token_contract: Address, treasury_balance: u64) {
        admin.require_auth();
        env.storage().instance().set(&TreasuryKey::Admin, &admin);
        env.storage().instance().set(&TreasuryKey::TokenContract, &token_contract);
        env.storage().instance().set(&TreasuryKey::TotalTreasury, &treasury_balance);
        env.storage().instance().set(&TreasuryKey::PendingTransfers, &Vec::<PendingTransfer>::new(&env));
    }

    /// Approve a transfer. If it exceeds 10% of treasury, queue it with a 72h delay.
    /// Otherwise execute immediately.
    pub fn approve_transfer(env: Env, admin: Address, recipient: Address, amount: u64) -> bool {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let treasury: u64 = env.storage().instance().get(&TreasuryKey::TotalTreasury).unwrap_or(0);
        let threshold = (treasury * THRESHOLD_BPS) / 10_000;

        if amount > threshold {
            // High-value: queue with speed bump
            Self::queue_transfer(&env, recipient, amount);
            false // not immediately executed
        } else {
            // Low-value: execute immediately
            Self::do_transfer(&env, &recipient, amount);
            true
        }
    }

    /// Execute a queued transfer after the 72-hour window has elapsed.
    pub fn execute_transfer(env: Env, caller: Address, transfer_id: u64) {
        caller.require_auth();
        Self::assert_admin(&env, &caller);

        let mut transfers: Vec<PendingTransfer> = env
            .storage()
            .instance()
            .get(&TreasuryKey::PendingTransfers)
            .unwrap_or(Vec::new(&env));

        let now = env.ledger().timestamp();
        let mut found = false;

        for i in 0..transfers.len() {
            let transfer = transfers.get(i).unwrap();
            if transfer.id == transfer_id {
                found = true;
                if transfer.vetoed {
                    panic!("Transfer has been vetoed");
                }
                if now < transfer.executable_after {
                    panic!(
                        "Speed bump active — transfer executable after {}",
                        transfer.executable_after
                    );
                }
                // Execute and remove from queue
                Self::do_transfer(&env, &transfer.recipient, transfer.amount);
                transfers.remove(i);
                break;
            }
        }

        if !found {
            panic!("Transfer ID not found");
        }

        env.storage().instance().set(&TreasuryKey::PendingTransfers, &transfers);
    }

    /// Veto a pending transfer during the 72-hour window.
    pub fn veto_transfer(env: Env, admin: Address, transfer_id: u64) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut transfers: Vec<PendingTransfer> = env
            .storage()
            .instance()
            .get(&TreasuryKey::PendingTransfers)
            .unwrap_or(Vec::new(&env));

        for i in 0..transfers.len() {
            let mut transfer = transfers.get(i).unwrap();
            if transfer.id == transfer_id {
                if env.ledger().timestamp() >= transfer.executable_after {
                    panic!("Veto window has passed — transfer is already executable");
                }
                transfer.vetoed = true;
                transfers.set(i, transfer);
                env.storage().instance().set(&TreasuryKey::PendingTransfers, &transfers);
                return;
            }
        }
        panic!("Transfer ID not found");
    }

    /// View all pending transfers (for community auditors).
    pub fn get_pending_transfers(env: Env) -> Vec<PendingTransfer> {
        env.storage()
            .instance()
            .get(&TreasuryKey::PendingTransfers)
            .unwrap_or(Vec::new(&env))
    }

    /// Update the treasury balance snapshot (call after deposits/withdrawals).
    pub fn update_treasury_balance(env: Env, admin: Address, new_balance: u64) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        env.storage().instance().set(&TreasuryKey::TotalTreasury, &new_balance);
    }

    // ── Internal ─────────────────────────────────────────────────────────────

    fn queue_transfer(env: &Env, recipient: Address, amount: u64) {
        let mut transfers: Vec<PendingTransfer> = env
            .storage()
            .instance()
            .get(&TreasuryKey::PendingTransfers)
            .unwrap_or(Vec::new(env));

        let now = env.ledger().timestamp();
        let id = now ^ (amount << 8); // simple deterministic ID

        transfers.push_back(PendingTransfer {
            id,
            recipient,
            amount,
            approved_at: now,
            executable_after: now + DELAY_SECONDS,
            vetoed: false,
        });

        env.storage().instance().set(&TreasuryKey::PendingTransfers, &transfers);
    }

    fn do_transfer(env: &Env, recipient: &Address, amount: u64) {
        let token_address: Address = env
            .storage()
            .instance()
            .get(&TreasuryKey::TokenContract)
            .expect("Token contract not set");

        let token = token::Client::new(env, &token_address);
        let contract_address = env.current_contract_address();
        token.transfer(&contract_address, recipient, &(amount as i128));
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

    // ---------------------------------------------------------------------
    // New public APIs for batch execution and external release with speed‑bump
    // ---------------------------------------------------------------------

    /// Release funds to a recipient, applying the speed‑bump logic.
    ///
    /// Returns `true` if the transfer was executed immediately, `false` if it
    /// was queued for later execution.
    pub fn release_with_speedbump(env: Env, caller: Address, recipient: Address, amount: u64) -> bool {
        // Anyone can call this; we only enforce the speed‑bump based on the
        // treasury balance and threshold.
        let treasury: u64 = env.storage().instance().get(&TreasuryKey::TotalTreasury).unwrap_or(0);
        let threshold = (treasury * THRESHOLD_BPS) / 10_000;
        if amount > threshold {
            // High‑value: queue with speed bump
            Self::queue_transfer(&env, recipient, amount);
            false
        } else {
            // Low‑value: execute immediately
            Self::do_transfer(&env, &recipient, amount);
            true
        }
    }

    /// Execute all pending transfers that have passed their delay window.
    /// Skips transfers still under the speed‑bump without failing.
    pub fn batch_execute(env: Env, caller: Address) {
        // Ensure only admin can trigger batch execution (optional – adjust as needed)
        Self::assert_admin(&env, &caller);
        let mut transfers: Vec<PendingTransfer> = env
            .storage()
            .instance()
            .get(&TreasuryKey::PendingTransfers)
            .unwrap_or(Vec::new(&env));
        let now = env.ledger().timestamp();
        // Iterate backwards so we can safely remove items while iterating.
        let mut i: i32 = transfers.len() as i32 - 1;
        while i >= 0 {
            let transfer = transfers.get(i as u32).unwrap();
            if transfer.vetoed {
                // Remove vetoed transfers silently.
                transfers.remove(i as u32);
            } else if now >= transfer.executable_after {
                // Execute and remove.
                Self::do_transfer(&env, &transfer.recipient, transfer.amount);
                transfers.remove(i as u32);
            } else {
                // Still pending – keep it.
            }
            i -= 1;
        }
        env.storage().instance().set(&TreasuryKey::PendingTransfers, &transfers);
    }
}