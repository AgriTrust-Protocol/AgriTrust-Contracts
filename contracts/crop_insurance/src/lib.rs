#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, String, Vec,
};

const BPS_DENOMINATOR: i128 = 10_000;
const MIN_CAPITAL_RATIO_BPS: i128 = 11_000;
const REINSURANCE_ATTACHMENT_BPS: i128 = 2_000;
const IDLE_TARGET_BPS: i128 = 8_000;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RiskScore {
    pub farmer: Address,
    pub coverage_amount: i128,
    pub crop_type: String,
    pub region: String,
    pub historical_yield_variance_bps: u32,
    pub risk_multiplier_bps: u32,
    pub valid_until_ledger: u32,
    pub nonce: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Policy {
    pub id: u64,
    pub farmer: Address,
    pub coverage_amount: i128,
    pub premium_paid: i128,
    pub crop_type: String,
    pub region: String,
    pub threshold_yield_bps: u32,
    pub active: bool,
    pub claimed: bool,
}

#[contracttype]
pub enum PoolKey {
    Admin,
    Token,
    Oracle,
    Reinsurance,
    AaveAdapter,
    BaseRateBps,
    NextPolicyId,
    TotalCapital,
    OutstandingRisk,
    IdleDeposited,
    Policies,
    ReinsuranceFundToken,
    ReinsurancePool,
}

#[contract]
pub struct CropInsurancePool;

#[contractimpl]
impl CropInsurancePool {
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        risk_oracle: Address,
        reinsurance_contract: Address,
        aave_adapter: Address,
        base_rate_bps: u32,
    ) {
        admin.require_auth();
        if env.storage().instance().has(&PoolKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&PoolKey::Admin, &admin);
        env.storage().instance().set(&PoolKey::Token, &token);
        env.storage().instance().set(&PoolKey::Oracle, &risk_oracle);
        env.storage()
            .instance()
            .set(&PoolKey::Reinsurance, &reinsurance_contract);
        env.storage()
            .instance()
            .set(&PoolKey::AaveAdapter, &aave_adapter);
        env.storage()
            .instance()
            .set(&PoolKey::BaseRateBps, &base_rate_bps);
        env.storage().instance().set(&PoolKey::NextPolicyId, &1u64);
        env.storage().instance().set(&PoolKey::TotalCapital, &0i128);
        env.storage()
            .instance()
            .set(&PoolKey::OutstandingRisk, &0i128);
        env.storage()
            .instance()
            .set(&PoolKey::IdleDeposited, &0i128);
        env.storage()
            .instance()
            .set(&PoolKey::Policies, &Vec::<Policy>::new(&env));
    }

    pub fn deposit(env: Env, provider: Address, amount: i128) {
        provider.require_auth();
        if amount <= 0 {
            panic!("invalid amount");
        }
        let token_id = Self::token(&env);
        token::Client::new(&env, &token_id).transfer(
            &provider,
            &env.current_contract_address(),
            &amount,
        );
        Self::add_capital(&env, amount);
        env.events()
            .publish((symbol_short!("deposit"), provider), amount);
    }

    pub fn pay_premium(
        env: Env,
        farmer: Address,
        score: RiskScore,
        threshold_yield_bps: u32,
    ) -> u64 {
        farmer.require_auth();
        let oracle = Self::oracle(&env);
        oracle.require_auth();
        if score.farmer != farmer {
            panic!("score farmer mismatch");
        }
        if env.ledger().sequence() > score.valid_until_ledger {
            panic!("risk score expired");
        }
        let premium = Self::quote_premium(env.clone(), score.clone());
        let projected = Self::outstanding(&env) + score.coverage_amount;
        let capital = Self::capital(&env);
        if capital * BPS_DENOMINATOR < projected * MIN_CAPITAL_RATIO_BPS {
            panic!("capital ratio below 110%");
        }
        token::Client::new(&env, &Self::token(&env)).transfer(
            &farmer,
            &env.current_contract_address(),
            &premium,
        );
        Self::add_capital(&env, premium);
        env.storage()
            .instance()
            .set(&PoolKey::OutstandingRisk, &projected);

        let mut policies = Self::policies(&env);
        let id: u64 = env
            .storage()
            .instance()
            .get(&PoolKey::NextPolicyId)
            .unwrap();
        env.storage()
            .instance()
            .set(&PoolKey::NextPolicyId, &(id + 1));
        policies.push_back(Policy {
            id,
            farmer: farmer.clone(),
            coverage_amount: score.coverage_amount,
            premium_paid: premium,
            crop_type: score.crop_type,
            region: score.region,
            threshold_yield_bps,
            active: true,
            claimed: false,
        });
        env.storage().instance().set(&PoolKey::Policies, &policies);
        Self::rebalance_idle(env.clone());
        env.events().publish(
            (symbol_short!("policy"), farmer),
            (id, premium, score.coverage_amount),
        );
        id
    }

    pub fn report_yield_and_payout(
        env: Env,
        oracle: Address,
        crop_type: String,
        region: String,
        observed_yield_bps: u32,
    ) -> i128 {
        oracle.require_auth();
        if oracle != Self::oracle(&env) {
            panic!("unauthorized oracle");
        }
        let mut policies = Self::policies(&env);
        let pool_before = Self::capital(&env);
        let mut total_payout = 0i128;
        let mut outstanding_delta = 0i128;
        let token_id = Self::token(&env);

        for i in 0..policies.len() {
            let mut p = policies.get(i).unwrap();
            if p.active
                && !p.claimed
                && p.crop_type == crop_type
                && p.region == region
                && observed_yield_bps < p.threshold_yield_bps
            {
                let loss_bps = (p.threshold_yield_bps - observed_yield_bps) as i128;
                let payout = (p.coverage_amount * loss_bps) / p.threshold_yield_bps as i128;
                if payout > 0 {
                    total_payout += payout;
                    outstanding_delta += p.coverage_amount;
                    Self::ensure_liquid(&env, payout);
                    token::Client::new(&env, &token_id).transfer(
                        &env.current_contract_address(),
                        &p.farmer,
                        &payout,
                    );
                    p.claimed = true;
                    p.active = false;
                    policies.set(i, p);
                }
            }
        }

        if total_payout > 0 {
            Self::sub_capital(&env, total_payout);
            let attachment = (pool_before * REINSURANCE_ATTACHMENT_BPS) / BPS_DENOMINATOR;
            if total_payout > attachment {
                let excess = total_payout - attachment;
                let reinsurer = Self::reinsurance(&env);
                let rein_client = ReinsuranceModuleClient::new(&env, &reinsurer);
                let recovered = rein_client.cover_excess(&env.current_contract_address(), &excess);
                Self::add_capital(&env, recovered);
            }
            env.storage().instance().set(
                &PoolKey::OutstandingRisk,
                &(Self::outstanding(&env) - outstanding_delta),
            );
        }
        env.storage().instance().set(&PoolKey::Policies, &policies);
        env.events()
            .publish((symbol_short!("payout"), oracle), total_payout);
        total_payout
    }

    pub fn quote_premium(env: Env, score: RiskScore) -> i128 {
        if score.coverage_amount <= 0 {
            panic!("invalid coverage");
        }
        let base_rate_bps: u32 = env.storage().instance().get(&PoolKey::BaseRateBps).unwrap();
        let adjusted_multiplier =
            score.risk_multiplier_bps as i128 + (score.historical_yield_variance_bps as i128 / 10);
        (score.coverage_amount * base_rate_bps as i128 * adjusted_multiplier)
            / (BPS_DENOMINATOR * BPS_DENOMINATOR)
    }

    pub fn rebalance_idle(env: Env) {
        let capital = Self::capital(&env);
        let outstanding = Self::outstanding(&env);
        let reserve = (outstanding * MIN_CAPITAL_RATIO_BPS) / BPS_DENOMINATOR;
        let target_idle = if capital > reserve {
            ((capital - reserve) * IDLE_TARGET_BPS) / BPS_DENOMINATOR
        } else {
            0
        };
        env.storage()
            .instance()
            .set(&PoolKey::IdleDeposited, &target_idle);
        env.events().publish(
            (symbol_short!("aave"), Self::aave_adapter(&env)),
            target_idle,
        );
    }

    pub fn get_policies(env: Env) -> Vec<Policy> {
        Self::policies(&env)
    }
    pub fn get_total_capital(env: Env) -> i128 {
        Self::capital(&env)
    }
    pub fn get_outstanding_risk(env: Env) -> i128 {
        Self::outstanding(&env)
    }
    pub fn get_idle_deposited(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&PoolKey::IdleDeposited)
            .unwrap_or(0)
    }

    fn token(env: &Env) -> Address {
        env.storage().instance().get(&PoolKey::Token).unwrap()
    }
    fn oracle(env: &Env) -> Address {
        env.storage().instance().get(&PoolKey::Oracle).unwrap()
    }
    fn reinsurance(env: &Env) -> Address {
        env.storage().instance().get(&PoolKey::Reinsurance).unwrap()
    }
    fn aave_adapter(env: &Env) -> Address {
        env.storage().instance().get(&PoolKey::AaveAdapter).unwrap()
    }
    fn policies(env: &Env) -> Vec<Policy> {
        env.storage()
            .instance()
            .get(&PoolKey::Policies)
            .unwrap_or(Vec::new(env))
    }
    fn capital(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&PoolKey::TotalCapital)
            .unwrap_or(0)
    }
    fn outstanding(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&PoolKey::OutstandingRisk)
            .unwrap_or(0)
    }
    fn add_capital(env: &Env, amount: i128) {
        env.storage()
            .instance()
            .set(&PoolKey::TotalCapital, &(Self::capital(env) + amount));
    }
    fn sub_capital(env: &Env, amount: i128) {
        env.storage()
            .instance()
            .set(&PoolKey::TotalCapital, &(Self::capital(env) - amount));
    }
    fn ensure_liquid(env: &Env, amount: i128) {
        let idle = Self::get_idle_deposited(env.clone());
        if idle > 0 {
            let withdrawal = if idle > amount { amount } else { idle };
            env.storage()
                .instance()
                .set(&PoolKey::IdleDeposited, &(idle - withdrawal));
        }
    }
}

