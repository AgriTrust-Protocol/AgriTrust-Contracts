use soroban_sdk::{Env, Address, Vec};

#[derive(Clone)]
pub struct EmergencyStop {
    pub is_paused: bool,
    pub pause_start: u64,
    pub pause_duration: u64, // e.g., 7 days in seconds
    pub auditors: Vec<Address>,
    pub signatures: Vec<Address>,
}

impl EmergencyStop {
    pub fn new(auditors: Vec<Address>, pause_duration: u64) -> Self {
        assert_eq!(auditors.len(), 3, "Require exactly 3 auditors");
        Self {
            is_paused: false,
            pause_start: 0,
            pause_duration,
            auditors,
            signatures: Vec::new(),
        }
    }

    pub fn sign_pause(&mut self, env: &Env, auditor: Address) -> bool {
        assert!(self.auditors.iter().any(|a| a == &auditor), "Not an auditor");
        if !self.signatures.iter().any(|a| a == &auditor) {
            self.signatures.push(auditor);
        }

        if self.signatures.len() >= 2 {
            self.is_paused = true;
            self.pause_start = env.ledger().timestamp();
            self.signatures.clear();
            return true;
        }
        false
    }

    pub fn resume_if_expired(&mut self, env: &Env) {
        if self.is_paused {
            let now = env.ledger().timestamp();
            if now >= self.pause_start + self.pause_duration {
                self.is_paused = false;
            }
        }
    }

    pub fn check_paused(&self, env: &Env) -> bool {
        if self.is_paused {
            let now = env.ledger().timestamp();
            if now >= self.pause_start + self.pause_duration {
                return false;
            }
            return true;
        }
        false
    }
}