use soroban_sdk::contracttype;

#[derive(Clone)]
#[contracttype]
pub enum GrantStatus {
    Active,
    Completed,
    Cancelled,
}

#[derive(Clone)]
#[contracttype]
pub struct Grant {
    pub status: GrantStatus,
    pub remaining_balance: i128,
    pub withdrawable_balance: i128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum GrantError {
    GrantNotFound = 1,
    InvalidStatus = 2,
    NonZeroBalance = 3,
}

#[derive(Clone)]
#[contracttype]
pub struct GrantTombstone {
    pub original_grant: Grant,
    pub archived_at_ledger: u32,
    pub archive_version: u32,
}