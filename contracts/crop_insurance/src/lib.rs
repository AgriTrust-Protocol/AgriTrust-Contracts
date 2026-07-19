#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, String, Vec,
};

const BPS_DENOMINATOR: i128 = 10_000;
const MIN_CAPITAL_RATIO_BPS: i128 = 11_000;
const REINSURANCE_ATTACHMENT_BPS: i128 = 2_000;
const IDLE_TARGET_BPS: i128 = 8_000;
const PARAMETRIC_ORACLE_COUNT: u32 = 5;
const ORACLE_OUTLIER_MAD_MULTIPLIER: i128 = 3;
const DISPUTE_WINDOW_LEDGERS: u32 = 120_960;
const DISPUTE_BOND_BPS: i128 = 1_000;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WeatherMetric {
    Rainfall30d,
    AvgTemp,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Comparator {
    Lt,
    Gt,
    Le,
    Ge,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TriggerNode {
    Condition(WeatherMetric, Comparator, i128),
    And(u32, u32),
    Or(u32, u32),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeatherReport {
    pub oracle: Address,
    pub rainfall_30d_mm: i128,
    pub avg_temp_c: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AggregatedWeather {
    pub rainfall_30d_mm: i128,
    pub avg_temp_c: i128,
    pub accepted_reports: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParametricPolicy {
    pub id: u64,
    pub farmer_id: u64,
    pub season_id: u64,
    pub farmer: Address,
    pub crop_type: String,
    pub coverage_amount: i128,
    pub deductible: i128,
    pub premium: i128,
    pub trigger_nodes: Vec<TriggerNode>,
    pub root_node: u32,
    pub oracles: Vec<Address>,
    pub active: bool,
    pub paid_out: bool,
    pub payout_amount: i128,
    pub payout_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dispute {
    pub id: u64,
    pub policy_id: u64,
    pub disputer: Address,
    pub oracle: Address,
    pub bond: i128,
    pub resolved: bool,
    pub successful: bool,
}

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
    Treasury,
    ParametricPolicies,
    Reports(u64),
    OracleBonds,
    Disputes,
    NextDisputeId,
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
        env.storage().instance().set(&PoolKey::Treasury, &admin);
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
        env.storage().instance().set(
            &PoolKey::ParametricPolicies,
            &Vec::<ParametricPolicy>::new(&env),
        );
        env.storage()
            .instance()
            .set(&PoolKey::OracleBonds, &Vec::<(Address, i128)>::new(&env));
        env.storage()
            .instance()
            .set(&PoolKey::Disputes, &Vec::<Dispute>::new(&env));
        env.storage().instance().set(&PoolKey::NextDisputeId, &1u64);
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

    pub fn create_parametric_policy(
        env: Env,
        farmer: Address,
        farmer_id: u64,
        season_id: u64,
        crop_type: String,
        coverage_amount: i128,
        deductible: i128,
        premium: i128,
        trigger_nodes: Vec<TriggerNode>,
        root_node: u32,
        oracles: Vec<Address>,
    ) -> u64 {
        farmer.require_auth();
        if coverage_amount <= 0 || premium <= 0 || deductible < 0 || deductible >= coverage_amount {
            panic!("invalid policy amounts");
        }
        if oracles.len() != PARAMETRIC_ORACLE_COUNT {
            panic!("requires 5 oracles");
        }
        if root_node >= trigger_nodes.len() {
            panic!("invalid trigger root");
        }
        let token_id = Self::token(&env);
        token::Client::new(&env, &token_id).transfer(
            &farmer,
            &env.current_contract_address(),
            &premium,
        );
        let cover_pool_share = (premium * 8_000) / BPS_DENOMINATOR;
        let oracle_share = (premium * 1_000) / BPS_DENOMINATOR;
        let treasury_share = premium - cover_pool_share - oracle_share;
        Self::add_capital(&env, cover_pool_share);
        let treasury = Self::treasury(&env);
        token::Client::new(&env, &token_id).transfer(
            &env.current_contract_address(),
            &treasury,
            &treasury_share,
        );
        let per_oracle = oracle_share / PARAMETRIC_ORACLE_COUNT as i128;
        for i in 0..oracles.len() {
            token::Client::new(&env, &token_id).transfer(
                &env.current_contract_address(),
                &oracles.get(i).unwrap(),
                &per_oracle,
            );
        }
        let mut policies = Self::parametric_policies(&env);
        let id: u64 = env
            .storage()
            .instance()
            .get(&PoolKey::NextPolicyId)
            .unwrap();
        env.storage()
            .instance()
            .set(&PoolKey::NextPolicyId, &(id + 1));
        policies.push_back(ParametricPolicy {
            id,
            farmer_id,
            season_id,
            farmer: farmer.clone(),
            crop_type,
            coverage_amount,
            deductible,
            premium,
            trigger_nodes,
            root_node,
            oracles,
            active: true,
            paid_out: false,
            payout_amount: 0,
            payout_ledger: 0,
        });
        env.storage()
            .instance()
            .set(&PoolKey::ParametricPolicies, &policies);
        env.storage()
            .instance()
            .set(&PoolKey::Reports(id), &Vec::<WeatherReport>::new(&env));
        env.storage().instance().set(
            &PoolKey::OutstandingRisk,
            &(Self::outstanding(&env) + coverage_amount),
        );
        env.events()
            .publish((symbol_short!("parampol"), farmer), id);
        id
    }

    pub fn bond_oracle(env: Env, oracle: Address, amount: i128) {
        oracle.require_auth();
        if amount <= 0 {
            panic!("invalid bond");
        }
        token::Client::new(&env, &Self::token(&env)).transfer(
            &oracle,
            &env.current_contract_address(),
            &amount,
        );
        let mut bonds = Self::oracle_bonds(&env);
        let mut found = false;
        for i in 0..bonds.len() {
            let (addr, bal) = bonds.get(i).unwrap();
            if addr == oracle {
                bonds.set(i, (addr, bal + amount));
                found = true;
            }
        }
        if !found {
            bonds.push_back((oracle, amount));
        }
        env.storage().instance().set(&PoolKey::OracleBonds, &bonds);
    }

    pub fn submit_weather_report(
        env: Env,
        policy_id: u64,
        oracle: Address,
        rainfall_30d_mm: i128,
        avg_temp_c: i128,
    ) {
        oracle.require_auth();
        let policy = Self::parametric_policy(&env, policy_id);
        if !Self::contains_address(&policy.oracles, &oracle) {
            panic!("unauthorized oracle");
        }
        let mut reports = Self::reports(&env, policy_id);
        for i in 0..reports.len() {
            if reports.get(i).unwrap().oracle == oracle {
                panic!("duplicate report");
            }
        }
        reports.push_back(WeatherReport {
            oracle,
            rainfall_30d_mm,
            avg_temp_c,
        });
        env.storage()
            .instance()
            .set(&PoolKey::Reports(policy_id), &reports);
    }

    pub fn claim_parametric_payout(env: Env, farmer: Address, policy_id: u64) -> i128 {
        farmer.require_auth();
        let mut policies = Self::parametric_policies(&env);
        for i in 0..policies.len() {
            let mut policy = policies.get(i).unwrap();
            if policy.id == policy_id {
                if policy.farmer != farmer {
                    panic!("unauthorized farmer");
                }
                if !policy.active || policy.paid_out {
                    panic!("policy not claimable");
                }
                let weather = Self::aggregate_weather(env.clone(), policy_id);
                if !Self::evaluate_trigger(&policy.trigger_nodes, policy.root_node, &weather) {
                    panic!("trigger not met");
                }
                let payout = policy.coverage_amount - policy.deductible;
                Self::ensure_liquid(&env, payout);
                token::Client::new(&env, &Self::token(&env)).transfer(
                    &env.current_contract_address(),
                    &policy.farmer,
                    &payout,
                );
                Self::sub_capital(&env, payout);
                env.storage().instance().set(
                    &PoolKey::OutstandingRisk,
                    &(Self::outstanding(&env) - policy.coverage_amount),
                );
                policy.active = false;
                policy.paid_out = true;
                policy.payout_amount = payout;
                policy.payout_ledger = env.ledger().sequence();
                policies.set(i, policy);
                env.storage()
                    .instance()
                    .set(&PoolKey::ParametricPolicies, &policies);
                return payout;
            }
        }
        panic!("policy not found");
    }

    pub fn submit_dispute(env: Env, disputer: Address, policy_id: u64, oracle: Address) -> u64 {
        disputer.require_auth();
        let policy = Self::parametric_policy(&env, policy_id);
        if !policy.paid_out {
            panic!("no payout to dispute");
        }
        if env.ledger().sequence() > policy.payout_ledger + DISPUTE_WINDOW_LEDGERS {
            panic!("dispute window closed");
        }
        let bond = (policy.payout_amount * DISPUTE_BOND_BPS) / BPS_DENOMINATOR;
        token::Client::new(&env, &Self::token(&env)).transfer(
            &disputer,
            &env.current_contract_address(),
            &bond,
        );
        let id: u64 = env
            .storage()
            .instance()
            .get(&PoolKey::NextDisputeId)
            .unwrap();
        env.storage()
            .instance()
            .set(&PoolKey::NextDisputeId, &(id + 1));
        let mut disputes = Self::disputes(&env);
        disputes.push_back(Dispute {
            id,
            policy_id,
            disputer,
            oracle,
            bond,
            resolved: false,
            successful: false,
        });
        env.storage().instance().set(&PoolKey::Disputes, &disputes);
        id
    }

    pub fn resolve_dispute(env: Env, admin: Address, dispute_id: u64, successful: bool) {
        admin.require_auth();
        if admin != Self::admin(&env) {
            panic!("unauthorized admin");
        }
        let mut disputes = Self::disputes(&env);
        for i in 0..disputes.len() {
            let mut dispute = disputes.get(i).unwrap();
            if dispute.id == dispute_id {
                if dispute.resolved {
                    panic!("dispute resolved");
                }
                dispute.resolved = true;
                dispute.successful = successful;
                if successful {
                    let slashed = Self::slash_oracle_bond(&env, &dispute.oracle, dispute.bond);
                    token::Client::new(&env, &Self::token(&env)).transfer(
                        &env.current_contract_address(),
                        &dispute.disputer,
                        &(dispute.bond + slashed),
                    );
                }
                disputes.set(i, dispute);
                env.storage().instance().set(&PoolKey::Disputes, &disputes);
                return;
            }
        }
        panic!("dispute not found");
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

    pub fn get_parametric_policies(env: Env) -> Vec<ParametricPolicy> {
        Self::parametric_policies(&env)
    }

    pub fn aggregate_weather(env: Env, policy_id: u64) -> AggregatedWeather {
        let reports = Self::reports(&env, policy_id);
        if reports.len() != PARAMETRIC_ORACLE_COUNT {
            panic!("requires 5 reports");
        }
        let mut rainfall = Vec::new(&env);
        let mut temps = Vec::new(&env);
        for i in 0..reports.len() {
            let report = reports.get(i).unwrap();
            rainfall.push_back(report.rainfall_30d_mm);
            temps.push_back(report.avg_temp_c);
        }
        let rain_median = Self::median(rainfall.clone());
        let temp_median = Self::median(temps.clone());
        let rain_mad = Self::mad(&env, rainfall.clone(), rain_median);
        let temp_mad = Self::mad(&env, temps.clone(), temp_median);
        let mut accepted_rain = Vec::new(&env);
        let mut accepted_temp = Vec::new(&env);
        for i in 0..reports.len() {
            let report = reports.get(i).unwrap();
            if Self::within_mad(report.rainfall_30d_mm, rain_median, rain_mad)
                && Self::within_mad(report.avg_temp_c, temp_median, temp_mad)
            {
                accepted_rain.push_back(report.rainfall_30d_mm);
                accepted_temp.push_back(report.avg_temp_c);
            }
        }
        if accepted_rain.len() == 0 {
            panic!("all reports rejected");
        }
        AggregatedWeather {
            rainfall_30d_mm: Self::median(accepted_rain.clone()),
            avg_temp_c: Self::median(accepted_temp),
            accepted_reports: accepted_rain.len(),
        }
    }

    fn evaluate_trigger(nodes: &Vec<TriggerNode>, root: u32, weather: &AggregatedWeather) -> bool {
        match nodes.get(root).unwrap() {
            TriggerNode::Condition(metric, comparator, threshold) => {
                let value = match metric {
                    WeatherMetric::Rainfall30d => weather.rainfall_30d_mm,
                    WeatherMetric::AvgTemp => weather.avg_temp_c,
                };
                match comparator {
                    Comparator::Lt => value < threshold,
                    Comparator::Gt => value > threshold,
                    Comparator::Le => value <= threshold,
                    Comparator::Ge => value >= threshold,
                }
            }
            TriggerNode::And(left, right) => {
                Self::evaluate_trigger(nodes, left, weather)
                    && Self::evaluate_trigger(nodes, right, weather)
            }
            TriggerNode::Or(left, right) => {
                Self::evaluate_trigger(nodes, left, weather)
                    || Self::evaluate_trigger(nodes, right, weather)
            }
        }
    }

    fn median(mut values: Vec<i128>) -> i128 {
        let len = values.len();
        for i in 0..len {
            for j in (i + 1)..len {
                if values.get(j).unwrap() < values.get(i).unwrap() {
                    let a = values.get(i).unwrap();
                    let b = values.get(j).unwrap();
                    values.set(i, b);
                    values.set(j, a);
                }
            }
        }
        values.get(len / 2).unwrap()
    }
    fn mad(env: &Env, values: Vec<i128>, median: i128) -> i128 {
        let mut deviations = Vec::new(env);
        for i in 0..values.len() {
            let v = values.get(i).unwrap();
            deviations.push_back(if v > median { v - median } else { median - v });
        }
        Self::median(deviations)
    }
    fn within_mad(value: i128, median: i128, mad: i128) -> bool {
        if mad == 0 {
            return value == median;
        }
        let deviation = if value > median {
            value - median
        } else {
            median - value
        };
        deviation <= mad * ORACLE_OUTLIER_MAD_MULTIPLIER
    }
    fn contains_address(addresses: &Vec<Address>, needle: &Address) -> bool {
        for i in 0..addresses.len() {
            if addresses.get(i).unwrap() == *needle {
                return true;
            }
        }
        false
    }
    pub fn get_idle_deposited(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&PoolKey::IdleDeposited)
            .unwrap_or(0)
    }

    fn admin(env: &Env) -> Address {
        env.storage().instance().get(&PoolKey::Admin).unwrap()
    }
    fn treasury(env: &Env) -> Address {
        env.storage().instance().get(&PoolKey::Treasury).unwrap()
    }
    fn parametric_policies(env: &Env) -> Vec<ParametricPolicy> {
        env.storage()
            .instance()
            .get(&PoolKey::ParametricPolicies)
            .unwrap_or(Vec::new(env))
    }
    fn parametric_policy(env: &Env, policy_id: u64) -> ParametricPolicy {
        let policies = Self::parametric_policies(env);
        for i in 0..policies.len() {
            let policy = policies.get(i).unwrap();
            if policy.id == policy_id {
                return policy;
            }
        }
        panic!("policy not found");
    }
    fn reports(env: &Env, policy_id: u64) -> Vec<WeatherReport> {
        env.storage()
            .instance()
            .get(&PoolKey::Reports(policy_id))
            .unwrap_or(Vec::new(env))
    }
    fn oracle_bonds(env: &Env) -> Vec<(Address, i128)> {
        env.storage()
            .instance()
            .get(&PoolKey::OracleBonds)
            .unwrap_or(Vec::new(env))
    }
    fn disputes(env: &Env) -> Vec<Dispute> {
        env.storage()
            .instance()
            .get(&PoolKey::Disputes)
            .unwrap_or(Vec::new(env))
    }
    fn slash_oracle_bond(env: &Env, oracle: &Address, requested: i128) -> i128 {
        let mut bonds = Self::oracle_bonds(env);
        for i in 0..bonds.len() {
            let (addr, bal) = bonds.get(i).unwrap();
            if addr == *oracle {
                let slashed = if requested > bal { bal } else { requested };
                bonds.set(i, (addr, bal - slashed));
                env.storage().instance().set(&PoolKey::OracleBonds, &bonds);
                return slashed;
            }
        }
        0
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
