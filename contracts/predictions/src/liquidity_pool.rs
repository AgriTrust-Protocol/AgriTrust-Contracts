use soroban_sdk::{contracttype, Address, Env, Map};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiquidityState {
    pub total_lp_shares: i128,
    pub fee_pool: i128,
    pub balances: Map<Address, i128>,
}

impl LiquidityState {
    pub fn new(env: &Env) -> Self {
        Self {
            total_lp_shares: 0,
            fee_pool: 0,
            balances: Map::new(env),
        }
    }

    pub fn deposit(&mut self, provider: Address, amount_per_outcome: i128, outcomes: u32) -> i128 {
        provider.require_auth();
        if amount_per_outcome <= 0 {
            panic!("invalid liquidity");
        }
        let minted = amount_per_outcome * outcomes as i128;
        self.total_lp_shares += minted;
        self.balances
            .set(provider.clone(), self.balance(&provider) + minted);
        minted
    }

    pub fn withdraw(&mut self, provider: Address, lp_shares: i128) -> i128 {
        provider.require_auth();
        let balance = self.balance(&provider);
        if lp_shares <= 0 || lp_shares > balance {
            panic!("invalid withdraw");
        }
        let fees = self.fee_pool * lp_shares / self.total_lp_shares;
        self.fee_pool -= fees;
        self.total_lp_shares -= lp_shares;
        self.balances.set(provider.clone(), balance - lp_shares);
        lp_shares + fees
    }

    pub fn accrue_fee(&mut self, fee: i128) {
        self.fee_pool += fee;
    }

    pub fn balance(&self, provider: &Address) -> i128 {
        self.balances.get(provider.clone()).unwrap_or(0)
    }
}
