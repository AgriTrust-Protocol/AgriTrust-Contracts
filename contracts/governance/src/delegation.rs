use soroban_sdk::{Address, Env};
use soroban_sdk::contracttype;

use crate::StorageKey;
use crate::proposal_manager::ProposalType;

#[derive(Clone, Debug)]
#[contracttype]
pub struct Delegation {
    pub delegator: Address,
    pub delegate: Address,
    pub proposal_type: ProposalType,
}

pub struct DelegationManager;

impl DelegationManager {
    pub fn set_delegate(
        env: &Env,
        delegator: &Address,
        delegate: &Address,
        proposal_type: &ProposalType,
    ) {
        if delegator == delegate {
            panic!("cannot delegate to self");
        }

        let delegation = Delegation {
            delegator: delegator.clone(),
            delegate: delegate.clone(),
            proposal_type: proposal_type.clone(),
        };

        let key = StorageKey::Delegation(delegator.clone(), proposal_type.clone());
        env.storage().instance().set(&key, &delegation);

        let count: u64 = env.storage().instance().get(&StorageKey::DelegationCount).unwrap_or(0);
        env.storage().instance().set(&StorageKey::DelegationCount, &(count + 1));

        env.events().publish(
            (soroban_sdk::symbol_short!("delegate"),),
            (delegator.clone(), delegate.clone(), proposal_type.clone()),
        );
    }

    pub fn remove_delegate(
        env: &Env,
        delegator: &Address,
        proposal_type: &ProposalType,
    ) {
        let key = StorageKey::Delegation(delegator.clone(), proposal_type.clone());
        if env.storage().instance().has(&key) {
            env.storage().instance().remove(&key);
        }

        env.events().publish(
            (soroban_sdk::symbol_short!("undeleg"),),
            (delegator.clone(), proposal_type.clone()),
        );
    }

    pub fn get_delegate(
        env: &Env,
        delegator: &Address,
        proposal_type: &ProposalType,
    ) -> Option<Address> {
        let key = StorageKey::Delegation(delegator.clone(), proposal_type.clone());
        env.storage()
            .instance()
            .get::<_, Delegation>(&key)
            .map(|d| d.delegate)
    }

    pub fn resolve_voter(
        env: &Env,
        voter: &Address,
        proposal_type: &ProposalType,
    ) -> Address {
        let key = StorageKey::Delegation(voter.clone(), proposal_type.clone());
        env.storage()
            .instance()
            .get::<_, Delegation>(&key)
            .map_or(voter.clone(), |d| d.delegate)
    }
}
