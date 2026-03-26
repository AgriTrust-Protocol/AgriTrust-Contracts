#![no_std]
use soroban_sdk::{contractimpl, Env, Address, Vec};
mod timelock;
mod multisig;

use timelock::{WithdrawalRequest, create_request, release, veto};
use multisig::MultiSig;

pub struct MultiSigTimelock;

#[contractimpl]
impl MultiSigTimelock {
    pub fn propose_withdrawal(env: Env, requester: Address, amount: i128) -> WithdrawalRequest {
        let req = create_request(&env, requester.clone(), amount);
        env.storage().set(&requester, &req);
        req
    }

    pub fn veto_withdrawal(env: Env, dao_member: Address, requester: Address) {
        let mut req: WithdrawalRequest = env.storage().get(&requester).unwrap();

        // For demo: assume single DAO member can veto, later replace with MultiSig logic
        veto(&mut req);
        env.storage().set(&requester, &req);
    }

    pub fn release_withdrawal(env: Env, requester: Address) -> bool {
        let mut req: WithdrawalRequest = env.storage().get(&requester).unwrap();
        let released = release(&env, &mut req);
        env.storage().set(&requester, &req);
        released
    }
}