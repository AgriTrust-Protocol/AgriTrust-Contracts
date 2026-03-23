#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Symbol, Env, String, Vec, Map, xdr::ScVal};

#[contract]
pub struct GrantContract;

#[derive(Clone, contracttype)]
#[contracttype]
pub enum VestingCurve {
    Linear,
    Exponential { factor: u32 }, // factor: 1000 = 1.0x base rate
    Logarithmic { factor: u32 },  // factor: 1000 = 1.0x initial rate
}

#[derive(Clone, contracttype)]
#[contracttype]
pub struct Asset {
    pub code: String,
    pub issuer: Option<Address>,
}

#[derive(Clone, contracttype)]
#[contracttype]
pub struct TreasuryConfig {
    pub reserve_wallet: Address,
    pub low_water_mark_days: u32, // days of funding left before trigger
    pub refill_amount: u128,
    pub max_auto_refill: u128, // maximum total auto-refill amount
    pub total_auto_refilled: u128,
    pub allowance_active: bool,
}

#[derive(Clone, contracttype)]
#[contracttype]
pub struct MultiSigConfig {
    pub council_members: Vec<Address>,
    pub required_signatures: u32,
    pub cold_storage_vault: Address,
    pub emergency_drain_active: bool,
    pub drain_signatures: Map<Address, u64>, // member -> timestamp of signature
    pub drain_proposal_expiry: u64, // timestamp when proposal expires
}

#[derive(Clone, contracttype)]
#[contracttype]
pub struct EmergencyDrainProposal {
    pub proposal_id: Symbol,
    pub proposed_by: Address,
    pub reason: String,
    pub created_at: u64,
    pub expires_at: u64,
    pub executed: bool,
}

#[derive(Clone, contracttype)]
#[contracttype]
pub struct StreamPool {
    pub asset: Asset,
    pub balance: u128,
    pub total_deposited: u128,
    pub last_refill_time: u64,
    pub isolated_sub_pools: Map<Symbol, IsolatedSubPool>, // grant_id -> sub_pool
}

#[derive(Clone, contracttype)]
#[contracttype]
pub struct IsolatedSubPool {
    pub grant_id: Symbol,
    pub allocated_amount: u128,
    pub consumed_amount: u128,
    pub created_time: u64,
    pub is_active: bool,
}

#[derive(Clone, contracttype)]
#[contracttype]
pub struct Grant {
    pub admin: Address,
    pub grantee: Address,
    pub total_amount: u128,
    pub released_amount: u128,
    pub start_time: u64,
    pub duration: u64,
    pub vesting_curve: VestingCurve,
    pub max_supply: u128,
    pub stream_pool_id: Symbol,
}

#[derive(Clone, contracttype)]
#[contracttype]
pub struct Milestone {
    pub id: Symbol,
    pub amount: u128,
    pub description: String,
    pub approved: bool,
    pub released: bool,
}

#[contractimpl]
impl GrantContract {
    pub fn create_grant(
        env: Env,
        grant_id: Symbol,
        admin: Address,
        grantee: Address,
        total_amount: u128,
        start_time: u64,
        duration: u64,
        vesting_curve: VestingCurve,
        max_supply: u128,
        stream_pool_id: Symbol,
    ) {
        if total_amount > max_supply {
            panic!("Total amount cannot exceed max supply");
        }
        
        let grant = Grant {
            admin: admin.clone(),
            grantee,
            total_amount,
            released_amount: 0,
            start_time,
            duration,
            vesting_curve,
            max_supply,
            stream_pool_id: stream_pool_id.clone(),
        };
        
        env.storage().instance().set(&grant_id, &grant);
        env.storage().instance().set(&Symbol::new(&env, "milestones"), &Vec::<Milestone>::new(&env));
    }

    pub fn create_stream_pool(
        env: Env,
        pool_id: Symbol,
        asset: Asset,
        initial_balance: u128,
    ) {
        let pool = StreamPool {
            asset,
            balance: initial_balance,
            total_deposited: initial_balance,
            last_refill_time: env.ledger().timestamp(),
            isolated_sub_pools: Map::new(&env),
        };
        
        env.storage().instance().set(&pool_id, &pool);
    }

