#![no_std]
mod auditor;
pub use auditor::EmergencyStop;

use soroban_sdk::{contractimpl, Env, Address};

pub struct EmergencyStopper;

#[contractimpl]
impl EmergencyStopper {
    pub fn init_emergency_stop(env: Env, auditors: Vec<Address>, pause_duration: u64) -> EmergencyStop {
        EmergencyStop::new(auditors, pause_duration)
    }

    pub fn sign_pause(env: Env, mut stop: EmergencyStop, auditor: Address) -> EmergencyStop {
        stop.sign_pause(&env, auditor);
        stop
    }

    pub fn resume_if_expired(env: Env, mut stop: EmergencyStop) -> EmergencyStop {
        stop.resume_if_expired(&env);
        stop
    }

    pub fn is_paused(env: Env, stop: EmergencyStop) -> bool {
        stop.check_paused(&env)
    }
}