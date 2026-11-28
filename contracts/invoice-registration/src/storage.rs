//! Storage helpers for the invoice registration contract.

use soroban_sdk::{Address, BytesN, Env, String};

use crate::types::{InvoiceBinding, RegistrationResult, StorageKey};

/// Ledgers below which a persistent entry's TTL is extended (~7 days at 5s/ledger).
pub const TTL_THRESHOLD: u32 = 120_960;
/// Minimum TTL extension in ledger units (~60 days at 5s/ledger).
pub const MIN_TTL_EXTEND: u32 = 1_036_800;

fn bump_persistent(env: &Env, key: &StorageKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

/// Load the registration stored under an `(issuer, external_invoice_id)` key.
pub fn get_registration(
    env: &Env,
    issuer: &Address,
    external_invoice_id: &String,
) -> Option<RegistrationResult> {
    let key = StorageKey::Registration(issuer.clone(), external_invoice_id.clone());
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        bump_persistent(env, &key);
    }
    data
}

/// Save the registration under an `(issuer, external_invoice_id)` key.
pub fn set_registration(env: &Env, data: &RegistrationResult) {
    let key = StorageKey::Registration(data.issuer.clone(), data.external_invoice_id.clone());
    env.storage().persistent().set(&key, data);
    bump_persistent(env, &key);
}

/// Load the key bound to an internal invoice id.
pub fn get_invoice_binding(env: &Env, invoice_id: &BytesN<32>) -> Option<InvoiceBinding> {
    let key = StorageKey::InvoiceBinding(invoice_id.clone());
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        bump_persistent(env, &key);
    }
    data
}

/// Bind an internal invoice id to the key that registered it.
pub fn set_invoice_binding(env: &Env, invoice_id: &BytesN<32>, binding: &InvoiceBinding) {
    let key = StorageKey::InvoiceBinding(invoice_id.clone());
    env.storage().persistent().set(&key, binding);
    bump_persistent(env, &key);
}
