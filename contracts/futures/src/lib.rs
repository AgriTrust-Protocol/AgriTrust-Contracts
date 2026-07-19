#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String, Vec};

pub const BPS: i128 = 10_000;
pub const INITIAL_MARGIN_BPS: i128 = 1_000;
pub const MAINTENANCE_MARGIN_BPS: i128 = 500;
pub const LIQUIDATION_BONUS_BPS: i128 = 500;
pub const CASH_SETTLEMENT_PENALTY_BPS: i128 = 200;
pub const ORACLE_TWAP_WINDOW_SECONDS: u64 = 3_600;
pub const TICK_BPS: i128 = 100;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FutureSpec {
    pub id: u64,
    pub crop: String,
    pub grade: String,
    pub region: String,
    pub expiry_ledger: u32,
    pub oracle_price: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Position {
    pub owner: Address,
    pub future_id: u64,
    pub tons: i128,
    pub entry_price: i128,
    pub margin: i128,
    pub short: bool,
    pub open: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiquidityPosition {
    pub owner: Address,
    pub future_id: u64,
    pub lower_price: i128,
    pub upper_price: i128,
    pub stable_amount: i128,
    pub future_amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProvenanceNFT {
    pub owner: Address,
    pub future_id: u64,
    pub crop: String,
    pub grade: String,
    pub region: String,
    pub tons: i128,
    pub burned: bool,
}

#[contracttype]
pub enum Key {
    Admin,
    NextFutureId,
    Futures(u64),
    Bal(Address, u64),
    Stable(Address),
    Positions,
    Liquidity,
    Nft(u64),
    OraclePrice(u64),
    OracleTimestamp(u64),
}

#[contract]
pub struct CropFuturesMarket;

#[contractimpl]
impl CropFuturesMarket {
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        if env.storage().instance().has(&Key::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&Key::Admin, &admin);
        env.storage().instance().set(&Key::NextFutureId, &1u64);
        env.storage()
            .instance()
            .set(&Key::Positions, &Vec::<Position>::new(&env));
        env.storage()
            .instance()
            .set(&Key::Liquidity, &Vec::<LiquidityPosition>::new(&env));
    }

    pub fn create_future(
        env: Env,
        admin: Address,
        crop: String,
        grade: String,
        region: String,
        expiry_ledger: u32,
        oracle_price: i128,
    ) -> u64 {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        if oracle_price <= 0 || expiry_ledger <= env.ledger().sequence() {
            panic!("invalid future");
        }
        let id: u64 = env.storage().instance().get(&Key::NextFutureId).unwrap();
        env.storage().instance().set(&Key::NextFutureId, &(id + 1));
        let spec = FutureSpec {
            id,
            crop,
            grade,
            region,
            expiry_ledger,
            oracle_price,
        };
        env.storage().persistent().set(&Key::Futures(id), &spec);
        env.storage()
            .persistent()
            .set(&Key::OraclePrice(id), &oracle_price);
        env.storage()
            .persistent()
            .set(&Key::OracleTimestamp(id), &env.ledger().timestamp());
        id
    }

    pub fn deposit_stable(env: Env, user: Address, amount: i128) {
        user.require_auth();
        if amount <= 0 {
            panic!("invalid amount");
        }
        let bal = Self::stable_balance(env.clone(), user.clone()) + amount;
        env.storage().persistent().set(&Key::Stable(user), &bal);
    }

    pub fn mint_short(env: Env, farmer: Address, future_id: u64, tons: i128) -> u32 {
        farmer.require_auth();
        let price = Self::oracle_price(env.clone(), future_id);
        let required = Self::initial_margin(tons, price);
        Self::debit_stable(&env, &farmer, required);
        Self::add_balance(&env, &farmer, future_id, tons);
        Self::push_position(
            &env,
            Position {
                owner: farmer.clone(),
                future_id,
                tons,
                entry_price: price,
                margin: required,
                short: true,
                open: true,
            },
        )
    }

    pub fn add_liquidity(
        env: Env,
        provider: Address,
        future_id: u64,
        lower_price: i128,
        upper_price: i128,
        stable_amount: i128,
        future_amount: i128,
    ) {
        provider.require_auth();
        let p = Self::oracle_price(env.clone(), future_id);
        if lower_price < p * 5 / 10 || upper_price > p * 2 || lower_price >= upper_price {
            panic!("range outside bounds");
        }
        if lower_price % TICK_BPS != 0 || upper_price % TICK_BPS != 0 {
            panic!("invalid tick");
        }
        Self::debit_stable(&env, &provider, stable_amount);
        Self::sub_balance(&env, &provider, future_id, future_amount);
        let mut liq = Self::liquidity(&env);
        liq.push_back(LiquidityPosition {
            owner: provider,
            future_id,
            lower_price,
            upper_price,
            stable_amount,
            future_amount,
        });
        env.storage().instance().set(&Key::Liquidity, &liq);
    }

    pub fn trade(env: Env, buyer: Address, future_id: u64, tons: i128) {
        buyer.require_auth();
        let price = Self::oracle_price(env.clone(), future_id);
        let cost = tons * price;
        Self::debit_stable(&env, &buyer, cost);
        Self::add_balance(&env, &buyer, future_id, tons);
    }

    pub fn update_oracle_twap(
        env: Env,
        oracle: Address,
        future_id: u64,
        twap_price: i128,
        window_seconds: u64,
    ) {
        oracle.require_auth();
        Self::assert_admin(&env, &oracle);
        if twap_price <= 0 || window_seconds < ORACLE_TWAP_WINDOW_SECONDS {
            panic!("invalid twap");
        }
        env.storage()
            .persistent()
            .set(&Key::OraclePrice(future_id), &twap_price);
        env.storage()
            .persistent()
            .set(&Key::OracleTimestamp(future_id), &env.ledger().timestamp());
    }

    pub fn liquidate(env: Env, liquidator: Address, index: u32) -> i128 {
        liquidator.require_auth();
        let mut positions = Self::read_positions(&env);
        let mut pos = positions.get(index).unwrap();
        if !pos.open {
            panic!("closed");
        }
        let price = Self::oracle_price(env.clone(), pos.future_id);
        let notional = pos.tons * price;
        let equity = if pos.short {
            pos.margin + (pos.entry_price - price) * pos.tons
        } else {
            pos.margin + (price - pos.entry_price) * pos.tons
        };
        if equity * BPS >= notional * MAINTENANCE_MARGIN_BPS {
            panic!("healthy");
        }
        let bonus = pos.margin * LIQUIDATION_BONUS_BPS / BPS;
        pos.open = false;
        pos.margin = 0;
        positions.set(index, pos);
        env.storage().instance().set(&Key::Positions, &positions);
        let bal = Self::stable_balance(env.clone(), liquidator.clone()) + bonus;
        env.storage()
            .persistent()
            .set(&Key::Stable(liquidator), &bal);
        bonus
    }

    pub fn mint_provenance_nft(env: Env, admin: Address, nft_id: u64, nft: ProvenanceNFT) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        env.storage().persistent().set(&Key::Nft(nft_id), &nft);
    }

    pub fn claim_physical(env: Env, holder: Address, future_id: u64, tons: i128, nft_id: u64) {
        holder.require_auth();
        Self::assert_expired(&env, future_id);
        let spec = Self::future(&env, future_id);
        let mut nft: ProvenanceNFT = env.storage().persistent().get(&Key::Nft(nft_id)).unwrap();
        if nft.owner != holder
            || nft.future_id != future_id
            || nft.crop != spec.crop
            || nft.grade != spec.grade
            || nft.region != spec.region
            || nft.tons < tons
            || nft.burned
        {
            panic!("invalid nft");
        }
        Self::sub_balance(&env, &holder, future_id, tons);
        nft.burned = true;
        env.storage().persistent().set(&Key::Nft(nft_id), &nft);
    }

    pub fn cash_settle(env: Env, holder: Address, future_id: u64, tons: i128) -> i128 {
        holder.require_auth();
        Self::assert_expired(&env, future_id);
        Self::sub_balance(&env, &holder, future_id, tons);
        let payout =
            tons * Self::oracle_price(env.clone(), future_id) * (BPS - CASH_SETTLEMENT_PENALTY_BPS)
                / BPS;
        let bal = Self::stable_balance(env.clone(), holder.clone()) + payout;
        env.storage().persistent().set(&Key::Stable(holder), &bal);
        payout
    }

    pub fn balance(env: Env, owner: Address, future_id: u64) -> i128 {
        env.storage()
            .persistent()
            .get(&Key::Bal(owner, future_id))
            .unwrap_or(0)
    }
    pub fn stable_balance(env: Env, owner: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&Key::Stable(owner))
            .unwrap_or(0)
    }
    pub fn positions(env: Env) -> Vec<Position> {
        Self::read_positions(&env)
    }

    fn assert_admin(env: &Env, who: &Address) {
        let admin: Address = env.storage().instance().get(&Key::Admin).unwrap();
        if &admin != who {
            panic!("not admin");
        }
    }
    fn future(env: &Env, id: u64) -> FutureSpec {
        env.storage().persistent().get(&Key::Futures(id)).unwrap()
    }
    fn oracle_price(env: Env, id: u64) -> i128 {
        env.storage()
            .persistent()
            .get(&Key::OraclePrice(id))
            .unwrap()
    }
    fn read_positions(env: &Env) -> Vec<Position> {
        env.storage().instance().get(&Key::Positions).unwrap()
    }
    fn liquidity(env: &Env) -> Vec<LiquidityPosition> {
        env.storage().instance().get(&Key::Liquidity).unwrap()
    }
    fn initial_margin(tons: i128, price: i128) -> i128 {
        tons * price * INITIAL_MARGIN_BPS / BPS
    }
    fn debit_stable(env: &Env, user: &Address, amount: i128) {
        if amount < 0 {
            panic!("invalid amount");
        }
        let bal = Self::stable_balance(env.clone(), user.clone());
        if bal < amount {
            panic!("insufficient stable");
        }
        env.storage()
            .persistent()
            .set(&Key::Stable(user.clone()), &(bal - amount));
    }
    fn add_balance(env: &Env, user: &Address, future_id: u64, amount: i128) {
        if amount <= 0 {
            panic!("invalid amount");
        }
        let bal = Self::balance(env.clone(), user.clone(), future_id) + amount;
        env.storage()
            .persistent()
            .set(&Key::Bal(user.clone(), future_id), &bal);
    }
    fn sub_balance(env: &Env, user: &Address, future_id: u64, amount: i128) {
        if amount < 0 {
            panic!("invalid amount");
        }
        let bal = Self::balance(env.clone(), user.clone(), future_id);
        if bal < amount {
            panic!("insufficient future");
        }
        env.storage()
            .persistent()
            .set(&Key::Bal(user.clone(), future_id), &(bal - amount));
    }
    fn push_position(env: &Env, pos: Position) -> u32 {
        let mut positions = Self::read_positions(env);
        let id = positions.len();
        positions.push_back(pos);
        env.storage().instance().set(&Key::Positions, &positions);
        id
    }
    fn assert_expired(env: &Env, future_id: u64) {
        if env.ledger().sequence() < Self::future(env, future_id).expiry_ledger {
            panic!("not expired");
        }
    }
}

#[cfg(test)]
mod test;