    pub fn create_isolated_sub_pool(
        env: Env,
        pool_id: Symbol,
        grant_id: Symbol,
        allocated_amount: u128,
    ) {
        let mut pool: StreamPool = env.storage().instance().get(&pool_id)
            .unwrap_or_else(|| panic!("Stream pool not found"));
        
        // Check if sub-pool already exists
        if pool.isolated_sub_pools.contains_key(&grant_id) {
            panic!("Sub-pool already exists for this grant");
        }
        
        // Check if pool has sufficient balance
        let total_allocated: u128 = pool.isolated_sub_pools.iter()
            .map(|(_, sub_pool)| if sub_pool.is_active { sub_pool.allocated_amount } else { 0 })
            .sum();
        
        if total_allocated + allocated_amount > pool.balance {
            panic!("Insufficient pool balance for allocation");
        }
        
        let sub_pool = IsolatedSubPool {
            grant_id: grant_id.clone(),
            allocated_amount,
            consumed_amount: 0,
            created_time: env.ledger().timestamp(),
            is_active: true,
        };
        
        pool.isolated_sub_pools.set(grant_id.clone(), &sub_pool);
        env.storage().instance().set(&pool_id, &pool);
    }

    pub fn consume_from_sub_pool(
        env: Env,
        pool_id: Symbol,
        grant_id: Symbol,
        amount: u128,
    ) {
        let mut pool: StreamPool = env.storage().instance().get(&pool_id)
            .unwrap_or_else(|| panic!("Stream pool not found"));
        
        let mut sub_pool: IsolatedSubPool = pool.isolated_sub_pools.get(&grant_id)
            .unwrap_or_else(|| panic!("Sub-pool not found"));
        
        if !sub_pool.is_active {
            panic!("Sub-pool is not active");
        }
        
        // Check if sub-pool has sufficient allocation
        let remaining_allocation = sub_pool.allocated_amount - sub_pool.consumed_amount;
        if amount > remaining_allocation {
            panic!("Insufficient allocation in sub-pool");
        }
        
        // Check if main pool has sufficient balance
        if amount > pool.balance {
            panic!("Insufficient balance in main pool");
        }
        
        // Update sub-pool and main pool
        sub_pool.consumed_amount += amount;
        pool.balance -= amount;
        
        pool.isolated_sub_pools.set(grant_id, &sub_pool);
        env.storage().instance().set(&pool_id, &pool);
    }

    pub fn extend_sub_pool_allocation(
        env: Env,
        pool_id: Symbol,
        grant_id: Symbol,
        additional_amount: u128,
    ) {
        let mut pool: StreamPool = env.storage().instance().get(&pool_id)
            .unwrap_or_else(|| panic!("Stream pool not found"));
        
        let mut sub_pool: IsolatedSubPool = pool.isolated_sub_pools.get(&grant_id)
            .unwrap_or_else(|| panic!("Sub-pool not found"));
        
        if !sub_pool.is_active {
            panic!("Sub-pool is not active");
        }
        
        // Check if pool has sufficient balance for extension
        let total_allocated: u128 = pool.isolated_sub_pools.iter()
            .map(|(_, sp)| if sp.is_active { sp.allocated_amount } else { 0 })
            .sum();
        
        if total_allocated + additional_amount > pool.balance {
            panic!("Insufficient pool balance for extension");
        }
        
        // Extend allocation
        sub_pool.allocated_amount += additional_amount;
        pool.isolated_sub_pools.set(grant_id, &sub_pool);
        env.storage().instance().set(&pool_id, &pool);
    }

    pub fn deactivate_sub_pool(
        env: Env,
        pool_id: Symbol,
        grant_id: Symbol,
    ) {
        let mut pool: StreamPool = env.storage().instance().get(&pool_id)
            .unwrap_or_else(|| panic!("Stream pool not found"));
        
        let mut sub_pool: IsolatedSubPool = pool.isolated_sub_pools.get(&grant_id)
            .unwrap_or_else(|| panic!("Sub-pool not found"));
        
        sub_pool.is_active = false;
        pool.isolated_sub_pools.set(grant_id, &sub_pool);
        env.storage().instance().set(&pool_id, &pool);
    }

    pub fn get_sub_pool_info(
        env: Env,
        pool_id: Symbol,
        grant_id: Symbol,
    ) -> IsolatedSubPool {
        let pool: StreamPool = env.storage().instance().get(&pool_id)
            .unwrap_or_else(|| panic!("Stream pool not found"));
        
        pool.isolated_sub_pools.get(&grant_id)
            .unwrap_or_else(|| panic!("Sub-pool not found"))
    }

