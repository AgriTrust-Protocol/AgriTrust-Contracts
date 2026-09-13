#![no_std]
#![allow(deprecated)]
#[cfg(test)]
extern crate std;

#[path = "../speed_bump.rs"]
pub mod speed_bump;

#[cfg(test)]
#[path = "../speed_bump_test.rs"]
mod speed_bump_test;

pub use speed_bump::{
    BatchReleaseResult, PendingRelease, ReleaseItem, SpeedBumpContract, SpeedBumpContractClient,
    SpeedBumpError, SPEED_BUMP_DELAY, SPEED_BUMP_THRESHOLD,
};
