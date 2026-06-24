//! Treasury contract library

#[path = "../speed_bump.rs"]
pub mod speed_bump;

pub use speed_bump::*;

#[cfg(test)]
#[path = "../speed_bump_test.rs"]
mod speed_bump_test;