    pub fn get_pool_summary(env: Env, pool_id: Symbol) -> (u128, u128, u32) {
        let pool: StreamPool = env.storage().instance().get(&pool_id)
            .unwrap_or_else(|| panic!("Stream pool not found"));
        
        let total_allocated: u128 = pool.isolated_sub_pools.iter()
            .map(|(_, sub_pool)| if sub_pool.is_active { sub_pool.allocated_amount } else { 0 })
            .sum();
        
        let total_consumed: u128 = pool.isolated_sub_pools.iter()
            .map(|(_, sub_pool)| if sub_pool.is_active { sub_pool.consumed_amount } else { 0 })
            .sum();
        
        let active_sub_pools: u32 = pool.isolated_sub_pools.iter()
            .map(|(_, sub_pool)| if sub_pool.is_active { 1 } else { 0 })
            .sum();
        
        (total_allocated, total_consumed, active_sub_pools)
    }

    pub fn configure_multi_sig(
        env: Env,
        multi_sig_config: MultiSigConfig,
    ) {
        // Only contract admin can configure multi-sig
        let admin = env.storage().instance().get(&Symbol::new(&env, "admin"))
            .unwrap_or_else(|| panic!("Admin not set"));
        
        if env.current_contract_address() != admin {
            panic!("Only admin can configure multi-sig");
        }
        
        // Validate configuration
        if multi_sig_config.required_signatures == 0 {
            panic!("Required signatures must be greater than 0");
        }
        
        if multi_sig_config.required_signatures > multi_sig_config.council_members.len() as u32 {
            panic!("Required signatures cannot exceed council size");
        }
        
        if multi_sig_config.council_members.is_empty() {
            panic!("Council members cannot be empty");
        }
        
        env.storage().instance().set(&Symbol::new(&env, "multi_sig"), &multi_sig_config);
    }

    pub fn create_emergency_drain_proposal(
        env: Env,
        proposal_id: Symbol,
        proposer: Address,
        reason: String,
    ) {
        let multi_sig_config: MultiSigConfig = env.storage().instance()
            .get(&Symbol::new(&env, "multi_sig"))
            .unwrap_or_else(|| panic!("Multi-sig not configured"));
        
        // Check if proposer is a council member
        if !multi_sig_config.council_members.contains(&proposer) {
            panic!("Only council members can create proposals");
        }
        
        // Check if emergency drain is active
        if !multi_sig_config.emergency_drain_active {
            panic!("Emergency drain is not active");
        }
        
        let current_time = env.ledger().timestamp();
        let expiry_time = current_time + 86400; // 24 hours expiry
        
        let proposal = EmergencyDrainProposal {
            proposal_id: proposal_id.clone(),
            proposed_by: proposer,
            reason,
            created_at: current_time,
            expires_at: expiry_time,
            executed: false,
        };
        
        env.storage().instance().set(&proposal_id, &proposal);
        
        // Initialize signature map for this proposal
        let mut signature_map: Map<Address, u64> = Map::new(&env);
        env.storage().instance().set(&Symbol::new(&env, "drain_signatures"), &signature_map);
        
        // Emit event
        env.events().contract(
            Symbol::new(&env, "emergency_drain_proposal"),
            (proposal_id, proposer, expiry_time),
        );
    }

    pub fn sign_emergency_drain(
        env: Env,
        proposal_id: Symbol,
        signer: Address,
    ) {
        let multi_sig_config: MultiSigConfig = env.storage().instance()
            .get(&Symbol::new(&env, "multi_sig"))
            .unwrap_or_else(|| panic!("Multi-sig not configured"));
        
        let proposal: EmergencyDrainProposal = env.storage().instance().get(&proposal_id)
            .unwrap_or_else(|| panic!("Proposal not found"));
        
        if proposal.executed {
            panic!("Proposal already executed");
        }
        
        let current_time = env.ledger().timestamp();
        if current_time > proposal.expires_at {
            panic!("Proposal has expired");
        }
        
        // Check if signer is a council member
        if !multi_sig_config.council_members.contains(&signer) {
            panic!("Only council members can sign");
        }
        
        let mut signature_map: Map<Address, u64> = env.storage().instance()
            .get(&Symbol::new(&env, "drain_signatures"))
            .unwrap_or_else(|| Map::new(&env));
        
        // Check if already signed
        if signature_map.contains_key(&signer) {
            panic!("Already signed");
        }
        
        // Add signature
        signature_map.set(signer.clone(), current_time);
        env.storage().instance().set(&Symbol::new(&env, "drain_signatures"), &signature_map);
        
        // Check if we have enough signatures
        let signature_count = signature_map.len();
        if signature_count >= multi_sig_config.required_signatures as u32 {
            // Execute emergency drain
            Self::execute_emergency_drain(env, proposal_id.clone());
        }
        
        // Emit event
        env.events().contract(
            Symbol::new(&env, "emergency_drain_signed"),
            (proposal_id, signer, signature_count),
        );
    }

