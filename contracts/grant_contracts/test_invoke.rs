#![no_std]
use soroban_sdk::{Env, Address, Symbol, IntoVal, symbol_short};

pub fn try_call_on_withdraw(env: &Env, recipient: &Address, grant_id: u64, amount: i128) {
    let args = (grant_id, amount).into_val(env);
    let _: Result<Result<(), _>, _> = env.try_invoke_contract(
        recipient,
        &Symbol::new(env, "on_withdraw"),
        args
    );
}
