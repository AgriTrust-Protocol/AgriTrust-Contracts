use soroban_sdk::{contracttype, Address, Env};

pub const SETTLEMENT_TIMELOCK_SECONDS: u64 = 86_400;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleReport {
    pub final_yield_bps: u32,
    pub published_timestamp: u64,
}

pub fn report(env: &Env, oracle: &Address, final_yield_bps: u32) -> OracleReport {
    oracle.require_auth();
    OracleReport {
        final_yield_bps,
        published_timestamp: env.ledger().timestamp(),
    }
}

pub fn assert_timelock_elapsed(env: &Env, report: &OracleReport) {
    if env.ledger().timestamp() < report.published_timestamp + SETTLEMENT_TIMELOCK_SECONDS {
        panic!("settlement timelock active");
    }
}