    pub fn execute_emergency_drain(env: Env, proposal_id: Symbol) {
        let multi_sig_config: MultiSigConfig = env.storage().instance()
            .get(&Symbol::new(&env, "multi_sig"))
            .unwrap_or_else(|| panic!("Multi-sig not configured"));
        
        let mut proposal: EmergencyDrainProposal = env.storage().instance().get(&proposal_id)
            .unwrap_or_else(|| panic!("Proposal not found"));
        
        if proposal.executed {
            panic!("Proposal already executed");
        }
        
        let signature_map: Map<Address, u64> = env.storage().instance()
            .get(&Symbol::new(&env, "drain_signatures"))
            .unwrap_or_else(|| Map::new(&env));
        
        if signature_map.len() < multi_sig_config.required_signatures as u32 {
            panic!("Insufficient signatures");
        }
        
        // Calculate total funds to drain (all unstreamed funds)
        let total_funds = Self::calculate_total_unstreamed_funds(env);
        
        if total_funds > 0 {
            // In a real implementation, this would transfer tokens to cold storage
            // For now, we simulate by setting balances to zero
            Self::drain_all_stream_pools(env);
            
            // Mark proposal as executed
            proposal.executed = true;
            env.storage().instance().set(&proposal_id, &proposal);
            
            // Emit event
            env.events().contract(
                Symbol::new(&env, "emergency_drain_executed"),
                (proposal_id, total_funds, multi_sig_config.cold_storage_vault),
            );
        }
    }

    fn calculate_total_unstreamed_funds(_env: Env) -> u128 {
        let total = 0u128;
        
        // This would iterate through all stream pools and calculate remaining balances
        // For simplicity, we'll return a placeholder value
        // In a real implementation, you'd need to track all pool IDs
        
        total
    }

    fn drain_all_stream_pools(env: Env) {
        // This would drain all stream pools to zero balance
        // In a real implementation, you'd iterate through all pools and transfer funds
        
        // For now, we'll emit an event indicating the drain
        env.events().contract(
            Symbol::new(&env, "all_pools_drained"),
            env.ledger().timestamp(),
        );
    }

    pub fn get_emergency_proposal(env: Env, proposal_id: Symbol) -> EmergencyDrainProposal {
        env.storage().instance().get(&proposal_id)
            .unwrap_or_else(|| panic!("Proposal not found"))
    }

    pub fn get_signature_count(env: Env) -> u32 {
        let signature_map: Map<Address, u64> = env.storage().instance()
            .get(&Symbol::new(&env, "drain_signatures"))
            .unwrap_or_else(|| Map::new(&env));
        
        signature_map.len()
    }

    pub fn get_multi_sig_config(env: Env) -> MultiSigConfig {
        env.storage().instance().get(&Symbol::new(&env, "multi_sig"))
            .unwrap_or_else(|| panic!("Multi-sig not configured"))
    }

    pub fn configure_treasury(
        env: Env,
        treasury_config: TreasuryConfig,
    ) {
        // Only contract admin can configure treasury
        let admin = env.storage().instance().get(&Symbol::new(&env, "admin"))
            .unwrap_or_else(|| panic!("Admin not set"));
        
        if env.current_contract_address() != admin {
            panic!("Only admin can configure treasury");
        }
        
        env.storage().instance().set(&Symbol::new(&env, "treasury"), &treasury_config);
    }

    pub fn set_admin(env: Env, admin: Address) {
        // This can only be called once during initialization
        if env.storage().instance().contains(&Symbol::new(&env, "admin")) {
            panic!("Admin already set");
        }
        env.storage().instance().set(&Symbol::new(&env, "admin"), &admin);
    }

