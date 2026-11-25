//! Storage helpers for the platform treasury management contract.

use soroban_sdk::{Address, Env, Vec};

use crate::types::{AllocationReceipt, AllocationRecord, DailyAllocation, StorageKey};

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

pub fn get_allocation_cooldown(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&StorageKey::AllocationCooldown)
        .unwrap_or(0)
}

pub fn set_allocation_cooldown(env: &Env, cooldown: u32) {
    env.storage()
        .instance()
        .set(&StorageKey::AllocationCooldown, &cooldown);
}

pub fn get_last_allocation_ledger(env: &Env) -> Option<u32> {
    env.storage()
        .instance()
        .get(&StorageKey::LastAllocationLedger)
}

pub fn set_last_allocation_ledger(env: &Env, ledger: u32) {
    env.storage()
        .instance()
        .set(&StorageKey::LastAllocationLedger, &ledger);
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

pub fn is_allowlist_enforced(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&StorageKey::SourceAllowlistEnforced)
        .unwrap_or(false)
}

pub fn set_allowlist_enforced(env: &Env, enforced: bool) {
    env.storage()
        .instance()
        .set(&StorageKey::SourceAllowlistEnforced, &enforced);
}

pub fn is_source_approved(env: &Env, source: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&StorageKey::ApprovedSource(source.clone()))
        .unwrap_or(false)
}

pub fn set_source_approved(env: &Env, source: &Address, approved: bool) {
    let key = StorageKey::ApprovedSource(source.clone());
    env.storage().persistent().set(&key, &approved);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn get_source_cap(env: &Env, source: &Address) -> Option<i128> {
    env.storage()
        .persistent()
        .get(&StorageKey::SourceCap(source.clone()))
}

pub fn set_source_cap(env: &Env, source: &Address, cap: Option<i128>) {
    let key = StorageKey::SourceCap(source.clone());
    match cap {
        Some(c) => {
            env.storage().persistent().set(&key, &c);
            env.storage()
                .persistent()
                .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
        }
        None => {
            env.storage().persistent().remove(&key);
        }
    }
}

pub fn get_source_cumulative_total(env: &Env, source: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::SourceCumulativeTotal(source.clone()))
        .unwrap_or(0)
}

pub fn set_source_cumulative_total(env: &Env, source: &Address, total: i128) {
    let key = StorageKey::SourceCumulativeTotal(source.clone());
    env.storage().persistent().set(&key, &total);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

/// Current treasury balance revision (0 before the first mutation).
pub fn get_treasury_revision(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&StorageKey::TreasuryRevision)
        .unwrap_or(0)
}

/// Increment the treasury balance revision and return the new value.
pub fn bump_treasury_revision(env: &Env) -> u64 {
    let next = get_treasury_revision(env).saturating_add(1);
    env.storage()
        .instance()
        .set(&StorageKey::TreasuryRevision, &next);
    next
}

/// Total number of allocation receipts ever written.
pub fn get_receipt_count(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&StorageKey::ReceiptCount)
        .unwrap_or(0)
}

/// Load one allocation receipt by id.
pub fn get_receipt(env: &Env, receipt_id: u64) -> Option<AllocationReceipt> {
    let key = StorageKey::Receipt(receipt_id);
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    data
}

/// Append an allocation receipt, returning its assigned id.
pub fn push_receipt(env: &Env, receipt: &AllocationReceipt) -> u64 {
    let id = get_receipt_count(env);
    let key = StorageKey::Receipt(id);
    env.storage().persistent().set(&key, receipt);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    env.storage()
        .instance()
        .set(&StorageKey::ReceiptCount, &(id + 1));
    id
}

/// Configured per-ledger-day allocation cap, if any.
pub fn get_daily_allocation_limit(env: &Env) -> Option<i128> {
    env.storage()
        .instance()
        .get(&StorageKey::DailyAllocationLimit)
}

pub fn set_daily_allocation_limit(env: &Env, limit: Option<i128>) {
    let key = StorageKey::DailyAllocationLimit;
    match limit {
        Some(l) => env.storage().instance().set(&key, &l),
        None => env.storage().instance().remove(&key),
    }
}

/// Cumulative allocation state for one ledger day.
pub fn get_daily_allocation(env: &Env, ledger_day: u64) -> DailyAllocation {
    let key = StorageKey::DailyAllocation(ledger_day);
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    data.unwrap_or(DailyAllocation {
        total: 0,
        limit_notified: false,
    })
}

pub fn set_daily_allocation(env: &Env, ledger_day: u64, state: &DailyAllocation) {
    let key = StorageKey::DailyAllocation(ledger_day);
    env.storage().persistent().set(&key, state);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn get_source_daily_total(env: &Env, source: &Address, ledger_day: u64) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::SourceDailyTotal(source.clone(), ledger_day))
        .unwrap_or(0)
}

pub fn set_source_daily_total(env: &Env, source: &Address, ledger_day: u64, total: i128) {
    let key = StorageKey::SourceDailyTotal(source.clone(), ledger_day);
    env.storage().persistent().set(&key, &total);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}