#[contract]
pub struct ReinsuranceModule;

#[contractimpl]
impl ReinsuranceModule {
    pub fn init_reinsurance(env: Env, admin: Address, token: Address, covered_pool: Address) {
        admin.require_auth();
        env.storage().instance().set(&PoolKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&PoolKey::ReinsuranceFundToken, &token);
        env.storage()
            .instance()
            .set(&PoolKey::ReinsurancePool, &covered_pool);
        env.storage().instance().set(&PoolKey::TotalCapital, &0i128);
    }

    pub fn fund(env: Env, funder: Address, amount: i128) {
        funder.require_auth();
        if amount <= 0 {
            panic!("invalid amount");
        }
        let token_id: Address = env
            .storage()
            .instance()
            .get(&PoolKey::ReinsuranceFundToken)
            .unwrap();
        token::Client::new(&env, &token_id).transfer(
            &funder,
            &env.current_contract_address(),
            &amount,
        );
        let capital: i128 = env
            .storage()
            .instance()
            .get(&PoolKey::TotalCapital)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&PoolKey::TotalCapital, &(capital + amount));
    }

    pub fn cover_excess(env: Env, pool: Address, requested: i128) -> i128 {
        pool.require_auth();
        let covered_pool: Address = env
            .storage()
            .instance()
            .get(&PoolKey::ReinsurancePool)
            .unwrap();
        if pool != covered_pool {
            panic!("unauthorized pool");
        }
        let capital: i128 = env
            .storage()
            .instance()
            .get(&PoolKey::TotalCapital)
            .unwrap_or(0);
        let payout = if requested > capital {
            capital
        } else {
            requested
        };
        if payout > 0 {
            let token_id: Address = env
                .storage()
                .instance()
                .get(&PoolKey::ReinsuranceFundToken)
                .unwrap();
            token::Client::new(&env, &token_id).transfer(
                &env.current_contract_address(),
                &pool,
                &payout,
            );
            env.storage()
                .instance()
                .set(&PoolKey::TotalCapital, &(capital - payout));
        }
        payout
    }

    pub fn get_fund_balance(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&PoolKey::TotalCapital)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod test;
