// contracts/grant_multisig/multisig.rs

use std::collections::HashSet;

/// Mock multi-sig / Smart Contract Wallet
/// Simulates a contract wallet that approves calls if the caller is authorized
#[derive(Debug)]
pub struct MockMultisig {
    authorized: HashSet<String>, // set of addresses authorized to execute
}

impl MockMultisig {
    /// Create a new MockMultisig with a list of authorized addresses
    pub fn new(auth_list: Vec<&str>) -> Self {
        let authorized: HashSet<String> = auth_list.into_iter().map(|a| a.to_string()).collect();
        Self { authorized }
    }

    /// Simulate execution by an external caller
    /// Returns true if caller is authorized, false otherwise
    pub fn execute(&self, caller: &str) -> bool {
        self.authorized.contains(caller)
    }

    /// Optional helper to add an authorized signer
    pub fn add_authorized(&mut self, caller: &str) {
        self.authorized.insert(caller.to_string());
    }

    /// Optional helper to remove an authorized signer
    pub fn remove_authorized(&mut self, caller: &str) {
        self.authorized.remove(caller);
    }

    /// Check if this is a contract (always true for multi-sig)
    pub fn is_contract(&self) -> bool {
        true
    }
}