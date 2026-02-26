#![allow(dead_code)]
#![allow(unused_variables)]

use ink::prelude::*;
use ink::storage::Mapping;

#[ink::contract]
pub mod timed_awards {
    use super::*;

    /// Maximum pause duration in milliseconds (14 days)
    const MAX_PAUSE_DURATION: u64 = 14 * 24 * 60 * 60 * 1000;

    #[ink(storage)]
    pub struct TimedAwardContract {
        awards: Mapping<u64, TimedAward>,
        next_award_id: u64,
    }

    #[derive(scale::Encode, scale::Decode, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub struct TimedAward {
        recipient: AccountId,
        total_deposit: Balance,
        initial_payout: Balance,
        remaining_balance: Balance,
        flow_rate_per_second: Balance,
        start_time: u64,
        duration_secs: u64,
        is_paused: bool,
        pause_timestamp: Option<u64>,
    }

    impl TimedAwardContract {
        #[ink(constructor)]
        pub fn new() -> Self {
            Self {
                awards: Mapping::default(),
                next_award_id: 1,
            }
        }

        /// Create a timed award with an initial payout and streaming remainder
        #[ink(message, payable)]
        pub fn create_award(
            &mut self,
            recipient: AccountId,
            total_deposit: Balance,
            initial_payout_amount: Balance,
            duration_secs: u64,
        ) -> u64 {
            assert!(
                initial_payout_amount <= total_deposit,
                "Initial payout exceeds total deposit"
            );

            let remaining_balance = total_deposit - initial_payout_amount;
            let flow_rate_per_second = if remaining_balance > 0 && duration_secs > 0 {
                remaining_balance / duration_secs
            } else {
                0
            };

            // Transfer initial payout immediately
            if initial_payout_amount > 0 {
                assert!(
                    self.env().transfer(recipient, initial_payout_amount).is_ok(),
                    "Initial payout transfer failed"
                );
            }

            let award_id = self.next_award_id;
            self.next_award_id += 1;

            let award = TimedAward {
                recipient,
                total_deposit,
                initial_payout: initial_payout_amount,
                remaining_balance,
                flow_rate_per_second,
                start_time: self.env().block_timestamp(),
                duration_secs,
                is_paused: false,
                pause_timestamp: None,
            };

            self.awards.insert(award_id, &award);

            award_id
        }

        /// Pause an award stream
        #[ink(message)]
        pub fn pause_award(&mut self, award_id: u64) {
            if let Some(mut award) = self.awards.get(award_id) {
                award.is_paused = true;
                award.pause_timestamp = Some(self.env().block_timestamp());
                self.awards.insert(award_id, &award);
            }
        }

        /// Withdraw available streamed funds
        #[ink(message)]
        pub fn withdraw(&mut self, award_id: u64) -> Balance {
            if let Some(mut award) = self.awards.get(award_id) {
                let current_time = self.env().block_timestamp();

                // Determine if pause is still within max duration
                let effective_paused = award.is_paused
                    && award
                        .pause_timestamp
                        .map_or(false, |ts| current_time < ts + MAX_PAUSE_DURATION);

                let elapsed_secs = if effective_paused {
                    0 // no accrual while actively paused
                } else {
                    // If paused beyond MAX_PAUSE_DURATION, auto-resume
                    current_time.saturating_sub(award.start_time).min(award.duration_secs)
                };

                let claimable_amount = award.flow_rate_per_second * elapsed_secs;

                // Update remaining balance
                award.remaining_balance = award.remaining_balance.saturating_sub(claimable_amount);

                // Auto-resume if pause expired
                if award.is_paused && !effective_paused {
                    award.is_paused = false;
                    award.pause_timestamp = None;
                }

                self.awards.insert(award_id, &award);

                if claimable_amount > 0 {
                    assert!(
                        self.env().transfer(award.recipient, claimable_amount).is_ok(),
                        "Withdrawal transfer failed"
                    );
                }

                claimable_amount
            } else {
                0
            }
        }

        /// View award info
        #[ink(message)]
        pub fn get_award(&self, award_id: u64) -> Option<TimedAward> {
            self.awards.get(award_id)
        }
    }
}