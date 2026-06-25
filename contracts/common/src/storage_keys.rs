use soroban_sdk::{Bytes, BytesN, Env};

/// Four-byte domain prefixes used to keep logical storage domains distinct before hashing.
pub const DOMAIN_GRANT: &[u8; 4] = b"GRNT";
pub const DOMAIN_VESTING: &[u8; 4] = b"VEST";
pub const DOMAIN_COMPLIANCE: &[u8; 4] = b"COMP";
pub const DOMAIN_TREASURY: &[u8; 4] = b"TRSY";

/// Derive a collision-resistant Soroban storage key from a domain, variant, and identifier.
///
/// The digest payload is `domain_prefix || variant_byte || identifier`. Keeping the domain
/// prefix in the preimage prevents same-identifier keys in different modules from overwriting
/// each other when contracts compose grant, vesting, compliance, and treasury features.
pub fn derive_storage_key(
    env: &Env,
    domain_prefix: &[u8; 4],
    variant: u8,
    identifier: &BytesN<32>,
) -> BytesN<32> {
    let mut payload = Bytes::from_array(env, domain_prefix);
    payload.push_back(variant);
    payload.append(&Bytes::from_array(env, &identifier.to_array()));
    env.crypto().sha256(&payload).into()
}

/// Legacy helper retained only for migrations/tests. It intentionally omits the domain prefix.
pub fn derive_legacy_storage_key(env: &Env, variant: u8, identifier: &BytesN<32>) -> BytesN<32> {
    let mut payload = Bytes::new(env);
    payload.push_back(variant);
    payload.append(&Bytes::from_array(env, &identifier.to_array()));
    env.crypto().sha256(&payload).into()
}
