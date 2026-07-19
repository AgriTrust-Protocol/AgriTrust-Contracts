use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, Vec};

use crate::errors::Error;

pub const MAX_SPLIT_CHILDREN: u32 = 16;
pub const MAX_MERGE_SOURCES: u32 = 16;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustodyEvent {
    pub event_id: u64,
    pub provenance_token_id: u64,
    pub custodian: Address,
    pub event_type: BytesN<32>,
    pub data_root: BytesN<32>,
    pub occurred_at: u64,
    pub locked: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Certificate {
    pub cert_type: BytesN<32>,
    pub issuer: Address,
    pub data_root: BytesN<32>,
    pub expires_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProvenanceToken {
    pub token_id: u64,
    pub owner: Address,
    pub batch_id: BytesN<32>,
    pub commodity_type: BytesN<32>,
    pub quantity: i128,
    pub origin_farm: Address,
    pub harvest_date: u64,
    pub metadata_cid_root: BytesN<32>,
    pub event_ids: Vec<u64>,
    pub certificate_ids: Vec<u64>,
    pub expires_at: u64,
    pub burned: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MerkleProof {
    pub leaf: BytesN<32>,
    pub siblings: Vec<BytesN<32>>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    NextTokenId,
    NextEventId,
    NextCertificateId,
    Token(u64),
    Event(u64),
    Certificate(u64),
    Custodian(Address),
    CertificateType(BytesN<32>),
    Compliance(BytesN<32>),
}

fn require_admin(env: &Env) -> Result<Address, Error> {
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .ok_or(Error::NotInitialized)?;
    Ok(admin)
}

fn next_u64(env: &Env, key: DataKey) -> u64 {
    let next = env.storage().persistent().get::<_, u64>(&key).unwrap_or(1);
    env.storage().persistent().set(&key, &(next + 1));
    next
}

fn get_live_token(env: &Env, token_id: u64) -> Result<ProvenanceToken, Error> {
    let token: ProvenanceToken = env
        .storage()
        .persistent()
        .get(&DataKey::Token(token_id))
        .ok_or(Error::TokenNotFound)?;
    if token.burned || (token.expires_at != 0 && env.ledger().timestamp() >= token.expires_at) {
        return Err(Error::TokenBurned);
    }
    Ok(token)
}

fn store_token(env: &Env, token: &ProvenanceToken) {
    env.storage()
        .persistent()
        .set(&DataKey::Token(token.token_id), token);
}

pub fn initialize(env: &Env, admin: Address) {
    if !env.storage().persistent().has(&DataKey::Admin) {
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Admin, &admin);
    }
}

pub fn set_authorized_custodian(
    env: &Env,
    custodian: Address,
    authorized: bool,
) -> Result<(), Error> {
    require_admin(env)?;
    env.storage()
        .persistent()
        .set(&DataKey::Custodian(custodian), &authorized);
    Ok(())
}

pub fn register_certificate_type(
    env: &Env,
    cert_type: BytesN<32>,
    active: bool,
) -> Result<(), Error> {
    require_admin(env)?;
    env.storage()
        .persistent()
        .set(&DataKey::CertificateType(cert_type), &active);
    Ok(())
}

pub fn set_compliance_standard(
    env: &Env,
    standard_id: BytesN<32>,
    required_types: Vec<BytesN<32>>,
) -> Result<(), Error> {
    require_admin(env)?;
    env.storage()
        .persistent()
        .set(&DataKey::Compliance(standard_id), &required_types);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn mint_batch(
    env: &Env,
    owner: Address,
    batch_id: BytesN<32>,
    commodity_type: BytesN<32>,
    quantity: i128,
    origin_farm: Address,
    harvest_date: u64,
    metadata_cid_root: BytesN<32>,
    expires_at: u64,
) -> Result<u64, Error> {
    require_admin(env)?;
    if quantity <= 0 {
        return Err(Error::InvalidQuantity);
    }
    let token_id = next_u64(env, DataKey::NextTokenId);
    let token = ProvenanceToken {
        token_id,
        owner,
        batch_id,
        commodity_type,
        quantity,
        origin_farm,
        harvest_date,
        metadata_cid_root,
        event_ids: Vec::new(env),
        certificate_ids: Vec::new(env),
        expires_at,
        burned: false,
    };
    store_token(env, &token);
    env.events()
        .publish((symbol_short!("prov"), symbol_short!("mint")), token_id);
    Ok(token_id)
}

pub fn owner_of(env: &Env, token_id: u64) -> Result<Address, Error> {
    Ok(get_live_token(env, token_id)?.owner)
}

pub fn transfer(env: &Env, from: Address, to: Address, token_id: u64) -> Result<(), Error> {
    from.require_auth();
    let mut token = get_live_token(env, token_id)?;
    if token.owner != from {
        return Err(Error::Unauthorized);
    }
    token.owner = to;
    store_token(env, &token);
    Ok(())
}

pub fn add_custody_event(
    env: &Env,
    token_id: u64,
    custodian: Address,
    event_type: BytesN<32>,
    data_root: BytesN<32>,
) -> Result<u64, Error> {
    custodian.require_auth();
    let authorized = env
        .storage()
        .persistent()
        .get::<_, bool>(&DataKey::Custodian(custodian.clone()))
        .unwrap_or(false);
    if !authorized {
        return Err(Error::Unauthorized);
    }
    let mut token = get_live_token(env, token_id)?;
    let event_id = next_u64(env, DataKey::NextEventId);
    let event = CustodyEvent {
        event_id,
        provenance_token_id: token_id,
        custodian,
        event_type,
        data_root,
        occurred_at: env.ledger().timestamp(),
        locked: true,
    };
    env.storage()
        .persistent()
        .set(&DataKey::Event(event_id), &event);
    token.event_ids.push_back(event_id);
    store_token(env, &token);
    Ok(event_id)
}

pub fn transfer_event(
    _env: &Env,
    _from: Address,
    _to: Address,
    _event_id: u64,
) -> Result<(), Error> {
    Err(Error::SoulboundTransfer)
}

pub fn attach_certificate(
    env: &Env,
    token_id: u64,
    cert_type: BytesN<32>,
    issuer: Address,
    data_root: BytesN<32>,
    expires_at: u64,
) -> Result<u64, Error> {
    issuer.require_auth();
    if !env
        .storage()
        .persistent()
        .get::<_, bool>(&DataKey::CertificateType(cert_type.clone()))
        .unwrap_or(false)
    {
        return Err(Error::UnknownCertificateType);
    }
    if expires_at <= env.ledger().timestamp() {
        return Err(Error::CertificateExpired);
    }
    let mut token = get_live_token(env, token_id)?;
    let cert_id = next_u64(env, DataKey::NextCertificateId);
    let cert = Certificate {
        cert_type,
        issuer,
        data_root,
        expires_at,
    };
    env.storage()
        .persistent()
        .set(&DataKey::Certificate(cert_id), &cert);
    token.certificate_ids.push_back(cert_id);
    store_token(env, &token);
    Ok(cert_id)
}

pub fn active_certificates(env: &Env, token_id: u64) -> Result<Vec<Certificate>, Error> {
    let token = get_live_token(env, token_id)?;
    let mut active = Vec::new(env);
    for id in token.certificate_ids.iter() {
        let cert: Certificate = env
            .storage()
            .persistent()
            .get(&DataKey::Certificate(id))
            .ok_or(Error::CertificateNotFound)?;
        if cert.expires_at > env.ledger().timestamp() {
            active.push_back(cert);
        }
    }
    Ok(active)
}

pub fn verify_compliance(env: &Env, token_id: u64, standard_id: BytesN<32>) -> Result<bool, Error> {
    let required: Vec<BytesN<32>> = env
        .storage()
        .persistent()
        .get(&DataKey::Compliance(standard_id))
        .ok_or(Error::UnknownComplianceStandard)?;
    let active = active_certificates(env, token_id)?;
    for needed in required.iter() {
        let mut found = false;
        for cert in active.iter() {
            if cert.cert_type == needed {
                found = true;
                break;
            }
        }
        if !found {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn split(
    env: &Env,
    owner: Address,
    token_id: u64,
    quantities: Vec<i128>,
) -> Result<Vec<u64>, Error> {
    owner.require_auth();
    let mut token = get_live_token(env, token_id)?;
    if token.owner != owner {
        return Err(Error::Unauthorized);
    }
    if quantities.len() == 0 || quantities.len() > MAX_SPLIT_CHILDREN {
        return Err(Error::InvalidQuantity);
    }
    let mut sum = 0i128;
    for q in quantities.iter() {
        if q <= 0 {
            return Err(Error::InvalidQuantity);
        }
        sum = sum.saturating_add(q);
    }
    if sum != token.quantity {
        return Err(Error::InvalidQuantity);
    }
    token.burned = true;
    store_token(env, &token);
    let mut children = Vec::new(env);
    for q in quantities.iter() {
        let child_id = next_u64(env, DataKey::NextTokenId);
        let child = ProvenanceToken {
            token_id: child_id,
            owner: owner.clone(),
            batch_id: token.batch_id.clone(),
            commodity_type: token.commodity_type.clone(),
            quantity: q,
            origin_farm: token.origin_farm.clone(),
            harvest_date: token.harvest_date,
            metadata_cid_root: token.metadata_cid_root.clone(),
            event_ids: token.event_ids.clone(),
            certificate_ids: token.certificate_ids.clone(),
            expires_at: token.expires_at,
            burned: false,
        };
        store_token(env, &child);
        children.push_back(child_id);
    }
    Ok(children)
}

pub fn merge(
    env: &Env,
    owner: Address,
    token_ids: Vec<u64>,
    aggregate_batch_id: BytesN<32>,
    metadata_cid_root: BytesN<32>,
) -> Result<u64, Error> {
    owner.require_auth();
    if token_ids.len() == 0 || token_ids.len() > MAX_MERGE_SOURCES {
        return Err(Error::InvalidQuantity);
    }
    let first = get_live_token(env, token_ids.get_unchecked(0))?;
    if first.owner != owner {
        return Err(Error::Unauthorized);
    }
    let mut quantity = 0i128;
    let mut events = Vec::new(env);
    let mut certs = Vec::new(env);
    for id in token_ids.iter() {
        let mut token = get_live_token(env, id)?;
        if token.owner != owner || token.commodity_type != first.commodity_type {
            return Err(Error::IncompatibleBatch);
        }
        quantity = quantity.saturating_add(token.quantity);
        for e in token.event_ids.iter() {
            events.push_back(e);
        }
        for c in token.certificate_ids.iter() {
            certs.push_back(c);
        }
        token.burned = true;
        store_token(env, &token);
    }
    let token_id = next_u64(env, DataKey::NextTokenId);
    let merged = ProvenanceToken {
        token_id,
        owner,
        batch_id: aggregate_batch_id,
        commodity_type: first.commodity_type,
        quantity,
        origin_farm: first.origin_farm,
        harvest_date: first.harvest_date,
        metadata_cid_root,
        event_ids: events,
        certificate_ids: certs,
        expires_at: first.expires_at,
        burned: false,
    };
    store_token(env, &merged);
    Ok(token_id)
}

pub fn token(env: &Env, token_id: u64) -> Result<ProvenanceToken, Error> {
    get_live_token(env, token_id)
}

pub fn event(env: &Env, event_id: u64) -> Result<CustodyEvent, Error> {
    env.storage()
        .persistent()
        .get(&DataKey::Event(event_id))
        .ok_or(Error::EventNotFound)
}

pub fn verify_metadata(_env: &Env, root: BytesN<32>, proof: MerkleProof) -> bool {
    root == proof.leaf && proof.siblings.len() == 0
}

pub fn set_metadata_root(
    env: &Env,
    owner: Address,
    token_id: u64,
    metadata_cid_root: BytesN<32>,
) -> Result<(), Error> {
    owner.require_auth();
    let mut token = get_live_token(env, token_id)?;
    if token.owner != owner {
        return Err(Error::Unauthorized);
    }
    token.metadata_cid_root = metadata_cid_root;
    store_token(env, &token);
    Ok(())
}
