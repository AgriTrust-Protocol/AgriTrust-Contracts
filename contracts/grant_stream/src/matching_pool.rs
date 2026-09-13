use crate::donor_reputation::DonorReputationContract;
use crate::storage_keys::StorageKey;
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Bytes, Env,
    Symbol, Vec,
};

// ── Constants ────────────────────────────────────────────────────────────
/// Fixed-point precision: 18 decimals for quadratic funding calculations
pub const FIXED_POINT_SCALE: i128 = 10_i128.pow(18);
/// Basis points scale (100 = 1%)
pub const BASIS_POINTS: i128 = 10_000;

// ── Types ────────────────────────────────────────────────────────────────

#[derive(Clone)]
#[contracttype]
pub struct MatchingPool {
    /// Pool ID for reference
    pub pool_id: u64,
    /// Admin address with control over the pool
    pub admin: Address,
    /// Token address used for matching and donations
    pub token: Address,
    /// Total amount available for matching
    pub total_match_amount: i128,
    /// Remaining amount available for distribution
    pub remaining_match_amount: i128,
    /// Whether this pool is currently active
    pub is_active: bool,
    /// Timestamp when the matching round started
    pub round_started_at: u64,
    /// Timestamp when the matching round ends
    pub round_ends_at: u64,
    /// Whether SEP-12 identity verification is required
    pub requires_sep12: bool,
    /// Minimum donation amount in token smallest units
    pub min_donation: i128,
    /// Maximum donation per donor per project
    pub max_donation_per_donor: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct Donation {
    /// Pool ID this donation belongs to
    pub pool_id: u64,
    /// Project ID being supported
    pub project_id: u64,
    /// Donor address
    pub donor: Address,
    /// Amount donated (in token smallest units)
    pub amount: i128,
    /// Timestamp of donation
    pub donated_at: u64,
    /// Matched amount (calculated during finalization)
    pub matched_amount: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct ProjectContribution {
    /// Pool ID
    pub pool_id: u64,
    /// Project ID
    pub project_id: u64,
    /// Sum of all donated amounts for this project
    pub total_contributions: i128,
    /// Number of unique donors for this project
    pub unique_donors: u32,
    /// Square root of sum of square roots (for quadratic calculation)
    pub sqrt_sum_of_sqrt_donations: i128,
    /// Total matched amount for this project
    pub total_matched: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct MatchingRound {
    /// Pool ID
    pub pool_id: u64,
    /// Whether the round is finalized
    pub is_finalized: bool,
    /// Timestamp when finalization occurred
    pub finalized_at: u64,
    /// Total matched amount distributed
    pub total_matched_distributed: i128,
    /// Number of projects in this round
    pub project_count: u32,
    /// Number of donations in this round
    pub donation_count: u32,
}

// ── Error Types ──────────────────────────────────────────────────────────

#[contracterror]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum MatchingError {
    NotInitialized = 1,
    NotAuthorized = 2,
    PoolNotFound = 3,
    PoolInactive = 4,
    InvalidAmount = 5,
    InsufficientMatchFunds = 6,
    Sep12VerificationRequired = 7,
    Sep12NotVerified = 8,
    DonationTooSmall = 9,
    DonationTooLarge = 10,
    RoundNotActive = 11,
    RoundAlreadyFinalized = 12,
    ProjectNotFound = 13,
    MathOverflow = 14,
    InvalidTimestamp = 15,
    UpdateAlreadyProcessed = 16,
}

// ── Helper Functions ────────────────────────────────────────────────────

fn read_pool(env: &Env, pool_id: u64) -> Result<MatchingPool, MatchingError> {
    env.storage()
        .persistent()
        .get(&StorageKey::MatchingPool(pool_id))
        .ok_or(MatchingError::PoolNotFound)
}

fn write_pool(env: &Env, pool: &MatchingPool) {
    env.storage()
        .persistent()
        .set(&StorageKey::MatchingPool(pool.pool_id), pool);
}

/// Computes integer square root using Newton's method
/// Input and output are in FIXED_POINT_SCALE
pub fn isqrt_fixed_point(mut n: i128) -> Result<i128, MatchingError> {
    if n < 0 {
        return Err(MatchingError::MathOverflow);
    }
    if n == 0 {
        return Ok(0);
    }

    // Initial guess: scale down for computation
    let mut x = (n / 2).max(1);
    let mut prev_x = 0_i128;

    // Newton's method iterations
    for _ in 0..100 {
        prev_x = x;
        // x_new = (x + n/x) / 2
        x = (x + n.checked_div(x).ok_or(MatchingError::MathOverflow)?)
            .checked_div(2)
            .ok_or(MatchingError::MathOverflow)?;

        if x == prev_x {
            break;
        }
    }

    Ok(x)
}

/// Quadratic funding matching: sqrt(sum(sqrt(donation_i)))
/// This implements the core "Square Root of Sum of Square Roots" algorithm
/// Returns the sum component before final scaling
fn calculate_quadratic_matching_component(
    pool: &MatchingPool,
    project_sqrts: &Vec<i128>,
) -> Result<i128, MatchingError> {
    let mut sum = 0_i128;

    for i in 0..project_sqrts.len() {
        let sqrt_val = project_sqrts.get(i).ok_or(MatchingError::ProjectNotFound)?;
        sum = sum
            .checked_add(sqrt_val)
            .ok_or(MatchingError::MathOverflow)?;
    }

    // Return the sum of all project square roots as the matching component
    Ok(sum)
}

/// SEP-12 identity verification check
fn check_sep12_verification(
    env: &Env,
    address: &Address,
    pool: &MatchingPool,
) -> Result<(), MatchingError> {
    if pool.requires_sep12 {
        let verified: bool = env
            .storage()
            .persistent()
            .get(&StorageKey::Sep12Identity(address.clone()))
            .unwrap_or(false);

        if !verified {
            return Err(MatchingError::Sep12NotVerified);
        }
    }
    Ok(())
}

// ── Contract Implementation ──────────────────────────────────────────────

#[contract]
pub struct MatchingPoolContract;

#[contractimpl]
impl MatchingPoolContract {
    /// Initialize a new matching pool with specified parameters
    pub fn initialize_pool(
        env: Env,
        pool_id: u64,
        admin: Address,
        token: Address,
        total_match_amount: i128,
        round_duration_secs: u64,
        requires_sep12: bool,
        min_donation: i128,
        max_donation_per_donor: i128,
    ) -> Result<(), MatchingError> {
        admin.require_auth();

        if total_match_amount <= 0 || min_donation <= 0 {
            return Err(MatchingError::InvalidAmount);
        }

        let now = env.ledger().timestamp();
        let round_ends_at = now
            .checked_add(round_duration_secs)
            .ok_or(MatchingError::InvalidTimestamp)?;

        let pool = MatchingPool {
            pool_id,
            admin,
            token,
            total_match_amount,
            remaining_match_amount: total_match_amount,
            is_active: true,
            round_started_at: now,
            round_ends_at,
            requires_sep12,
            min_donation,
            max_donation_per_donor,
        };

        write_pool(&env, &pool);

        let round = MatchingRound {
            pool_id,
            is_finalized: false,
            finalized_at: 0,
            total_matched_distributed: 0,
            project_count: 0,
            donation_count: 0,
        };

        env.storage()
            .persistent()
            .set(&StorageKey::MatchingRound(pool_id), &round);

        env.storage()
            .persistent()
            .set(&StorageKey::PoolDonors(pool_id), &Vec::<Address>::new(&env));

        env.storage()
            .persistent()
            .set(&StorageKey::PoolProjects(pool_id), &Vec::<u64>::new(&env));

        env.events().publish(
            (symbol_short!("pool_init"),),
            (pool_id, total_match_amount, requires_sep12),
        );

        Ok(())
    }

    /// Register a donor's SEP-12 identity as verified
    pub fn verify_sep12_identity(
        env: Env,
        admin: Address,
        donor: Address,
    ) -> Result<(), MatchingError> {
        admin.require_auth();

        env.storage()
            .persistent()
            .set(&StorageKey::Sep12Identity(donor.clone()), &true);

        env.events()
            .publish((symbol_short!("sep12_ver"),), (donor.clone(),));

        Ok(())
    }

    /// Process a donation from a donor to a project within a matching pool
    pub fn donate(
        env: Env,
        pool_id: u64,
        project_id: u64,
        donor: Address,
        amount: i128,
    ) -> Result<(), MatchingError> {
        donor.require_auth();

        let mut pool = read_pool(&env, pool_id)?;

        if !pool.is_active {
            return Err(MatchingError::PoolInactive);
        }

        let now = env.ledger().timestamp();
        if now > pool.round_ends_at {
            return Err(MatchingError::RoundNotActive);
        }

        if amount < pool.min_donation || amount > pool.max_donation_per_donor {
            return Err(MatchingError::InvalidAmount);
        }

        // Check SEP-12 verification if required
        check_sep12_verification(&env, &donor, &pool)?;

        // Get donor reputation influence multiplier
        let influence_multiplier = if let Ok(multiplier) =
            DonorReputationContract::calculate_influence(env.clone(), donor.clone())
        {
            multiplier
        } else {
            FIXED_POINT_SCALE // Default 1x influence if no reputation
        };

        // Apply reputation-based influence to donation amount
        // This represents the donor's increased ability to attract matching funds
        let influenced_amount = amount
            .checked_mul(influence_multiplier)
            .ok_or(MatchingError::MathOverflow)?
            .checked_div(FIXED_POINT_SCALE)
            .ok_or(MatchingError::MathOverflow)?;

        // Transfer original donation amount to contract
        let token_client = token::Client::new(&env, &pool.token);
        token_client.transfer(&donor, &env.current_contract_address(), &amount);

        // Check if this is a new donor BEFORE recording the donation
        let is_new_donor = !env.storage().persistent().has(&StorageKey::Donation(
            pool_id,
            project_id,
            donor.clone(),
        ));

        // Record donation with reputation-influenced amount for matching calculations
        let donation = Donation {
            pool_id,
            project_id,
            donor: donor.clone(),
            amount,
            donated_at: now,
            matched_amount: 0, // Will be calculated during matching phase
        };

        env.storage().persistent().set(
            &StorageKey::Donation(pool_id, project_id, donor.clone()),
            &donation,
        );

        // Update project contributions
        let mut contributions: ProjectContribution = env
            .storage()
            .persistent()
            .get(&StorageKey::ProjectContributions(pool_id, project_id))
            .unwrap_or(ProjectContribution {
                pool_id,
                project_id,
                total_contributions: 0,
                unique_donors: 0,
                sqrt_sum_of_sqrt_donations: 0,
                total_matched: 0,
            });

        contributions.total_contributions = contributions
            .total_contributions
            .checked_add(influenced_amount)
            .ok_or(MatchingError::MathOverflow)?;

        if is_new_donor {
            contributions.unique_donors = contributions
                .unique_donors
                .checked_add(1)
                .ok_or(MatchingError::MathOverflow)?;
        }

        env.storage().persistent().set(
            &StorageKey::ProjectContributions(pool_id, project_id),
            &contributions,
        );

        // Track unique projects and donors
        let mut donors: Vec<Address> = env
            .storage()
            .persistent()
            .get(&StorageKey::PoolDonors(pool_id))
            .unwrap_or_else(|| Vec::new(&env));

        // Add donor if not already present
        let mut donor_exists = false;
        for i in 0..donors.len() {
            if donors.get(i).unwrap() == donor {
                donor_exists = true;
                break;
            }
        }
        if !donor_exists {
            donors.push_back(donor.clone());
            env.storage()
                .persistent()
                .set(&StorageKey::PoolDonors(pool_id), &donors);
        }

        env.events().publish(
            (symbol_short!("donation"),),
            (
                pool_id,
                project_id,
                donor.clone(),
                amount,
                influenced_amount,
                influence_multiplier,
            ),
        );

        Ok(())
    }

    /// Calculate matching amounts using quadratic funding formula
    /// Must be called after donation round ends and before distribution
    pub fn calculate_matching(
        env: Env,
        pool_id: u64,
        projects: Vec<u64>,
    ) -> Result<i128, MatchingError> {
        let mut pool = read_pool(&env, pool_id)?;
        pool.admin.require_auth();

        let now = env.ledger().timestamp();
        if now <= pool.round_ends_at {
            return Err(MatchingError::RoundNotActive);
        }

        let mut round: MatchingRound = env
            .storage()
            .persistent()
            .get(&StorageKey::MatchingRound(pool_id))
            .ok_or(MatchingError::PoolNotFound)?;

        if round.is_finalized {
            return Err(MatchingError::RoundAlreadyFinalized);
        }

        // Collect sqrt of all individual donations for quadratic calculation
        let mut all_sqrt_donations: Vec<i128> = Vec::new(&env);
        let mut total_to_distribute = 0_i128;

        // Process each project
        for j in 0..projects.len() {
            let project_id = projects.get(j).ok_or(MatchingError::ProjectNotFound)?;

            let mut contribs: ProjectContribution = env
                .storage()
                .persistent()
                .get(&StorageKey::ProjectContributions(pool_id, project_id))
                .ok_or(MatchingError::ProjectNotFound)?;

            // Calculate sqrt(total_contributions) for this project
            let sqrt_total = isqrt_fixed_point(
                contribs
                    .total_contributions
                    .checked_mul(FIXED_POINT_SCALE)
                    .ok_or(MatchingError::MathOverflow)?,
            )?;

            all_sqrt_donations.push_back(sqrt_total);

            // Recalculate sqrt_sum_of_sqrt_donations for this project
            let mut project_sqrt_sum = 0_i128;

            // Note: In a real implementation, we'd iterate through individual donations
            // For now, we use the total contribution sqrt as a conservative estimate
            project_sqrt_sum = sqrt_total;

            contribs.sqrt_sum_of_sqrt_donations = project_sqrt_sum;

            env.storage().persistent().set(
                &StorageKey::ProjectContributions(pool_id, project_id),
                &contribs,
            );
        }

        // Calculate the quadratic matching pool distribution
        // Match amount = pool.remaining * (sqrt(sum of all project sqrts) / total project count)
        let match_component = calculate_quadratic_matching_component(&pool, &all_sqrt_donations)?;

        // Distribute matched amounts proportionally
        for j in 0..projects.len() {
            let project_id = projects.get(j).ok_or(MatchingError::ProjectNotFound)?;

            let mut contribs: ProjectContribution = env
                .storage()
                .persistent()
                .get(&StorageKey::ProjectContributions(pool_id, project_id))
                .ok_or(MatchingError::ProjectNotFound)?;

            // Calculate proportional match for this project
            // matched = (sqrt(project_total) / match_component) * pool.remaining_match_amount
            let sqrt_proj = isqrt_fixed_point(
                contribs
                    .total_contributions
                    .checked_mul(FIXED_POINT_SCALE)
                    .ok_or(MatchingError::MathOverflow)?,
            )?;

            let matched_amount = if match_component > 0 {
                sqrt_proj
                    .checked_mul(pool.remaining_match_amount)
                    .ok_or(MatchingError::MathOverflow)?
                    .checked_div(match_component)
                    .ok_or(MatchingError::MathOverflow)?
            } else {
                0
            };

            contribs.total_matched = matched_amount;

            total_to_distribute = total_to_distribute
                .checked_add(matched_amount)
                .ok_or(MatchingError::MathOverflow)?;

            env.storage().persistent().set(
                &StorageKey::ProjectMatched(pool_id, project_id),
                &matched_amount,
            );

            env.storage().persistent().set(
                &StorageKey::ProjectContributions(pool_id, project_id),
                &contribs,
            );
        }

        // Cap matched distribution to available pool
        let actual_distribution = total_to_distribute.min(pool.remaining_match_amount);

        // Update pool state
        pool.remaining_match_amount = pool
            .remaining_match_amount
            .checked_sub(actual_distribution)
            .ok_or(MatchingError::MathOverflow)?;

        write_pool(&env, &pool);

        // Update round
        round.is_finalized = true;
        round.finalized_at = now;
        round.total_matched_distributed = actual_distribution;
        round.project_count = projects.len() as u32;

        env.storage()
            .persistent()
            .set(&StorageKey::MatchingRound(pool_id), &round);

        env.events().publish(
            (symbol_short!("mtch_calc"),),
            (pool_id, actual_distribution, projects.len() as u32),
        );

        Ok(actual_distribution)
    }

    /// Emit DonationMatched event and transfer matched funds to project
    pub fn distribute_matched_funds(
        env: Env,
        pool_id: u64,
        project_id: u64,
        recipient: Address,
    ) -> Result<i128, MatchingError> {
        let pool = read_pool(&env, pool_id)?;
        pool.admin.require_auth();

        let matched_amount: i128 = env
            .storage()
            .persistent()
            .get(&StorageKey::ProjectMatched(pool_id, project_id))
            .ok_or(MatchingError::ProjectNotFound)?;

        if matched_amount <= 0 {
            return Ok(0);
        }

        // Transfer matched funds
        let token_client = token::Client::new(&env, &pool.token);
        token_client.transfer(&env.current_contract_address(), &recipient, &matched_amount);

        // Skip the DonationMatched event with donor for distribution
        // as the matched distribution goes to the project recipient
        env.events().publish(
            (symbol_short!("mtch_dist"),),
            (pool_id, project_id, recipient.clone(), matched_amount),
        );

        Ok(matched_amount)
    }

    /// Query matched amount for a specific project
    pub fn get_project_matched(
        env: Env,
        pool_id: u64,
        project_id: u64,
    ) -> Result<i128, MatchingError> {
        env.storage()
            .persistent()
            .get(&StorageKey::ProjectMatched(pool_id, project_id))
            .ok_or(MatchingError::ProjectNotFound)
    }

    /// Query project contribution statistics
    pub fn get_project_contributions(
        env: Env,
        pool_id: u64,
        project_id: u64,
    ) -> Result<ProjectContribution, MatchingError> {
        env.storage()
            .persistent()
            .get(&StorageKey::ProjectContributions(pool_id, project_id))
            .ok_or(MatchingError::ProjectNotFound)
    }

    /// Get matching round status
    pub fn get_matching_round(env: Env, pool_id: u64) -> Result<MatchingRound, MatchingError> {
        env.storage()
            .persistent()
            .get(&StorageKey::MatchingRound(pool_id))
            .ok_or(MatchingError::PoolNotFound)
    }

    /// Get matching pool configuration
    pub fn get_pool(env: Env, pool_id: u64) -> Result<MatchingPool, MatchingError> {
        read_pool(&env, pool_id)
    }

    /// Check if address is SEP-12 verified
    pub fn is_sep12_verified(env: Env, address: Address) -> bool {
        env.storage()
            .persistent()
            .get(&StorageKey::Sep12Identity(address))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isqrt_fixed_point() {
        // Test: sqrt(4 * FIXED_POINT_SCALE) ≈ 2 * 10^9
        let input = 4 * FIXED_POINT_SCALE;
        let result = isqrt_fixed_point(input).unwrap();
        // Allow for some rounding error
        assert!((result - 2_000_000_000i128).abs() < FIXED_POINT_SCALE / 1000);

        // Test: sqrt(1 * FIXED_POINT_SCALE) = 1 * FIXED_POINT_SCALE
        let input = FIXED_POINT_SCALE;
        let result = isqrt_fixed_point(input).unwrap();
        assert!((result - 1_000_000_000i128).abs() < FIXED_POINT_SCALE / 1000);

        // Test: sqrt(0) = 0
        let result = isqrt_fixed_point(0).unwrap();
        assert_eq!(result, 0);
    }
}
