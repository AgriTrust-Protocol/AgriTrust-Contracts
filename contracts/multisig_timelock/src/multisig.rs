use soroban_sdk::{Env, Address, Map, Vec};

pub struct MultiSig {
    pub dao_members: Vec<Address>,
    pub threshold: u8,
}

impl MultiSig {
    pub fn new(dao_members: Vec<Address>, threshold: u8) -> Self {
        assert!(threshold <= dao_members.len() as u8, "Threshold > members");
        Self { dao_members, threshold }
    }

    pub fn is_valid_member(&self, addr: &Address) -> bool {
        self.dao_members.iter().any(|a| a == addr)
    }

    pub fn check_approval(&self, approvals: &Vec<Address>) -> bool {
        approvals.len() as u8 >= self.threshold
    }
}