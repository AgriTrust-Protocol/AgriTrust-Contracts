use soroban_sdk::{Env, Address};

#[derive(Clone)]
pub enum GrantStatus {
    Waiting,
    Active,
    Completed,
    Cancelled,
}

#[derive(Clone)]
pub struct Grant {
    pub id: u32,
    pub owner: Address,
    pub amount: i128,
    pub status: GrantStatus,
}

impl Grant {
    pub fn new(id: u32, owner: Address, amount: i128) -> Self {
        Self {
            id,
            owner,
            amount,
            status: GrantStatus::Waiting,
        }
    }

    pub fn complete(&mut self) {
        self.status = GrantStatus::Completed;
    }

    pub fn activate(&mut self) {
        self.status = GrantStatus::Active;
    }
}