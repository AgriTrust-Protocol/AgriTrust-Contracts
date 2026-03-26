use crate::grant::{Grant, GrantStatus};
use soroban_sdk::Env;

/// Conditional funding trigger: only start `dependent` grant if `prereq` grant is Completed
pub fn trigger(env: &Env, prereq: &Grant, dependent: &mut Grant) -> bool {
    match prereq.status {
        GrantStatus::Completed => {
            dependent.activate();
            true
        },
        _ => false, // Still waiting for prerequisite grant to complete
    }
}