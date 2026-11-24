//! Storage helpers for the KYC attestation contract.

use soroban_sdk::{Address, Env};

use crate::types::{Attestation, StorageKey};

/// Bump when the TTL of a persistent entry falls below this many ledgers
/// (~7 days at 5 s/ledger).
pub const TTL_THRESHOLD: u32 = 120_960;
/// Target TTL extension in ledger units (~60 days at 5 s/ledger).
pub const MIN_TTL_EXTEND: u32 = 1_036_800;

// ── Oracle (instance storage) ────────────────────────────────────────────────

pub fn set_oracle(env: &Env, oracle: &Address) {
    env.storage().instance().set(&StorageKey::Oracle, oracle);
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn get_oracle(env: &Env) -> Option<Address> {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, MIN_TTL_EXTEND);
    env.storage().instance().get(&StorageKey::Oracle)
}

// ── Attestation (persistent storage) ────────────────────────────────────────

/// Persist an attestation record for `wallet`.
pub fn set_attestation(env: &Env, wallet: &Address, attestation: &Attestation) {
    let key = StorageKey::Attestation(wallet.clone());
    env.storage().persistent().set(&key, attestation);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

/// Load the raw attestation for `wallet`, or `None` if none has ever been set.
pub fn get_attestation(env: &Env, wallet: &Address) -> Option<Attestation> {
    let key = StorageKey::Attestation(wallet.clone());
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    data
}
