#![no_std]
mod invariants;
pub use invariants::{FinancialState, verify_invariant, withdraw, deposit};