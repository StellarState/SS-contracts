use soroban_sdk::{Address, Env, Symbol, Vec};

use crate::types::{DistributionState, FeeTier, StorageKey};

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&StorageKey::Admin, admin);
}

pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&StorageKey::Admin)
}

pub fn get_distribution(
    env: &Env,
    escrow: &Address,
    invoice_id: &Symbol,
) -> Option<DistributionState> {
    env.storage().persistent().get(&StorageKey::Distribution(
        escrow.clone(),
        invoice_id.clone(),
    ))
}

pub fn set_distribution(
    env: &Env,
    escrow: &Address,
    invoice_id: &Symbol,
    state: &DistributionState,
) {
    env.storage().persistent().set(
        &StorageKey::Distribution(escrow.clone(), invoice_id.clone()),
        state,
    );
}

pub fn get_fee_tiers(env: &Env) -> Option<Vec<FeeTier>> {
    env.storage().instance().get(&StorageKey::FeeTiers)
}

pub fn set_fee_tiers(env: &Env, tiers: &Vec<FeeTier>) {
    env.storage().instance().set(&StorageKey::FeeTiers, tiers);
}

pub fn get_investor_bonus_bps(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&StorageKey::InvestorBonusBps)
        .unwrap_or(0)
}

pub fn set_investor_bonus_bps(env: &Env, bonus_bps: u32) {
    env.storage()
        .instance()
        .set(&StorageKey::InvestorBonusBps, &bonus_bps);
}
