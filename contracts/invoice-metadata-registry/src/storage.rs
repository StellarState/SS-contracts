//! Storage helpers for the invoice metadata registry contract.

use soroban_sdk::{Address, BytesN, Env};

use crate::types::{InvoiceMetadata, MetadataRevision, StorageKey};

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

pub fn get_invoice(env: &Env, invoice_id: &BytesN<32>) -> Option<InvoiceMetadata> {
    let key = StorageKey::Invoice(invoice_id.clone());
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    data
}

pub fn set_invoice(env: &Env, invoice_id: &BytesN<32>, data: &InvoiceMetadata) {
    let key = StorageKey::Invoice(invoice_id.clone());
    env.storage().persistent().set(&key, data);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}

pub fn get_revisions(
    env: &Env,
    invoice_id: &BytesN<32>,
) -> Option<soroban_sdk::Vec<MetadataRevision>> {
    let key = StorageKey::Revisions(invoice_id.clone());
    let revisions = env.storage().persistent().get(&key);
    if revisions.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
    }
    revisions
}

pub fn set_revisions(
    env: &Env,
    invoice_id: &BytesN<32>,
    revisions: &soroban_sdk::Vec<MetadataRevision>,
) {
    let key = StorageKey::Revisions(invoice_id.clone());
    env.storage().persistent().set(&key, revisions);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, MIN_TTL_EXTEND);
}