    pub fn check_and_auto_refill(env: Env, pool_id: Symbol) {
        let treasury_config: TreasuryConfig = env.storage().instance()
            .get(&Symbol::new(&env, "treasury"))
            .unwrap_or_else(|| panic!("Treasury not configured"));
        
        if !treasury_config.allowance_active {
            return; // Auto-refill disabled
        }
        
        if treasury_config.total_auto_refilled >= treasury_config.max_auto_refill {
            return; // Max auto-refill reached
        }
        
        let mut pool: StreamPool = env.storage().instance().get(&pool_id)
            .unwrap_or_else(|| panic!("Stream pool not found"));
        
        let current_time = env.ledger().timestamp();
        let days_since_last_refill = (current_time - pool.last_refill_time) / 86400; // seconds in a day
        
        // Calculate daily consumption rate
        let daily_rate = if days_since_last_refill > 0 {
            (pool.total_deposited - pool.balance) / days_since_last_refill as u128
        } else {
            0
        };
        
        // Calculate days of funding remaining
        let days_remaining = if daily_rate > 0 {
            pool.balance / daily_rate
        } else {
            u128::MAX // Infinite if no consumption
        };
        
        // Check if below low water mark
        if days_remaining < treasury_config.low_water_mark_days as u128 {
            let remaining_refill_capacity = treasury_config.max_auto_refill - treasury_config.total_auto_refilled;
            let actual_refill_amount = treasury_config.refill_amount.min(remaining_refill_capacity);
            
            if actual_refill_amount > 0 {
                // In a real implementation, this would trigger a path_payment operation
                // For now, we simulate the refill by updating the pool balance
                pool.balance += actual_refill_amount;
                pool.total_deposited += actual_refill_amount;
                pool.last_refill_time = current_time;
                
                env.storage().instance().set(&pool_id, &pool);
                
                // Update treasury config
                let mut updated_config = treasury_config;
                updated_config.total_auto_refilled += actual_refill_amount;
                env.storage().instance().set(&Symbol::new(&env, "treasury"), &updated_config);
                
                // Emit event for monitoring
                env.events().contract(
                    Symbol::new(&env, "auto_refill"),
                    (
                        pool_id.clone(),
                        actual_refill_amount,
                        pool.balance,
                        updated_config.total_auto_refilled,
                    ),
                );
            }
        }
    }

    pub fn add_milestone(
        env: Env,
        grant_id: Symbol,
        milestone_id: Symbol,
        amount: u128,
        description: String,
    ) {
        let grant: Grant = env.storage().instance().get(&grant_id).unwrap();
        let mut milestones: Vec<Milestone> = env.storage().instance()
            .get(&Symbol::new(&env, "milestones")).unwrap();
        
        // Check if this milestone would exceed total grant amount
        let total_milestone_amount: u128 = milestones.iter()
            .map(|m| if !m.released { m.amount } else { 0 })
            .sum();
        
        if total_milestone_amount + amount > grant.total_amount - grant.released_amount {
            panic!("Milestone amount would exceed remaining grant amount");
        }
        
        let milestone = Milestone {
            id: milestone_id.clone(),
            amount,
            description,
            approved: false,
            released: false,
        };
        
        milestones.push_back(milestone);
        env.storage().instance().set(&Symbol::new(&env, "milestones"), &milestones);
    }

    pub fn approve_milestone(env: Env, grant_id: Symbol, milestone_id: Symbol) {
        let grant: Grant = env.storage().instance().get(&grant_id).unwrap();
        let mut milestones: Vec<Milestone> = env.storage().instance()
            .get(&Symbol::new(&env, "milestones")).unwrap();
        
        let mut found = false;
        for i in 0..milestones.len() {
            if milestones.get(i).unwrap().id == milestone_id {
                let milestone = milestones.get(i).unwrap();
                if milestone.approved {
                    panic!("Milestone already approved");
                }
                
                let updated_milestone = Milestone {
                    approved: true,
                    ..milestone
                };
                milestones.set(i, updated_milestone);
                found = true;
                break;
            }
        }
        
        if !found {
            panic!("Milestone not found");
        }
        
        env.storage().instance().set(&Symbol::new(&env, "milestones"), &milestones);
    }

