#![cfg(test)]

use super::scheduler::{
    adjusted_total_amount, effective_cliff_start, CLIFF_PERIOD, VESTING_PERIOD,
};
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_effective_cliff_start_fuzz(
        start_time in 0u64..u64::MAX / 2,
        actual_conversion_time in 0u64..u64::MAX / 2
    ) {
        let result = effective_cliff_start(start_time, actual_conversion_time);
        
        let intended_cliff_end = start_time.saturating_add(CLIFF_PERIOD);
        let max_valid_time = intended_cliff_end.saturating_add(VESTING_PERIOD);
        
        if actual_conversion_time <= intended_cliff_end {
            assert_eq!(result, Some(intended_cliff_end));
        } else if actual_conversion_time >= max_valid_time {
            assert_eq!(result, None);
        } else {
            assert_eq!(result, Some(actual_conversion_time));
        }
    }

    #[test]
    fn test_adjusted_total_amount_fuzz(
        total_amount in 0i128..i128::MAX,
        start_time in 0u64..u64::MAX / 2,
        actual_conversion_time in 0u64..u64::MAX / 2
    ) {
        let result = adjusted_total_amount(total_amount, start_time, actual_conversion_time);
        
        // Assert invariants:
        // 1. Result should never exceed original total amount
        assert!(result <= total_amount);
        
        // 2. Result should be non-negative
        assert!(result >= 0);
        
        let intended_cliff_end = start_time.saturating_add(CLIFF_PERIOD);
        let max_valid_time = intended_cliff_end.saturating_add(VESTING_PERIOD);
        
        if actual_conversion_time <= intended_cliff_end {
            assert_eq!(result, total_amount);
        } else if actual_conversion_time >= max_valid_time {
            assert_eq!(result, 0);
        }
    }
}
