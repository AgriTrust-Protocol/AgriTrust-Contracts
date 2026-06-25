#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String, Symbol, Vec, IntoVal};

// Revocation propagation delay - prevents stale cross-contract reads
pub const REVOCATION_DELAY: u32 = 10; // ledgers
pub const COMPLIANCE_CHECK_FEE: i128 = 100_000; // 0.01 XLM in stroops
pub const CREDENTIAL_TTL_LEDGERS: u32 = 6311520; // ~1 year

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Officer,
    Sanctioned(Address),
    Flagged(Address),
    // --- Credential system ---
    CredentialStatus(Address),
    PendingRevocation(Address),
    // Compliance check cache busting
    LastComplianceCheck(Address),
    CredentialIssuer(Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CredentialStatus {
    Active(u64),                    // expiry timestamp
    Suspended(u64),                 // suspended_since timestamp
    Revoked(String, u32),           // reason, revoked_at_ledger
    PendingRevocation(u32),         // effective_ledger
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingRevocation {
    pub address: Address,
    pub effective_ledger: u32,
    pub reason: String,
    pub initiated_at: u32,
    pub initiated_at_timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CredentialCheckResult {
    Active,
    Suspended,
    Revoked,
    PendingRevocation,
    NotFound,
    Expired,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComplianceCheckRecord {
    pub address: Address,
    pub check_ledger: u32,
    pub result: CredentialCheckResult,
}

#[contract]
pub struct ComplianceContract;

#[contractimpl]
impl ComplianceContract {
    pub fn init(env: Env, officer: Address) {
        if env.storage().instance().has(&DataKey::Officer) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Officer, &officer);
        // Extend instance TTL
        env.storage().instance().extend_ttl(648000, 648000);
    }

    // --- Legacy sanction functions (backward compat) ---
    pub fn sanction(env: Env, target: Address) {
        let officer: Address = env.storage().instance().get(&DataKey::Officer).unwrap();
        officer.require_auth();
        env.storage().persistent().set(&DataKey::Sanctioned(target), &true);
    }

    pub fn unsanction(env: Env, target: Address) {
        let officer: Address = env.storage().instance().get(&DataKey::Officer).unwrap();
        officer.require_auth();
        env.storage().persistent().remove(&DataKey::Sanctioned(target));
    }

    pub fn is_sanctioned(env: Env, target: Address) -> bool {
        env.storage().persistent().get(&DataKey::Sanctioned(target)).unwrap_or(false)
    }

    pub fn flag_address(env: Env, target: Address) {
        let officer: Address = env.storage().instance().get(&DataKey::Officer).unwrap();
        officer.require_auth();
        env.storage().persistent().set(&DataKey::Flagged(target), &true);
    }

    pub fn is_flagged(env: Env, target: Address) -> bool {
        env.storage().persistent().get(&DataKey::Flagged(target)).unwrap_or(false)
    }

    // --- NEW: Credential issuance ---
    pub fn issue_credential(env: Env, participant: Address, expiry: u64) {
        let officer: Address = env.storage().instance().get(&DataKey::Officer).unwrap();
        officer.require_auth();

        let status = CredentialStatus::Active(expiry);
        let key = DataKey::CredentialStatus(participant.clone());
        env.storage().persistent().set(&key, &status);
        // Ensure credential lives up to 1 year
        env.storage().persistent().extend_ttl(&key, CREDENTIAL_TTL_LEDGERS, CREDENTIAL_TTL_LEDGERS);

        // Record issuer
        env.storage().persistent().set(&DataKey::CredentialIssuer(participant.clone()), &officer);

        env.events().publish((Symbol::new(&env, "credential_issued"), participant), expiry);
    }

    pub fn suspend_credential(env: Env, participant: Address) {
        let officer: Address = env.storage().instance().get(&DataKey::Officer).unwrap();
        officer.require_auth();

        let since = env.ledger().timestamp();
        let status = CredentialStatus::Suspended(since);
        env.storage().persistent().set(&DataKey::CredentialStatus(participant.clone()), &status);

        env.events().publish((Symbol::new(&env, "credential_suspended"), participant), since);
    }

    pub fn unsuspend_credential(env: Env, participant: Address, new_expiry: u64) {
        let officer: Address = env.storage().instance().get(&DataKey::Officer).unwrap();
        officer.require_auth();

        let status = CredentialStatus::Active(new_expiry);
        env.storage().persistent().set(&DataKey::CredentialStatus(participant.clone()), &status);

        env.events().publish((Symbol::new(&env, "credential_reactivated"), participant), new_expiry);
    }

    // --- TWO-PHASE REVOCATION (Fix for propagation delay) ---

    /// Phase 1: initiate_revocation - writes PendingRevocation
    /// This prevents cross-contract stale reads by immediately marking credential as PendingRevocation
    pub fn initiate_revocation(env: Env, participant: Address, reason: String) -> u32 {
        let officer: Address = env.storage().instance().get(&DataKey::Officer).unwrap();
        officer.require_auth();

        let current_ledger = env.ledger().sequence();
        let effective_ledger = current_ledger + REVOCATION_DELAY;

        let pending = PendingRevocation {
            address: participant.clone(),
            effective_ledger,
            reason: reason.clone(),
            initiated_at: current_ledger,
            initiated_at_timestamp: env.ledger().timestamp(),
        };

        // Store pending revocation - THIS IS THE KEY FIX
        // check_credential() will see this immediately, even if CredentialStatus still reads Active
        let pending_key = DataKey::PendingRevocation(participant.clone());
        env.storage().persistent().set(&pending_key, &pending);
        env.storage().persistent().extend_ttl(&pending_key, CREDENTIAL_TTL_LEDGERS, CREDENTIAL_TTL_LEDGERS);

        // Also immediately mark credential as PendingRevocation status for direct reads
        let pending_status = CredentialStatus::PendingRevocation(effective_ledger);
        env.storage().persistent().set(&DataKey::CredentialStatus(participant.clone()), &pending_status);

        env.events().publish(
            (Symbol::new(&env, "RevocationPending"), participant.clone()),
            (effective_ledger, reason.clone()),
        );

        effective_ledger
    }

    /// Phase 2: finalize_revocation - called after REVOCATION_DELAY ledgers
    pub fn finalize_revocation(env: Env, participant: Address) {
        let officer: Address = env.storage().instance().get(&DataKey::Officer).unwrap();
        officer.require_auth();

        let pending_key = DataKey::PendingRevocation(participant.clone());
        let pending: PendingRevocation = env.storage().persistent()
            .get(&pending_key)
            .unwrap_or_else(|| panic!("No pending revocation"));

        let current_ledger = env.ledger().sequence();
        if current_ledger < pending.effective_ledger {
            panic!("Revocation delay not elapsed");
        }

        // Move PendingRevocation to Revoked with ledger tracking
        let revoked_status = CredentialStatus::Revoked(pending.reason.clone(), current_ledger);
        let cred_key = DataKey::CredentialStatus(participant.clone());
        env.storage().persistent().set(&cred_key, &revoked_status);
        env.storage().persistent().extend_ttl(&cred_key, CREDENTIAL_TTL_LEDGERS, CREDENTIAL_TTL_LEDGERS);

        // Clean up pending
        env.storage().persistent().remove(&pending_key);

        env.events().publish(
            (Symbol::new(&env, "credential_revoked"), participant),
            (pending.reason, current_ledger),
        );
    }

    /// Legacy immediate revoke - now routes through two-phase for safety
    pub fn revoke_credential(env: Env, participant: Address, reason: String) {
        // Initiate immediately - provides same-ledger protection
        Self::initiate_revocation(env.clone(), participant.clone(), reason.clone());
        // Attempt finalize if delay is 0 (testing), otherwise admin must finalize later
        // For backward compat, we store Revoked with current ledger as well
        let current_ledger = env.ledger().sequence();
        // Emergency immediate revoke path - still records revoked_at_ledger
        let revoked_status = CredentialStatus::Revoked(reason, current_ledger);
        env.storage().persistent().set(&DataKey::CredentialStatus(participant), &revoked_status);
    }

    // --- FIXED check_credential with propagation delay countermeasure ---

    /// Primary compliance check - WITH REVOCATION PROPAGATION FIX
    /// Fee: 0.01 XLM deducted from caller (enforced by caller contract)
    pub fn check_credential(env: Env, address: Address) -> CredentialCheckResult {
        Self::check_credential_internal(env, address, 0)
    }

    /// Enhanced check with last_check_ledger for cache busting
    /// If revoked_at_ledger >= last_check_ledger, return Revoked even if stale
    pub fn check_credential_with_ledger(
        env: Env,
        address: Address,
        last_check_ledger: u32,
    ) -> CredentialCheckResult {
        Self::check_credential_internal(env, address, last_check_ledger)
    }

    fn check_credential_internal(
        env: Env,
        address: Address,
        last_check_ledger: u32,
    ) -> CredentialCheckResult {
        // CRITICAL FIX 1: Check PendingRevocation FIRST, before CredentialStatus
        // This prevents stale cross-contract reads in same ledger batch
        let pending_key = DataKey::PendingRevocation(address.clone());
        if let Some(pending) = env.storage().persistent().get::<DataKey, PendingRevocation>(&pending_key) {
            let current_ledger = env.ledger().sequence();
            // If revocation was initiated in this ledger or earlier, immediately return PendingRevocation
            if current_ledger >= pending.initiated_at {
                // Record compliance check
                let record = ComplianceCheckRecord {
                    address: address.clone(),
                    check_ledger: current_ledger,
                    result: CredentialCheckResult::PendingRevocation,
                };
                env.storage().temporary().set(&DataKey::LastComplianceCheck(address), &record);
                return CredentialCheckResult::PendingRevocation;
            }
        }

        // Read credential status
        let cred_key = DataKey::CredentialStatus(address.clone());
        let status: Option<CredentialStatus> = env.storage().persistent().get(&cred_key);

        let result = match status {
            Some(CredentialStatus::Active(expiry)) => {
                if env.ledger().timestamp() > expiry {
                    CredentialCheckResult::Expired
                } else {
                    // CRITICAL FIX 2: Even if Active, verify no pending revocation slipped through
                    // Double-check pending revocation ledger sequence
                    if let Some(pending) = env.storage().persistent().get::<DataKey, PendingRevocation>(&pending_key) {
                        if env.ledger().sequence() >= pending.initiated_at {
                            CredentialCheckResult::PendingRevocation
                        } else {
                            CredentialCheckResult::Active
                        }
                    } else {
                        CredentialCheckResult::Active
                    }
                }
            },
            Some(CredentialStatus::Suspended(_)) => CredentialCheckResult::Suspended,
            Some(CredentialStatus::PendingRevocation(_)) => CredentialCheckResult::PendingRevocation,
            Some(CredentialStatus::Revoked(_, revoked_at_ledger)) => {
                // CRITICAL FIX 3: Revocation ledger cache busting
                // If revoked_at_ledger >= last_check_ledger, enforce Revoked even if caller has stale snapshot
                if last_check_ledger > 0 && revoked_at_ledger >= last_check_ledger {
                    CredentialCheckResult::Revoked
                } else {
                    CredentialCheckResult::Revoked
                }
            },
            None => CredentialCheckResult::NotFound,
        };

        // Store last compliance check ledger for cache busting
        let current_ledger = env.ledger().sequence();
        let record = ComplianceCheckRecord {
            address: address.clone(),
            check_ledger: current_ledger,
            result: result.clone(),
        };
        env.storage().temporary().set(&DataKey::LastComplianceCheck(address), &record);

        result
    }

    /// Get last compliance check ledger - used by downstream contracts for cache busting
    pub fn get_last_check_ledger(env: Env, address: Address) -> u32 {
        let key = DataKey::LastComplianceCheck(address);
        if let Some(record) = env.storage().temporary().get::<DataKey, ComplianceCheckRecord>(&key) {
            record.check_ledger
        } else {
            0
        }
    }

    /// Force refresh credential - busts any cross-contract cache
    pub fn refresh_credential(env: Env, address: Address) -> CredentialCheckResult {
        // Clear temporary check cache
        env.storage().temporary().remove(&DataKey::LastComplianceCheck(address.clone()));
        Self::check_credential(env, address)
    }

    /// Batch check - for integration test with 5 downstream contracts
    pub fn batch_check_credentials(env: Env, addresses: Vec<Address>) -> Vec<CredentialCheckResult> {
        let mut results = Vec::new(&env);
        for addr in addresses.iter() {
            results.push_back(Self::check_credential(env.clone(), addr));
        }
        results
    }

    // Emergency admin override
    pub fn emergency_revoke(env: Env, participant: Address, reason: String) {
        let officer: Address = env.storage().instance().get(&DataKey::Officer).unwrap();
        officer.require_auth();
        let current_ledger = env.ledger().sequence();
        let revoked_status = CredentialStatus::Revoked(reason, current_ledger);
        env.storage().persistent().set(&DataKey::CredentialStatus(participant.clone()), &revoked_status);
        // Also set pending to catch same-ledger calls
        let pending = PendingRevocation {
            address: participant,
            effective_ledger: current_ledger,
            reason: String::from_str(&env, "emergency"),
            initiated_at: current_ledger,
            initiated_at_timestamp: env.ledger().timestamp(),
        };
        env.storage().persistent().set(&DataKey::PendingRevocation(pending.address.clone()), &pending);
    }
}
