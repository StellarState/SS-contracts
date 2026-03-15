//! Storage helpers for balances, metadata, and allowances.

use soroban_sdk::Address;

use crate::types::{AllowanceData, StorageKey, TokenMetadata};

/// Load token metadata from instance storage.
pub fn get_metadata(env: &soroban_sdk::Env) -> Option<TokenMetadata> {
    env.storage().instance().get(&StorageKey::Metadata)
}

/// Save token metadata to instance storage.
pub fn set_metadata(env: &soroban_sdk::Env, meta: &TokenMetadata) {
    env.storage().instance().set(&StorageKey::Metadata, meta);
}

/// Load total supply from instance storage.
pub fn get_total_supply(env: &soroban_sdk::Env) -> i128 {
    env.storage()
        .instance()
        .get(&StorageKey::TotalSupply)
        .unwrap_or(0)
}

/// Save total supply to instance storage.
pub fn set_total_supply(env: &soroban_sdk::Env, amount: i128) {
    env.storage()
        .instance()
        .set(&StorageKey::TotalSupply, &amount);
}

/// Get balance for an address (persistent storage).
pub fn get_balance(env: &soroban_sdk::Env, addr: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::Balance(addr.clone()))
        .unwrap_or(0)
}

/// Set balance for an address (persistent storage).
pub fn set_balance(env: &soroban_sdk::Env, addr: &Address, amount: i128) {
    if amount == 0 {
        env.storage()
            .persistent()
            .remove(&StorageKey::Balance(addr.clone()));
    } else {
        env.storage()
            .persistent()
            .set(&StorageKey::Balance(addr.clone()), &amount);
    }
}

/// Get allowance (from, spender). Returns 0 if expired or not set.
pub fn get_allowance(
    env: &soroban_sdk::Env,
    from: &Address,
    spender: &Address,
    current_ledger: u32,
) -> i128 {
    let key = StorageKey::Allowance(from.clone(), spender.clone());
    let data: Option<AllowanceData> = env.storage().persistent().get(&key);
    match data {
        Some(a) if a.expiration_ledger >= current_ledger => a.amount,
        _ => 0,
    }
}

/// Set allowance (from, spender) -> (amount, expiration_ledger).
/// Removes the key when amount is 0 to save persistent storage.
pub fn set_allowance(
    env: &soroban_sdk::Env,
    from: &Address,
    spender: &Address,
    amount: i128,
    expiration_ledger: u32,
) {
    let key = StorageKey::Allowance(from.clone(), spender.clone());
    if amount == 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(
            &key,
            &AllowanceData {
                amount,
                expiration_ledger,
            },
        );
    }
}

/// Get raw allowance data (for decreasing allowance on transfer_from/burn_from).
pub fn get_allowance_data(
    env: &soroban_sdk::Env,
    from: &Address,
    spender: &Address,
) -> Option<AllowanceData> {
    env.storage()
        .persistent()
        .get(&StorageKey::Allowance(from.clone(), spender.clone()))
}
