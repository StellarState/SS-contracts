//! Storage helpers for the secondary market trade history contract.

use soroban_sdk::{Address, BytesN, Env, Vec};

use crate::types::{StorageKey, TradeRecord};

/// Maximum trades retained per invoice; the oldest is pruned on overflow.
pub const MAX_TRADES: u32 = 100;

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

pub fn set_recorder(env: &Env, recorder: &Address) {
    env.storage()
        .instance()
        .set(&StorageKey::Recorder, recorder);
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn get_recorder(env: &Env) -> Option<Address> {
    env.storage().instance().get(&StorageKey::Recorder)
}

/// Load an invoice's trade history, newest trade first. Empty `Vec` if
/// no trade has ever been recorded.
pub fn get_history(env: &Env, invoice_id: &BytesN<32>) -> Vec<TradeRecord> {
    let key = StorageKey::History(invoice_id.clone());
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    data.unwrap_or(Vec::new(env))
}

/// Prepend `record` to `invoice_id`'s history (newest-first), pruning the
/// oldest entry (the last element) if the list would exceed `MAX_TRADES`.
pub fn push_trade(env: &Env, invoice_id: &BytesN<32>, record: TradeRecord) {
    let key = StorageKey::History(invoice_id.clone());
    let mut history = get_history(env, invoice_id);
    history.insert(0, record);
    while history.len() > MAX_TRADES {
        history.remove(history.len() - 1);
    }
    env.storage().persistent().set(&key, &history);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}
