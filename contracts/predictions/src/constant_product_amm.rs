use soroban_sdk::Vec;

pub const BPS_DENOMINATOR: i128 = 10_000;

pub fn validate_outcomes(outcomes: u32) {
    if !(3..=10).contains(&outcomes) {
        panic!("invalid outcome count");
    }
}

pub fn product(reserves: &Vec<i128>) -> i128 {
    let mut k = 1_i128;
    for reserve in reserves.iter() {
        if reserve <= 0 {
            panic!("empty reserve");
        }
        k = k.checked_mul(reserve).expect("product overflow");
    }
    k
}

pub fn buy_cost(reserves: &Vec<i128>, outcome: u32, shares_out: i128, fee_bps: u32) -> i128 {
    if shares_out <= 0 || outcome >= reserves.len() || reserves.get(outcome).unwrap() <= shares_out
    {
        panic!("invalid buy");
    }
    let k = product(reserves);
    let mut denominator = reserves.get(outcome).unwrap() - shares_out;
    for i in 0..reserves.len() {
        if i != outcome {
            denominator = denominator
                .checked_mul(reserves.get(i).unwrap())
                .expect("denominator overflow");
        }
    }
    let new_quote = ceil_div(k, denominator);
    let raw_cost = new_quote - reserves.get(0).unwrap();
    apply_fee(raw_cost.max(1), fee_bps)
}

pub fn sell_return(reserves: &Vec<i128>, outcome: u32, shares_in: i128, fee_bps: u32) -> i128 {
    if shares_in <= 0 || outcome >= reserves.len() {
        panic!("invalid sell");
    }
    let reserve = reserves.get(outcome).unwrap();
    let raw = shares_in.checked_mul(reserve).expect("return overflow") / (reserve + shares_in);
    remove_fee(raw.max(1), fee_bps)
}

pub fn apply_fee(amount: i128, fee_bps: u32) -> i128 {
    ceil_div(
        amount * (BPS_DENOMINATOR + fee_bps as i128),
        BPS_DENOMINATOR,
    )
}

pub fn remove_fee(amount: i128, fee_bps: u32) -> i128 {
    amount * (BPS_DENOMINATOR - fee_bps as i128) / BPS_DENOMINATOR
}

fn ceil_div(a: i128, b: i128) -> i128 {
    if b <= 0 {
        panic!("bad denominator");
    }
    (a + b - 1) / b
}
