use soroban_sdk::Env;
use crate::invariants::{FinancialState, deposit, withdraw, verify_invariant};

#[test]
fn test_invariant_simple() {
    let mut state = FinancialState {
        total_deposited: 10000,
        total_withdrawn: 0,
        remaining_balance: 10000,
    };

    assert!(verify_invariant(&state));

    assert!(withdraw(&mut state, 3000));
    assert_eq!(state.total_withdrawn, 3000);
    assert_eq!(state.remaining_balance, 7000);
    assert!(verify_invariant(&state));

    assert!(deposit(&mut state, 5000));
    assert_eq!(state.total_deposited, 15000);
    assert_eq!(state.remaining_balance, 12000);
    assert!(verify_invariant(&state));
}

#[test]
fn test_invariant_overwithdraw() {
    let mut state = FinancialState {
        total_deposited: 5000,
        total_withdrawn: 0,
        remaining_balance: 5000,
    };

    assert!(!withdraw(&mut state, 6000)); // Can't withdraw more than remaining
    assert!(verify_invariant(&state)); // Invariant still holds
}