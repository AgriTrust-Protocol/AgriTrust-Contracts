#![allow(dead_code)]
#![allow(unused_variables)]

use ink::prelude::*;
use ink::storage::Mapping;

#[ink::contract]
pub mod awards {
    use super::*;

    #[ink(storage)]
    pub struct AwardContract {
        awards: Mapping<u64, Award>,
        next_award_id: u64,
    }

    #[derive(scale::Encode, scale::Decode, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub struct Award {
        recipient: AccountId,
        total_deposit: Balance,
        initial_payout: Balance,
        remaining_balance: Balance,
        flow_rate_per_second: Balance,
        start_time: u64,
        duration_secs: u64,
    }

    impl AwardContract {
        #[ink(constructor)]
        pub fn new() -> Self {
            Self {
                awards: Mapping::default(),
                next_award_id: 1,
            }
        }

        /// Create an award with an initial payout and a streamed remainder
        #[ink(message, payable)]
        pub fn create_award(
            &mut self,
            recipient: AccountId,
            total_deposit: Balance,
            initial_payout_amount: Balance,
            duration_secs: u64, // e.g., 6 months = 6*30*24*60*60
        ) -> u64 {
            assert!(initial_payout_amount <= total_deposit, "Initial payout exceeds total deposit");

            let remaining_balance = total_deposit - initial_payout_amount;
            let flow_rate_per_second = if remaining_balance > 0 && duration_secs > 0 {
                remaining_balance / duration_secs
            } else {
                0
            };

            // Immediately transfer the initial payout to the recipient
            if initial_payout_amount > 0 {
                assert!(
                    self.env().transfer(recipient, initial_payout_amount).is_ok(),
                    "Initial payout transfer failed"
                );
            }

            let award_id = self.next_award_id;
            self.next_award_id += 1;

            let award = Award {
                recipient,
                total_deposit,
                initial_payout: initial_payout_amount,
                remaining_balance,
                flow_rate_per_second,
                start_time: self.env().block_timestamp(),
                duration_secs,
            };

            self.awards.insert(award_id, &award);

            award_id
        }

        /// View award info
        #[ink(message)]
        pub fn get_award(&self, award_id: u64) -> Option<Award> {
            self.awards.get(award_id)
        }

        /// Compute the currently claimable streamed amount
        #[ink(message)]
        pub fn claimable(&self, award_id: u64) -> Balance {
            if let Some(award) = self.awards.get(award_id) {
                let elapsed = self.env().block_timestamp() - award.start_time;
                let elapsed_secs = elapsed.min(award.duration_secs);
                award.flow_rate_per_second * elapsed_secs
            } else {
                0
            }
        }
    }
}