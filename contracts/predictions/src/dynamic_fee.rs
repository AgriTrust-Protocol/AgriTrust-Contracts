use soroban_sdk::{Env, Vec};

pub const MIN_FEE_BPS: u32 = 10;
pub const MAX_FEE_BPS: u32 = 200;
pub const VOL_WINDOW_LEDGERS: u32 = 17_280;

pub fn fee_bps(env: &Env, price_changes_bps: &Vec<i128>) -> u32 {
    if price_changes_bps.len() < 2 {
        return MIN_FEE_BPS;
    }
    let stddev = stddev_bps(price_changes_bps);
    let premium = (stddev / 20) as u32;
    let fee = MIN_FEE_BPS + premium;
    let _ledger_window_start = env.ledger().sequence().saturating_sub(VOL_WINDOW_LEDGERS);
    fee.min(MAX_FEE_BPS).max(MIN_FEE_BPS)
}

fn stddev_bps(values: &Vec<i128>) -> i128 {
    let n = values.len() as i128;
    let mut sum = 0_i128;
    for value in values.iter() {
        sum += value;
    }
    let mean = sum / n;
    let mut variance = 0_i128;
    for value in values.iter() {
        let delta = value - mean;
        variance += delta * delta;
    }
    integer_sqrt(variance / n)
}

fn integer_sqrt(n: i128) -> i128 {
    if n <= 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}
