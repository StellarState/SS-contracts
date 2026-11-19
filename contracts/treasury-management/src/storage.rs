//! Storage helpers for the platform treasury management contract.

use soroban_sdk::{Address, Env, Vec};

use crate::types::{AllocationRecord, StorageKey};

/// Ledgers below which a persistent entry's TTL is extended (~7 days at 5s/ledger).
pub const TTL_THRESHOLD: u32 = 120_960;
/// Minimum TTL extension in ledger units (~60 days at 5s/ledger).
pub const MIN_TTL_EXTEND: u32 = 1_036_800;

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&StorageKey::Admin, admin);
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
    env.storage().instance().get(&StorageKey::Admin)
}

pub fn get_total_balance(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&StorageKey::TotalBalance)
        .unwrap_or(0)
}

pub fn set_total_balance(env: &Env, amount: i128) {
    env.storage()
        .instance()
        .set(&StorageKey::TotalBalance, &amount);
}

pub fn get_known_sources(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&StorageKey::KnownSources)
        .unwrap_or(Vec::new(env))
}

/// Append `source` to the known-sources list if it isn't already present.
fn remember_source(env: &Env, source: &Address) {
    let mut sources = get_known_sources(env);
    if !sources.contains(source) {
        sources.push_back(source.clone());
        env.storage()
            .instance()
            .set(&StorageKey::KnownSources, &sources);
    }
}

pub fn get_source_balance(env: &Env, source: &Address) -> i128 {
    let key = StorageKey::SourceBalance(source.clone());
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    data.unwrap_or(0)
}

/// Record that `source` contributed `amount`, updating its per-source
/// balance, the running total, and the known-sources list.
pub fn credit_source(env: &Env, source: &Address, amount: i128) {
    remember_source(env, source);
    let key = StorageKey::SourceBalance(source.clone());
    let balance = get_source_balance(env, source);
    env.storage().persistent().set(&key, &(balance + amount));
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn get_allocation_count(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&StorageKey::AllocationCount)
        .unwrap_or(0)
}

pub fn get_allocation(env: &Env, index: u32) -> Option<AllocationRecord> {
    let key = StorageKey::Allocation(index);
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    data
}

/// Append a new allocation record, returning its assigned index.
pub fn push_allocation(env: &Env, record: &AllocationRecord) -> u32 {
    let index = get_allocation_count(env);
    let key = StorageKey::Allocation(index);
    env.storage().persistent().set(&key, record);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    env.storage()
        .instance()
        .set(&StorageKey::AllocationCount, &(index + 1));
    index
}
