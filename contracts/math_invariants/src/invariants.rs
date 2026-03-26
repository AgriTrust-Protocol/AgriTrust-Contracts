use soroban_sdk::Env;

/// Core financial state for a contract
#[derive(Clone)]
pub struct FinancialState {
    pub total_deposited: i128,
    pub total_withdrawn: i128,
    pub remaining_balance: i128,
}

/// Verify that TotalWithdrawn + RemainingBalance == TotalDeposited
pub fn verify_invariant(state: &FinancialState) -> bool {
    state.total_withdrawn + state.remaining_balance == state.total_deposited
}

/// Update financial state safely
pub fn withdraw(state: &mut FinancialState, amount: i128) -> bool {
    if amount > state.remaining_balance {
        return false;
    }
    state.total_withdrawn += amount;
    state.remaining_balance -= amount;

    // Check invariant after withdrawal
    verify_invariant(state)
}

pub fn deposit(state: &mut FinancialState, amount: i128) -> bool {
    state.total_deposited += amount;
    state.remaining_balance += amount;

    verify_invariant(state)
}