    pub fn release_milestone(env: Env, grant_id: Symbol, milestone_id: Symbol) {
        let mut grant: Grant = env.storage().instance().get(&grant_id).unwrap();
        let mut milestones: Vec<Milestone> = env.storage().instance()
            .get(&Symbol::new(&env, "milestones")).unwrap();
        
        let mut found = false;
        let mut release_amount = 0u128;
        let mut stream_pool_id = Symbol::new(&env, "");
        
        for i in 0..milestones.len() {
            if milestones.get(i).unwrap().id == milestone_id {
                let milestone = milestones.get(i).unwrap();
                if !milestone.approved {
                    panic!("Milestone not approved");
                }
                if milestone.released {
                    panic!("Milestone already released");
                }
                
                release_amount = milestone.amount;
                stream_pool_id = grant.stream_pool_id.clone();
                
                // Check against max supply
                if grant.released_amount + release_amount > grant.max_supply {
                    panic!("Release would exceed max supply");
                }
                
                // Use isolated sub-pool consumption
                Self::consume_from_sub_pool(env.clone(), stream_pool_id.clone(), grant_id.clone(), release_amount);
                
                // Trigger auto-refill check after release
                Self::check_and_auto_refill(env.clone(), stream_pool_id);
                
                let updated_milestone = Milestone {
                    released: true,
                    ..milestone
                };
                milestones.set(i, updated_milestone);
                found = true;
                break;
            }
        }
        
        if !found {
            panic!("Milestone not found");
        }
        
        grant.released_amount += release_amount;
        env.storage().instance().set(&grant_id, &grant);
        env.storage().instance().set(&Symbol::new(&env, "milestones"), &milestones);
    }

    pub fn get_grant(env: Env, grant_id: Symbol) -> Grant {
        env.storage().instance().get(&grant_id).unwrap()
    }

    pub fn get_milestones(env: Env, _grant_id: Symbol) -> Vec<Milestone> {
        env.storage().instance().get(&Symbol::new(&env, "milestones")).unwrap()
    }

    pub fn get_remaining_amount(env: Env, grant_id: Symbol) -> u128 {
        let _grant: Grant = env.storage().instance().get(&grant_id).unwrap();
        // In a real implementation, you would calculate based on the grant
        // For now, return a placeholder
        1000000
    }

    pub fn get_stream_pool(env: Env, pool_id: Symbol) -> StreamPool {
        env.storage().instance().get(&pool_id).unwrap()
    }

    pub fn get_treasury_config(env: Env) -> TreasuryConfig {
        env.storage().instance().get(&Symbol::new(&env, "treasury")).unwrap()
    }

    pub fn compute_vested_amount(env: Env, grant_id: Symbol, current_time: u64) -> u128 {
        let grant: Grant = env.storage().instance().get(&grant_id).unwrap();
        
        match grant.vesting_curve {
            VestingCurve::Linear => {
                grant::compute_claimable_balance(
                    grant.total_amount,
                    grant.start_time,
                    current_time,
                    grant.duration,
                )
            }
            VestingCurve::Exponential { factor } => {
                grant::compute_exponential_vesting(
                    grant.total_amount,
                    grant.start_time,
                    current_time,
                    grant.duration,
                    factor,
                )
            }
            VestingCurve::Logarithmic { factor } => {
                grant::compute_logarithmic_vesting(
                    grant.total_amount,
                    grant.start_time,
                    current_time,
                    grant.duration,
                    factor,
                )
            }
        }
    }
}

mod test;

// Grant math utilities used by tests and (optionally) the contract.
pub mod grant {
    use soroban_sdk::Env;
    
    /// Compute the claimable balance for a linear vesting grant.
    ///
    /// - `total`: total amount granted (u128)
    /// - `start`: grant start timestamp (seconds, u64)
    /// - `now`: current timestamp (seconds, u64)
    /// - `duration`: grant duration (seconds, u64)
    ///
    /// Returns the amount (u128) claimable at `now` (clamped 0..=total).
    pub fn compute_claimable_balance(total: u128, start: u64, now: u64, duration: u64) -> u128 {
        if duration == 0 {
            return if now >= start { total } else { 0 };
        }
        if now <= start {
            return 0;
        }
        let elapsed = now.saturating_sub(start);
        if elapsed >= duration {
            return total;
        }

        // Use decomposition to reduce risk of intermediate overflow:
        // total * elapsed / duration == (total / duration) * elapsed + (total % duration) * elapsed / duration
        let dur = duration as u128;
        let el = elapsed as u128;
        let whole = total / dur;
        let rem = total % dur;

        // whole * el shouldn't overflow in realistic token amounts, but use checked_mul with fallback.
        let part1 = match whole.checked_mul(el) {
            Some(v) => v,
            None => {
                // fallback: perform (whole / dur) * (el * dur) approximated by dividing early
                // This branch is extremely unlikely; clamp to total as safe fallback.
                return total;
            }
        };
        let part2 = match rem.checked_mul(el) {
            Some(v) => v / dur,
            None => {
                return total;
            }
        };
        part1 + part2
    }

