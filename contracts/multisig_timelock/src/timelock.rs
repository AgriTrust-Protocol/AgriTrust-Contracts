use soroban_sdk::{Env, Address};

#[derive(Clone)]
pub struct WithdrawalRequest {
    pub amount: i128,
    pub requester: Address,
    pub start_time: u64,
    pub vetoed: bool,
    pub released: bool,
}

pub fn create_request(env: &Env, requester: Address, amount: i128) -> WithdrawalRequest {
    let now = env.ledger().timestamp();
    let mut req = WithdrawalRequest {
        amount,
        requester: requester.clone(),
        start_time: now,
        vetoed: false,
        released: false,
    };

    if amount <= 5_000 {
        req.released = true;
    }
    req
}

pub fn release(env: &Env, req: &mut WithdrawalRequest) -> bool {
    if req.released || req.vetoed {
        return false;
    }
    let now = env.ledger().timestamp();
    if now >= req.start_time + 48 * 3600 {
        req.released = true;
        return true;
    }
    false
}

pub fn veto(req: &mut WithdrawalRequest) {
    req.vetoed = true;
}