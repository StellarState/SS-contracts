//! Storage helpers: instance for config, persistent for escrow data.

use soroban_sdk::{Address, Env, Symbol};

use crate::errors::Error;
use crate::types::{Config, EscrowData, StorageKey};

/// Ledgers below which a persistent entry's TTL is extended (~7 days at 5s/ledger).
const TTL_THRESHOLD: u32 = 120_960;
/// Ledgers to extend a persistent entry's TTL to when bumped (~30 days at 5s/ledger).
const TTL_EXTEND_TO: u32 = 518_400;
/// TTL threshold for instance storage (~7 days at 5s/ledger).
const INSTANCE_TTL_THRESHOLD: u32 = 120_960;
/// TTL extension for instance storage (~30 days at 5s/ledger).
const INSTANCE_TTL_EXTEND_TO: u32 = 518_400;

/// Extend the TTL of an escrow's persistent storage entry so it survives
/// ledger pruning across the full lifetime of a (potentially long-lived,
/// e.g. multi-month) invoice, not just the archival minimum.
pub fn extend_ttl(env: &Env, inv_id: Symbol) {
    env.storage().persistent().extend_ttl(
        &StorageKey::Escrow(inv_id),
        TTL_THRESHOLD,
        TTL_EXTEND_TO,
    );
}

fn extend_ttl_nonce(env: &Env, buyer: &Address) {
    env.storage().persistent().extend_ttl(
        &StorageKey::Nonce(buyer.clone()),
        TTL_THRESHOLD,
        TTL_EXTEND_TO,
    );
}

fn extend_ttl_funder(env: &Env, inv_id: Symbol, funder: &Address) {
    env.storage().persistent().extend_ttl(
        &StorageKey::FunderAmount(inv_id, funder.clone()),
        TTL_THRESHOLD,
        TTL_EXTEND_TO,
    );
}

fn extend_ttl_whitelist(env: &Env, buyer: &Address) {
    env.storage().persistent().extend_ttl(
        &StorageKey::BuyerWhitelist(buyer.clone()),
        TTL_THRESHOLD,
        TTL_EXTEND_TO,
    );
}

fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND_TO);
}

/// Load contract config from instance storage.
/// Bumps instance TTL on every access to prevent expiration of global config.
pub fn get_config(env: &Env) -> Option<Config> {
    let config = env.storage().instance().get(&StorageKey::Config);
    if config.is_some() {
        extend_instance_ttl(env);
    }
    config
}

/// Save contract config to instance storage.
/// Bumps instance TTL to ensure global config survives long deployments.
pub fn set_config(env: &Env, config: &Config) {
    env.storage().instance().set(&StorageKey::Config, config);
    extend_instance_ttl(env);
}

/// Load escrow data for an invoice from persistent storage.
/// Extends the entry's TTL on every access so actively-used escrows never
/// expire mid-lifecycle regardless of how long between state transitions.
pub fn get_escrow(env: &Env, inv_id: Symbol) -> Option<EscrowData> {
    let data = env
        .storage()
        .persistent()
        .get(&StorageKey::Escrow(inv_id.clone()));
    if data.is_some() {
        extend_ttl(env, inv_id);
    }
    data
}

/// Save escrow data for an invoice to persistent storage, extending its TTL.
pub fn set_escrow(env: &Env, inv_id: Symbol, data: &EscrowData) {
    env.storage()
        .persistent()
        .set(&StorageKey::Escrow(inv_id.clone()), data);
    extend_ttl(env, inv_id);
}

/// Check if an escrow exists for the given invoice.
/// Bumps TTL if found so existence checks don't silently cause expiration.
pub fn has_escrow(env: &Env, inv_id: Symbol) -> bool {
    let exists = env.storage().persistent().has(&StorageKey::Escrow(inv_id.clone()));
    if exists {
        extend_ttl(env, inv_id);
    }
    exists
}

/// Remove escrow data for an invoice from persistent storage (storage footprint cleanup).
pub fn remove_escrow(env: &Env, inv_id: Symbol) {
    env.storage()
        .persistent()
        .remove(&StorageKey::Escrow(inv_id));
}

/// Get the highest nonce consumed so far for a buyer's signed off-chain approvals.
/// Bumps TTL to ensure nonce tracking persists across long funding windows.
pub fn get_nonce(env: &Env, buyer: &Address) -> u64 {
    let key = StorageKey::Nonce(buyer.clone());
    let nonce = env.storage().persistent().get(&key).unwrap_or(0);
    if nonce > 0 {
        extend_ttl_nonce(env, buyer);
    }
    nonce
}

/// Record the highest nonce consumed for a buyer's signed off-chain approvals.
/// Enforces monotonic nonce increase to prevent nonce rollback attacks.
/// Bumps TTL on successful write.
pub fn set_nonce(env: &Env, buyer: &Address, nonce: u64) -> Result<(), Error> {
    let current = get_nonce(env, buyer);
    if nonce <= current {
        return Err(Error::NonceAlreadyUsed);
    }
    env.storage()
        .persistent()
        .set(&StorageKey::Nonce(buyer.clone()), &nonce);
    extend_ttl_nonce(env, buyer);
    Ok(())
}

/// Get the amount funded by a specific funder for an invoice.
/// Bumps TTL so funder accounting doesn't expire mid-escrow lifecycle.
pub fn get_funder_amount(env: &Env, inv_id: Symbol, funder: &Address) -> i128 {
    let key = StorageKey::FunderAmount(inv_id.clone(), funder.clone());
    let amount = env.storage().persistent().get(&key).unwrap_or(0);
    if amount != 0 {
        extend_ttl_funder(env, inv_id, funder);
    }
    amount
}

/// Set the amount funded by a specific funder for an invoice.
/// Rejects negative amounts to prevent accounting manipulation.
/// Bumps TTL on successful write (if non-zero) or cleans up zero entries.
pub fn set_funder_amount(
    env: &Env,
    inv_id: Symbol,
    funder: &Address,
    amount: i128,
) -> Result<(), Error> {
    if amount < 0 {
        return Err(Error::InvalidAmount);
    }
    if amount == 0 {
        env.storage()
            .persistent()
            .remove(&StorageKey::FunderAmount(inv_id, funder.clone()));
    } else {
        env.storage().persistent().set(
            &StorageKey::FunderAmount(inv_id.clone(), funder.clone()),
            &amount,
        );
        extend_ttl_funder(env, inv_id, funder);
    }
    Ok(())
}

/// Whether `buyer` is whitelisted to fund (buy) escrows. Absent entry = not whitelisted.
/// Bumps TTL when the buyer is whitelisted so the entry persists.
pub fn is_whitelisted(env: &Env, buyer: &Address) -> bool {
    let key = StorageKey::BuyerWhitelist(buyer.clone());
    let whitelisted = env.storage().persistent().get(&key).unwrap_or(false);
    if whitelisted {
        extend_ttl_whitelist(env, buyer);
    }
    whitelisted
}

/// Set (or clear) a buyer's whitelist status.
/// Bumps TTL when enabling whitelist; removes storage entry when disabling.
pub fn set_whitelisted(env: &Env, buyer: &Address, allowed: bool) {
    if allowed {
        env.storage()
            .persistent()
            .set(&StorageKey::BuyerWhitelist(buyer.clone()), &true);
        extend_ttl_whitelist(env, buyer);
    } else {
        env.storage()
            .persistent()
            .remove(&StorageKey::BuyerWhitelist(buyer.clone()));
    }
}