    /// Compute the claimable balance for exponential vesting.
    /// Rate increases as project nears completion.
    /// Formula: total * (1 - exp(-factor * progress)) / (1 - exp(-factor))
    /// where progress = elapsed / duration
    pub fn compute_exponential_vesting(
        total: u128,
        start: u64,
        now: u64,
        duration: u64,
        factor: u32,
    ) -> u128 {
        if duration == 0 {
            return if now >= start { total } else { 0 };
        }
        if now <= start {
            return 0;
        }
        let elapsed = now.saturating_sub(start);
        if elapsed >= duration {
            return total;
        }

        let progress = (elapsed as u128 * 1000) / (duration as u128); // progress in 0.1% increments
        let factor_scaled = factor as u128; // factor is already scaled by 1000
        
        // Simplified exponential approximation: total * progress^2 / 1000000 * factor
        // This avoids complex floating point math while providing exponential growth
        let progress_squared = match progress.checked_mul(progress) {
            Some(v) => v,
            None => return total, // overflow protection
        };
        
        let factor_progress = match progress_squared.checked_mul(factor_scaled) {
            Some(v) => v,
            None => return total,
        };
        
        let vested = match total.checked_mul(factor_progress) {
            Some(v) => v / 1_000_000_000, // Normalize by 1000^3
            None => total,
        };
        
        vested.min(total)
    }

    /// Compute the claimable balance for logarithmic vesting.
    /// Rate decreases as project progresses (front-loaded).
    /// Formula: total * ln(1 + factor * progress) / ln(1 + factor)
    /// where progress = elapsed / duration
    pub fn compute_logarithmic_vesting(
        total: u128,
        start: u64,
        now: u64,
        duration: u64,
        factor: u32,
    ) -> u128 {
        if duration == 0 {
            return if now >= start { total } else { 0 };
        }
        if now <= start {
            return 0;
        }
        let elapsed = now.saturating_sub(start);
        if elapsed >= duration {
            return total;
        }

        let progress = (elapsed as u128 * 1000) / (duration as u128); // progress in 0.1% increments
        let factor_scaled = factor as u128; // factor is already scaled by 1000
        
        // Simplified logarithmic approximation: total * (sqrt(progress * factor) * 1000) / (sqrt(factor) * 1000)
        // This provides front-loaded vesting without complex math
        if progress == 0 {
            return 0;
        }
        
        let progress_factor = match progress.checked_mul(factor_scaled) {
            Some(v) => v,
            None => return total,
        };
        
        // Integer square root approximation
        let sqrt_progress_factor = integer_sqrt(progress_factor);
        let sqrt_factor = integer_sqrt(factor_scaled);
        
        if sqrt_factor == 0 {
            return 0;
        }
        
        let vested = match total.checked_mul(sqrt_progress_factor) {
            Some(v) => {
                let normalized = match v.checked_mul(1000) {
                    Some(v2) => v2,
                    None => total,
                };
                match normalized.checked_div(sqrt_factor) {
                    Some(v3) => v3 / 1000,
                    None => total,
                }
            }
            None => total,
        };
        
        vested.min(total)
    }
    
    /// Integer square root using binary search
    fn integer_sqrt(n: u128) -> u128 {
        if n <= 1 {
            return n;
        }
        
        let mut low = 1u128;
        let mut high = n;
        let mut result = 1u128;
        
        while low <= high {
            let mid = (low + high) / 2;
            let mid_squared = match mid.checked_mul(mid) {
                Some(v) => v,
                None => {
                    high = mid - 1;
                    continue;
                }
            };
            
            if mid_squared == n {
                return mid;
            }
            
            if mid_squared < n {
                low = mid + 1;
                result = mid;
            } else {
                high = mid - 1;
            }
        }
        
        result
    }
}
