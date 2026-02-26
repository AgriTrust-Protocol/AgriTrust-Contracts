use grant_stream::DripContract; // adjust to your actual contract module
use proptest::prelude::*;

// Helper enum for randomized actions
#[derive(Debug, Clone)]
enum Action {
    Pause,
    Resume,
    Withdraw(u128),
    ChangeRate(u128),
}

// Property-based test
proptest! {
    #[test]
    fn drip_invariants(actions in prop::collection::vec(
        prop_oneof![
            Just(Action::Pause),
            Just(Action::Resume),
            (1u128..1000u128).prop_map(Action::Withdraw),
            (1u128..100u128).prop_map(Action::ChangeRate),
        ],
        1..50 // number of actions in the sequence
    )) {
        // Initialize the contract with a random deposit
        let initial_deposit = 10000u128;
        let mut drip = DripContract::new(initial_deposit, 10); // 10 tokens per block rate, example
        let mut total_withdrawn = 0u128;
        let mut accrued_while_paused = 0u128;
        let mut is_paused = false;

        for action in actions {
            match action {
                Action::Pause => {
                    drip.pause();
                    is_paused = true;
                }
                Action::Resume => {
                    drip.resume();
                    is_paused = false;
                }
                Action::Withdraw(amount) => {
                    let withdrawn = drip.withdraw(amount);
                    total_withdrawn += withdrawn;
                }
                Action::ChangeRate(new_rate) => {
                    drip.set_rate(new_rate);
                }
            }

            // Assert that during paused state, accrued doesn't increase
            if is_paused {
                let accrued = drip.accrued_balance();
                prop_assert_eq!(accrued, accrued_while_paused);
            } else {
                accrued_while_paused = drip.accrued_balance();
            }

            // Assert invariant: total withdrawn + remaining balance == initial deposit
            let remaining = drip.remaining_balance();
            prop_assert_eq!(total_withdrawn + remaining, initial_deposit);
        }
    }
}