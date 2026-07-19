#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Symbol,
    Vec,
};

const MAX_STREAMS_PER_RECIPIENT: u32 = 5;
const MAX_VESTING_PER_RECIPIENT: u32 = 3;
const LARGE_WITHDRAWAL_USDC: i128 = 10_000_0000000; // 10,000 USDC at 7 decimals.
const DUST_LIMIT: i128 = 100_000; // 0.01 token at 7 decimals.
const AAVE_LIMIT_BPS: i128 = 3_000;
const UNISWAP_LIMIT_BPS: i128 = 2_000;

#[contract]
pub struct Treasury;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stream {
    pub id: u64,
    pub token: Address,
    pub recipient: Address,
    pub amount_per_second: i128,
    pub start_time: u64,
    pub end_time: u64,
    pub withdrawn: i128,
    pub cancelled: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VestingSchedule {
    pub id: u64,
    pub token: Address,
    pub recipient: Address,
    pub total_amount: i128,
    pub cliff_duration: u64,
    pub vesting_duration: u64,
    pub start_time: u64,
    pub withdrawn: i128,
    pub cancelled: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WithdrawalProposal {
    pub id: u64,
    pub token: Address,
    pub recipient: Address,
    pub amount: i128,
    pub approvals: Vec<Address>,
    pub executed: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YieldPosition {
    pub token: Address,
    pub idle_balance_snapshot: i128,
    pub aave_deposited: i128,
    pub uniswap_deposited: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Key {
    Admin,
    Usdc,
    StreamSeq,
    VestingSeq,
    ProposalSeq,
    Streams(Address),
    Vestings(Address),
    Proposal(u64),
    Owners,
    Council,
    Paused,
    PauseApprovals,
    Yield(Address),
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum TreasuryError {
    NotInitialized = 1,
    Unauthorized = 2,
    InvalidAmount = 3,
    InvalidTimeRange = 4,
    StreamLimit = 5,
    VestingLimit = 6,
    NotFound = 7,
    NothingWithdrawable = 8,
    Paused = 9,
    DuplicateApproval = 10,
    InsufficientApprovals = 11,
    YieldLimitExceeded = 12,
}

#[contractimpl]
impl Treasury {
    pub fn initialize(
        env: Env,
        admin: Address,
        usdc: Address,
        owners: Vec<Address>,
        council: Vec<Address>,
    ) {
        admin.require_auth();
        if owners.len() != 5 || council.len() != 3 {
            panic!("invalid signer set");
        }
        env.storage().instance().set(&Key::Admin, &admin);
        env.storage().instance().set(&Key::Usdc, &usdc);
        env.storage().instance().set(&Key::Owners, &owners);
        env.storage().instance().set(&Key::Council, &council);
        env.storage().instance().set(&Key::Paused, &false);
    }

    pub fn create_stream(
        env: Env,
        admin: Address,
        token: Address,
        recipient: Address,
        amount_per_second: i128,
        start_time: u64,
        end_time: u64,
    ) -> Result<u64, TreasuryError> {
        Self::require_admin(&env, &admin)?;
        Self::require_live(&env)?;
        if amount_per_second <= 0 {
            return Err(TreasuryError::InvalidAmount);
        }
        if start_time >= end_time {
            return Err(TreasuryError::InvalidTimeRange);
        }
        let mut streams = Self::streams(&env, &recipient);
        if streams.len() >= MAX_STREAMS_PER_RECIPIENT {
            return Err(TreasuryError::StreamLimit);
        }
        let id = Self::next(&env, Key::StreamSeq);
        streams.push_back(Stream {
            id,
            token: token.clone(),
            recipient: recipient.clone(),
            amount_per_second,
            start_time,
            end_time,
            withdrawn: 0,
            cancelled: false,
        });
        env.storage()
            .persistent()
            .set(&Key::Streams(recipient.clone()), &streams);
        Self::account(&env, symbol_short!("stream"), token, recipient, 0, id);
        Ok(id)
    }

    pub fn withdraw_accrued(
        env: Env,
        recipient: Address,
        stream_id: u64,
    ) -> Result<i128, TreasuryError> {
        recipient.require_auth();
        Self::require_live(&env)?;
        let mut streams = Self::streams(&env, &recipient);
        for i in 0..streams.len() {
            let mut s = streams.get(i).unwrap();
            if s.id == stream_id && !s.cancelled {
                let due = Self::stream_accrued(&env, &s) - s.withdrawn;
                if due <= 0 {
                    return Err(TreasuryError::NothingWithdrawable);
                }
                s.withdrawn += due;
                streams.set(i, s.clone());
                env.storage()
                    .persistent()
                    .set(&Key::Streams(recipient.clone()), &streams);
                token::Client::new(&env, &s.token).transfer(
                    &env.current_contract_address(),
                    &recipient,
                    &due,
                );
                Self::account(
                    &env,
                    symbol_short!("stream_w"),
                    s.token,
                    recipient,
                    -due,
                    stream_id,
                );
                return Ok(due);
            }
        }
        Err(TreasuryError::NotFound)
    }

    pub fn cancel_stream(
        env: Env,
        admin: Address,
        recipient: Address,
        stream_id: u64,
    ) -> Result<(), TreasuryError> {
        Self::require_admin(&env, &admin)?;
        let mut streams = Self::streams(&env, &recipient);
        for i in 0..streams.len() {
            let mut s = streams.get(i).unwrap();
            if s.id == stream_id {
                s.cancelled = true;
                streams.set(i, s);
                env.storage()
                    .persistent()
                    .set(&Key::Streams(recipient), &streams);
                return Ok(());
            }
        }
        Err(TreasuryError::NotFound)
    }

    pub fn create_vesting(
        env: Env,
        admin: Address,
        token: Address,
        recipient: Address,
        total_amount: i128,
        cliff_duration: u64,
        vesting_duration: u64,
        start_time: u64,
    ) -> Result<u64, TreasuryError> {
        Self::require_admin(&env, &admin)?;
        Self::require_live(&env)?;
        if total_amount <= 0 {
            return Err(TreasuryError::InvalidAmount);
        }
        if vesting_duration == 0 || cliff_duration > vesting_duration {
            return Err(TreasuryError::InvalidTimeRange);
        }
        let mut schedules = Self::vestings(&env, &recipient);
        if schedules.len() >= MAX_VESTING_PER_RECIPIENT {
            return Err(TreasuryError::VestingLimit);
        }
        let id = Self::next(&env, Key::VestingSeq);
        schedules.push_back(VestingSchedule {
            id,
            token: token.clone(),
            recipient: recipient.clone(),
            total_amount,
            cliff_duration,
            vesting_duration,
            start_time,
            withdrawn: 0,
            cancelled: false,
        });
        env.storage()
            .persistent()
            .set(&Key::Vestings(recipient.clone()), &schedules);
        Self::account(&env, symbol_short!("vesting"), token, recipient, 0, id);
        Ok(id)
    }

    pub fn withdraw_vested(
        env: Env,
        recipient: Address,
        schedule_id: u64,
    ) -> Result<i128, TreasuryError> {
        recipient.require_auth();
        Self::require_live(&env)?;
        let mut schedules = Self::vestings(&env, &recipient);
        for i in 0..schedules.len() {
            let mut v = schedules.get(i).unwrap();
            if v.id == schedule_id && !v.cancelled {
                let due = Self::compute_vested_amount(&env, &v) - v.withdrawn;
                if due <= 0 {
                    return Err(TreasuryError::NothingWithdrawable);
                }
                v.withdrawn += due;
                schedules.set(i, v.clone());
                env.storage()
                    .persistent()
                    .set(&Key::Vestings(recipient.clone()), &schedules);
                token::Client::new(&env, &v.token).transfer(
                    &env.current_contract_address(),
                    &recipient,
                    &due,
                );
                Self::account(
                    &env,
                    symbol_short!("vest_w"),
                    v.token,
                    recipient,
                    -due,
                    schedule_id,
                );
                return Ok(due);
            }
        }
        Err(TreasuryError::NotFound)
    }

    pub fn vested_amount(env: Env, schedule: VestingSchedule) -> i128 {
        Self::compute_vested_amount(&env, &schedule)
    }
    pub fn get_streams(env: Env, recipient: Address) -> Vec<Stream> {
        Self::streams(&env, &recipient)
    }
    pub fn get_vestings(env: Env, recipient: Address) -> Vec<VestingSchedule> {
        Self::vestings(&env, &recipient)
    }

    pub fn propose_withdrawal(
        env: Env,
        owner: Address,
        token: Address,
        recipient: Address,
        amount: i128,
    ) -> Result<u64, TreasuryError> {
        Self::require_owner(&env, &owner)?;
        if amount <= 0 {
            return Err(TreasuryError::InvalidAmount);
        }
        let id = Self::next(&env, Key::ProposalSeq);
        let mut approvals = Vec::new(&env);
        approvals.push_back(owner);
        env.storage().persistent().set(
            &Key::Proposal(id),
            &WithdrawalProposal {
                id,
                token,
                recipient,
                amount,
                approvals,
                executed: false,
            },
        );
        Ok(id)
    }

    pub fn approve_withdrawal(
        env: Env,
        owner: Address,
        proposal_id: u64,
    ) -> Result<(), TreasuryError> {
        Self::require_owner(&env, &owner)?;
        let mut p: WithdrawalProposal = env
            .storage()
            .persistent()
            .get(&Key::Proposal(proposal_id))
            .ok_or(TreasuryError::NotFound)?;
        if Self::contains(&p.approvals, &owner) {
            return Err(TreasuryError::DuplicateApproval);
        }
        p.approvals.push_back(owner);
        env.storage()
            .persistent()
            .set(&Key::Proposal(proposal_id), &p);
        Ok(())
    }

    pub fn execute_withdrawal(
        env: Env,
        owner: Address,
        proposal_id: u64,
    ) -> Result<(), TreasuryError> {
        Self::require_owner(&env, &owner)?;
        Self::require_live(&env)?;
        let mut p: WithdrawalProposal = env
            .storage()
            .persistent()
            .get(&Key::Proposal(proposal_id))
            .ok_or(TreasuryError::NotFound)?;
        let usdc: Address = env
            .storage()
            .instance()
            .get(&Key::Usdc)
            .ok_or(TreasuryError::NotInitialized)?;
        if p.token == usdc && p.amount > LARGE_WITHDRAWAL_USDC && p.approvals.len() < 3 {
            return Err(TreasuryError::InsufficientApprovals);
        }
        p.executed = true;
        env.storage()
            .persistent()
            .set(&Key::Proposal(proposal_id), &p);
        token::Client::new(&env, &p.token).transfer(
            &env.current_contract_address(),
            &p.recipient,
            &p.amount,
        );
        Self::account(
            &env,
            symbol_short!("gov_w"),
            p.token,
            p.recipient,
            -p.amount,
            proposal_id,
        );
        Ok(())
    }

    pub fn emergency_pause(env: Env, council_member: Address) -> Result<(), TreasuryError> {
        Self::require_council(&env, &council_member)?;
        let mut approvals: Vec<Address> = env
            .storage()
            .instance()
            .get(&Key::PauseApprovals)
            .unwrap_or(Vec::new(&env));
        if !Self::contains(&approvals, &council_member) {
            approvals.push_back(council_member);
        }
        if approvals.len() >= 2 {
            env.storage().instance().set(&Key::Paused, &true);
        }
        env.storage()
            .instance()
            .set(&Key::PauseApprovals, &approvals);
        Ok(())
    }

    pub fn deposit_aave(
        env: Env,
        admin: Address,
        token: Address,
        idle_balance: i128,
        amount: i128,
    ) -> Result<(), TreasuryError> {
        Self::deposit_yield(
            env,
            admin,
            token,
            idle_balance,
            amount,
            AAVE_LIMIT_BPS,
            true,
        )
    }
    pub fn deposit_uniswap(
        env: Env,
        admin: Address,
        token: Address,
        idle_balance: i128,
        amount: i128,
    ) -> Result<(), TreasuryError> {
        Self::deposit_yield(
            env,
            admin,
            token,
            idle_balance,
            amount,
            UNISWAP_LIMIT_BPS,
            false,
        )
    }
    pub fn sweep(env: Env, admin: Address, token_addr: Address) -> Result<i128, TreasuryError> {
        Self::require_admin(&env, &admin)?;
        let bal = token::Client::new(&env, &token_addr).balance(&env.current_contract_address());
        if bal >= DUST_LIMIT {
            return Err(TreasuryError::InvalidAmount);
        }
        if bal > 0 {
            token::Client::new(&env, &token_addr).transfer(
                &env.current_contract_address(),
                &admin,
                &bal,
            );
            Self::account(&env, symbol_short!("sweep"), token_addr, admin, -bal, 0);
        }
        Ok(bal)
    }

    fn deposit_yield(
        env: Env,
        admin: Address,
        token: Address,
        idle_balance: i128,
        amount: i128,
        limit_bps: i128,
        is_aave: bool,
    ) -> Result<(), TreasuryError> {
        Self::require_admin(&env, &admin)?;
        Self::require_live(&env)?;
        if amount <= 0 || idle_balance <= 0 {
            return Err(TreasuryError::InvalidAmount);
        }
        if amount > idle_balance * limit_bps / 10_000 {
            return Err(TreasuryError::YieldLimitExceeded);
        }
        let mut p: YieldPosition = env
            .storage()
            .persistent()
            .get(&Key::Yield(token.clone()))
            .unwrap_or(YieldPosition {
                token: token.clone(),
                idle_balance_snapshot: idle_balance,
                aave_deposited: 0,
                uniswap_deposited: 0,
            });
        if is_aave {
            p.aave_deposited += amount;
            Self::account(&env, symbol_short!("aave"), token.clone(), admin, amount, 0);
        } else {
            p.uniswap_deposited += amount;
            Self::account(
                &env,
                symbol_short!("uni_v3"),
                token.clone(),
                admin,
                amount,
                0,
            );
        }
        p.idle_balance_snapshot = idle_balance;
        env.storage().persistent().set(&Key::Yield(token), &p);
        Ok(())
    }

    fn stream_accrued(env: &Env, s: &Stream) -> i128 {
        let now = env.ledger().timestamp().min(s.end_time);
        if now <= s.start_time {
            0
        } else {
            i128::from(now - s.start_time) * s.amount_per_second
        }
    }
    fn compute_vested_amount(env: &Env, v: &VestingSchedule) -> i128 {
        let now = env.ledger().timestamp();
        if now < v.start_time + v.cliff_duration {
            0
        } else if now >= v.start_time + v.vesting_duration {
            v.total_amount
        } else {
            v.total_amount * i128::from(now - v.start_time) / i128::from(v.vesting_duration)
        }
    }
    fn streams(env: &Env, r: &Address) -> Vec<Stream> {
        env.storage()
            .persistent()
            .get(&Key::Streams(r.clone()))
            .unwrap_or(Vec::new(env))
    }
    fn vestings(env: &Env, r: &Address) -> Vec<VestingSchedule> {
        env.storage()
            .persistent()
            .get(&Key::Vestings(r.clone()))
            .unwrap_or(Vec::new(env))
    }
    fn next(env: &Env, key: Key) -> u64 {
        let id: u64 = env.storage().instance().get(&key).unwrap_or(0) + 1;
        env.storage().instance().set(&key, &id);
        id
    }
    fn require_admin(env: &Env, caller: &Address) -> Result<(), TreasuryError> {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&Key::Admin)
            .ok_or(TreasuryError::NotInitialized)?;
        if &admin != caller {
            return Err(TreasuryError::Unauthorized);
        }
        Ok(())
    }
    fn require_live(env: &Env) -> Result<(), TreasuryError> {
        if env.storage().instance().get(&Key::Paused).unwrap_or(false) {
            Err(TreasuryError::Paused)
        } else {
            Ok(())
        }
    }
    fn require_owner(env: &Env, caller: &Address) -> Result<(), TreasuryError> {
        caller.require_auth();
        let owners: Vec<Address> = env
            .storage()
            .instance()
            .get(&Key::Owners)
            .ok_or(TreasuryError::NotInitialized)?;
        if Self::contains(&owners, caller) {
            Ok(())
        } else {
            Err(TreasuryError::Unauthorized)
        }
    }
    fn require_council(env: &Env, caller: &Address) -> Result<(), TreasuryError> {
        caller.require_auth();
        let council: Vec<Address> = env
            .storage()
            .instance()
            .get(&Key::Council)
            .ok_or(TreasuryError::NotInitialized)?;
        if Self::contains(&council, caller) {
            Ok(())
        } else {
            Err(TreasuryError::Unauthorized)
        }
    }
    fn contains(v: &Vec<Address>, a: &Address) -> bool {
        for i in 0..v.len() {
            if v.get(i).unwrap() == *a {
                return true;
            }
        }
        false
    }
    fn account(
        env: &Env,
        kind: Symbol,
        token: Address,
        account: Address,
        delta: i128,
        ref_id: u64,
    ) {
        env.events().publish(
            (symbol_short!("acct"), kind),
            (token, account, delta, ref_id, env.ledger().timestamp()),
        );
    }
}

#[cfg(test)]
mod test;
