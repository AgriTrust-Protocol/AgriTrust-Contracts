pub const MAX_LEVERAGE_BPS: u32 = 100_000;
pub const MIN_MARGIN_BPS: u32 = 1_000;

pub fn borrowed_amount(notional: i128, margin: i128) -> i128 {
    if notional <= 0 || margin <= 0 || margin * (MAX_LEVERAGE_BPS as i128) < notional * 10_000 {
        panic!("exceeds 10x leverage");
    }
    notional - margin
}
