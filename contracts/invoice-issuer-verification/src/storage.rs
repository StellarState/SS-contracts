//! Storage helpers for the invoice issuer verification badge contract.

use soroban_sdk::{Address, Env};

use crate::types::{StorageKey, VerificationRecord};

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

/// Load the verification record stored for `issuer`, or `None` if it was
/// never granted (or was revoked).
pub fn get_verification(env: &Env, issuer: &Address) -> Option<VerificationRecord> {
    let key = StorageKey::Verification(issuer.clone());
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    data
}

/// Save (or overwrite) the verification record for `issuer`.
pub fn set_verification(env: &Env, issuer: &Address, record: &VerificationRecord) {
    let key = StorageKey::Verification(issuer.clone());
    env.storage().persistent().set(&key, record);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

/// Remove the verification record for `issuer` entirely (used by revoke).
pub fn remove_verification(env: &Env, issuer: &Address) {
    let key = StorageKey::Verification(issuer.clone());
    env.storage().persistent().remove(&key);
}
