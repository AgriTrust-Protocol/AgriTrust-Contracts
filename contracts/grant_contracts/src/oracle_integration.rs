use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, Map, Symbol, Vec, i128,
};

// --- SEP-40 Oracle Integration Constants ---
pub const MAX_STALENESS_SECONDS: u64 = 1200; // 20 minutes
pub const PRICE_SCALING_FACTOR: i128 = 1_000_000; // 6 decimal places for precision

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct OraclePriceFeed {
    pub base_asset: Address,
    pub quote_asset: Address,
    pub price: i128,
    pub timestamp: u64,
    pub decimals: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PricePair {
    pub base: Address,
    pub quote: Address,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[contracterror]
pub enum OracleError {
    StalePriceData = 1,
    InvalidPrice = 2,
    OracleNotFound = 3,
    MathOverflow = 4,
    InvalidAssets = 5,
}

/// SEP-40 Oracle Interface Trait
pub trait SEP40Oracle {
    fn get_price(env: &Env, base: Address, quote: Address) -> Result<OraclePriceFeed, OracleError>;
    fn get_timestamp(env: &Env, base: Address, quote: Address) -> Result<u64, OracleError>;
}

/// Oracle Implementation for Grant-Stream
pub struct GrantStreamOracle;

#[contractimpl]
impl GrantStreamOracle {
    /// Get current price for asset pair with staleness check
    pub fn get_current_price(env: Env, base: Address, quote: Address) -> Result<OraclePriceFeed, OracleError> {
        let price_feed = Self::get_price(&env, base.clone(), quote.clone())?;
        
        // Verify price freshness
        Self::verify_price_freshness(price_feed.timestamp)?;
        
        // Validate price
        if price_feed.price <= 0 {
            return Err(OracleError::InvalidPrice);
        }
        
        Ok(price_feed)
    }
    
    /// Verify price data is not stale
    pub fn verify_price_freshness(timestamp: u64) -> Result<(), OracleError> {
        let current_time = env.ledger().timestamp();
        
        if current_time.saturating_sub(timestamp) > MAX_STALENESS_SECONDS {
            return Err(OracleError::StalePriceData);
        }
        
        Ok(())
    }
    
    /// Convert USD amount to XLM using current oracle price
    pub fn convert_usd_to_xlm(env: Env, usd_amount: i128) -> Result<i128, OracleError> {
        if usd_amount <= 0 {
            return Err(OracleError::InvalidAmount);
        }
        
        // Get XLM/USDC price (XLM as base, USDC as quote)
        let xlm_address = Address::from_string(&env, String::from_str(&env, "XLM"));
        let usdc_address = Address::from_string(&env, String::from_str(&env, "USDC"));
        
        let price_feed = Self::get_current_price(env, xlm_address, usdc_address)?;
        
        // Convert using fixed-point arithmetic
        // USD amount * SCALING_FACTOR / XLM price
        let scaled_amount = usd_amount
            .checked_mul(PRICE_SCALING_FACTOR)
            .ok_or(OracleError::MathOverflow)?;
            
        let xlm_amount = scaled_amount
            .checked_div(price_feed.price)
            .ok_or(OracleError::MathOverflow)?;
        
        Ok(xlm_amount)
    }
    
    /// Convert XLM amount to USD using current oracle price
    pub fn convert_xlm_to_usd(env: Env, xlm_amount: i128) -> Result<i128, OracleError> {
        if xlm_amount <= 0 {
            return Err(OracleError::InvalidAmount);
        }
        
        // Get XLM/USDC price (XLM as base, USDC as quote)
        let xlm_address = Address::from_string(&env, String::from_str(&env, "XLM"));
        let usdc_address = Address::from_string(&env, String::from_str(&env, "USDC"));
        
        let price_feed = Self::get_current_price(env, xlm_address, usdc_address)?;
        
        // Convert using fixed-point arithmetic
        // XLM amount * price / SCALING_FACTOR
        let usd_scaled = xlm_amount
            .checked_mul(price_feed.price)
            .ok_or(OracleError::MathOverflow)?;
            
        let usd_amount = usd_scaled
            .checked_div(PRICE_SCALING_FACTOR)
            .ok_or(OracleError::MathOverflow)?;
        
        Ok(usd_amount)
    }
    
    /// Get price for any asset pair
    pub fn get_price(env: &Env, base: Address, quote: Address) -> Result<OraclePriceFeed, OracleError> {
        // In a real implementation, this would call the actual SEP-40 oracle contract
        // For now, we'll simulate with storage-based price feeds
        
        let price_key = Symbol::new(&env, &format!("price_{}_{}", base, quote));
        
        if let Some(price_feed) = env.storage().instance().get::<Symbol, OraclePriceFeed>(&price_key) {
            Ok(price_feed)
        } else {
            // Try reverse pair
            let reverse_key = Symbol::new(&env, &format!("price_{}_{}", quote, base));
            if let Some(reverse_feed) = env.storage().instance().get::<Symbol, OraclePriceFeed>(&reverse_key) {
                // Calculate inverse price
                let inverse_price = PRICE_SCALING_FACTOR
                    .checked_mul(PRICE_SCALING_FACTOR)
                    .ok_or(OracleError::MathOverflow)?
                    .checked_div(reverse_feed.price)
                    .ok_or(OracleError::MathOverflow)?;
                
                Ok(OraclePriceFeed {
                    base_asset: base,
                    quote_asset: quote,
                    price: inverse_price,
                    timestamp: reverse_feed.timestamp,
                    decimals: reverse_feed.decimals,
                })
            } else {
                Err(OracleError::OracleNotFound)
            }
        }
    }
    
    /// Update price feed (admin only)
    pub fn update_price_feed(
        env: Env,
        admin: Address,
        base: Address,
        quote: Address,
        price: i128,
        timestamp: u64,
        decimals: u32,
    ) -> Result<(), OracleError> {
        // Verify admin authorization
        admin.require_auth();
        
        if price <= 0 {
            return Err(OracleError::InvalidPrice);
        }
        
        if base == quote {
            return Err(OracleError::InvalidAssets);
        }
        
        let price_feed = OraclePriceFeed {
            base_asset: base.clone(),
            quote_asset: quote.clone(),
            price,
            timestamp,
            decimals,
        };
        
        let price_key = Symbol::new(&env, &format!("price_{}_{}", base, quote));
        env.storage().instance().set(&price_key, price_feed);
        
        Ok(())
    }
    
    /// Get all supported asset pairs
    pub fn get_supported_pairs(env: Env) -> Result<Vec<PricePair>, OracleError> {
        let pairs_key = Symbol::new(&env, "supported_pairs");
        
        if let Some(pairs) = env.storage().instance().get::<Symbol, Vec<PricePair>>(&pairs_key) {
            Ok(pairs)
        } else {
            Ok(Vec::new(&env))
        }
    }
    
    /// Add supported asset pair (admin only)
    pub fn add_supported_pair(env: Env, admin: Address, base: Address, quote: Address) -> Result<(), OracleError> {
        admin.require_auth();
        
        if base == quote {
            return Err(OracleError::InvalidAssets);
        }
        
        let pairs_key = Symbol::new(&env, "supported_pairs");
        let mut pairs = Self::get_supported_pairs(env.clone())?;
        
        let new_pair = PricePair { base, quote };
        
        // Check if pair already exists
        for pair in pairs.iter() {
            if pair.base == new_pair.base && pair.quote == new_pair.quote {
                return Ok(()); // Pair already exists
            }
        }
        
        pairs.push_back(new_pair);
        env.storage().instance().set(&pairs_key, pairs);
        
        Ok(())
    }
}

impl SEP40Oracle for GrantStreamOracle {
    fn get_price(env: &Env, base: Address, quote: Address) -> Result<OraclePriceFeed, OracleError> {
        Self::get_price(env, base, quote)
    }
    
    fn get_timestamp(env: &Env, base: Address, quote: Address) -> Result<u64, OracleError> {
        let price_feed = Self::get_price(env, base, quote)?;
        Ok(price_feed.timestamp)
    }
}
