//! Storage module for the invoice-token contract.
//!
//! This module manages the lifecycle and access patterns for token state, including balances, 
//! allowances, metadata, fees, nonces, and access control roles.
//! 
//! # Storage Architecture
//! The storage architecture leverages two types of Soroban storage:
//! - **Instance Storage:** Used for global contract state (e.g., `Metadata`, `TotalSupply`, `FeeBps`, `RoleAdmin`, `RoleGrant`). 
//!   Instance storage shares the TTL of the contract instance. Whenever the contract is invoked, 
//!   instance storage is implicitly bumped based on the instance's bump policy.
//! - **Persistent Storage:** Used for user-specific state that outlives the current instance TTL and must be explicitly 
//!   bumped (e.g., `Balance`, `Allowance`, `Nonce`, `History`, `Frozen`). This ensures that user funds and approvals 
//!   are not lost even if the contract instance itself is archived.
//!
//! # TTL Bump Policies
//! When modifying persistent storage, developers should ensure TTLs are sufficiently bumped 
//! (typically done in the top-level token interface functions invoking this module, or automatically by the host).

use soroban_sdk::{Address, Symbol, Vec};

use crate::types::{AllowanceData, OwnershipHistoryRecord, StorageKey, TokenMetadata};

/// Load token metadata from instance storage.
///
/// # Parameters
/// - `env`: The environment context.
///
/// # Returns
/// Returns `Some(TokenMetadata)` if initialized, otherwise `None`.
pub fn get_metadata(env: &soroban_sdk::Env) -> Option<TokenMetadata> {
    env.storage().instance().get(&StorageKey::Metadata)
}

/// Save token metadata to instance storage.
///
/// # Parameters
/// - `env`: The environment context.
/// - `meta`: The metadata structure to store.
pub fn set_metadata(env: &soroban_sdk::Env, meta: &TokenMetadata) {
    env.storage().instance().set(&StorageKey::Metadata, meta);
}

/// Load total supply from instance storage.
///
/// # Parameters
/// - `env`: The environment context.
///
/// # Returns
/// Returns the total token supply as `i128`. Defaults to `0` if not set.
pub fn get_total_supply(env: &soroban_sdk::Env) -> i128 {
    env.storage()
        .instance()
        .get(&StorageKey::TotalSupply)
        .unwrap_or(0)
}

/// Save total supply to instance storage.
///
/// # Parameters
/// - `env`: The environment context.
/// - `amount`: The new total supply amount.
pub fn set_total_supply(env: &soroban_sdk::Env, amount: i128) {
    env.storage()
        .instance()
        .set(&StorageKey::TotalSupply, &amount);
}

/// Get balance for an address (persistent storage).
///
/// # Parameters
/// - `env`: The environment context.
/// - `addr`: The address to query the balance for.
///
/// # Returns
/// Returns the balance as `i128`. Defaults to `0` if the account does not exist or has no balance.
pub fn get_balance(env: &soroban_sdk::Env, addr: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::Balance(addr.clone()))
        .unwrap_or(0)
}

/// Set balance for an address (persistent storage).
/// If the amount is 0, the entry is removed from storage to save space.
///
/// # Parameters
/// - `env`: The environment context.
/// - `addr`: The address whose balance will be set.
/// - `amount`: The new balance.
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

/// Check whether an account is frozen.
///
/// # Parameters
/// - `env`: The environment context.
/// - `account`: The address to check.
///
/// # Returns
/// Returns `true` if the account is frozen, otherwise `false`.
pub fn is_account_frozen(env: &soroban_sdk::Env, account: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&StorageKey::Frozen(account.clone()))
        .unwrap_or(false)
}

/// Update an account's frozen state, removing unrestricted entries from storage.
///
/// # Parameters
/// - `env`: The environment context.
/// - `account`: The address to update.
/// - `frozen`: The new frozen state.
pub fn set_account_frozen(env: &soroban_sdk::Env, account: &Address, frozen: bool) {
    let key = StorageKey::Frozen(account.clone());
    if frozen {
        env.storage().persistent().set(&key, &true);
    } else {
        env.storage().persistent().remove(&key);
    }
}

/// Get allowance (from, spender). Returns 0 if expired or not set.
///
/// # Parameters
/// - `env`: The environment context.
/// - `from`: The owner of the funds.
/// - `spender`: The authorized spender.
/// - `current_ledger`: The current ledger sequence number.
///
/// # Returns
/// Returns the available allowance as `i128`. Returns `0` if expired or not found.
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
///
/// # Parameters
/// - `env`: The environment context.
/// - `from`: The owner of the funds.
/// - `spender`: The authorized spender.
/// - `amount`: The amount to authorize.
/// - `expiration_ledger`: The ledger sequence when this allowance expires.
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
///
/// # Parameters
/// - `env`: The environment context.
/// - `from`: The owner of the funds.
/// - `spender`: The authorized spender.
///
/// # Returns
/// Returns `Some(AllowanceData)` if found, otherwise `None`.
pub fn get_allowance_data(
    env: &soroban_sdk::Env,
    from: &Address,
    spender: &Address,
) -> Option<AllowanceData> {
    env.storage()
        .persistent()
        .get(&StorageKey::Allowance(from.clone(), spender.clone()))
}

// ==================== Fee (Issue #113) ====================

/// Get fee basis points from instance storage. Returns 0 if not set.
///
/// # Parameters
/// - `env`: The environment context.
///
/// # Returns
/// Returns the fee basis points as `i128`.
pub fn get_fee_bps(env: &soroban_sdk::Env) -> i128 {
    env.storage()
        .instance()
        .get(&StorageKey::FeeBps)
        .unwrap_or(0)
}

/// Save fee basis points to instance storage.
///
/// # Parameters
/// - `env`: The environment context.
/// - `bps`: The fee basis points to set.
pub fn set_fee_bps(env: &soroban_sdk::Env, bps: i128) {
    env.storage().instance().set(&StorageKey::FeeBps, &bps);
}

// ==================== Role Admin (Issue #108) ====================

/// Get the admin address for a specific role. Returns None if unset.
///
/// # Parameters
/// - `env`: The environment context.
/// - `role`: The role symbol.
///
/// # Returns
/// Returns the admin address `Some(Address)` if set, otherwise `None`.
pub fn get_role_admin(env: &soroban_sdk::Env, role: &Symbol) -> Option<Address> {
    env.storage()
        .instance()
        .get(&StorageKey::RoleAdmin(role.clone()))
}

/// Set the admin address for a specific role.
///
/// # Parameters
/// - `env`: The environment context.
/// - `role`: The role symbol.
/// - `admin`: The admin address.
pub fn set_role_admin(env: &soroban_sdk::Env, role: &Symbol, admin: &Address) {
    env.storage()
        .instance()
        .set(&StorageKey::RoleAdmin(role.clone()), admin);
}

/// Check whether `account` has been granted `role`.
///
/// # Parameters
/// - `env`: The environment context.
/// - `role`: The role symbol to check.
/// - `account`: The account address to verify.
///
/// # Returns
/// Returns `true` if the account has the role, otherwise `false`.
pub fn has_role(env: &soroban_sdk::Env, role: &Symbol, account: &Address) -> bool {
    env.storage()
        .instance()
        .get(&StorageKey::RoleGrant(role.clone(), account.clone()))
        .unwrap_or(false)
}

/// Grant or revoke `role` for `account`.
///
/// # Parameters
/// - `env`: The environment context.
/// - `role`: The role symbol.
/// - `account`: The account address.
/// - `granted`: Boolean indicating whether to grant (`true`) or revoke (`false`).
pub fn set_role_grant(env: &soroban_sdk::Env, role: &Symbol, account: &Address, granted: bool) {
    let key = StorageKey::RoleGrant(role.clone(), account.clone());
    if granted {
        env.storage().instance().set(&key, &true);
    } else {
        env.storage().instance().remove(&key);
    }
}

// ==================== Nonce (Issue #106) ====================

/// Get the current nonce for an address. Starts at 0.
///
/// # Parameters
/// - `env`: The environment context.
/// - `addr`: The account address.
///
/// # Returns
/// Returns the current nonce as `u64`.
pub fn get_nonce(env: &soroban_sdk::Env, addr: &Address) -> u64 {
    env.storage()
        .persistent()
        .get(&StorageKey::Nonce(addr.clone()))
        .unwrap_or(0u64)
}

/// Increment and return the new nonce for an address.
///
/// # Parameters
/// - `env`: The environment context.
/// - `addr`: The account address.
///
/// # Returns
/// Returns the newly incremented nonce as `u64`.
#[allow(dead_code)]
pub fn increment_nonce(env: &soroban_sdk::Env, addr: &Address) -> u64 {
    let current = get_nonce(env, addr);
    let new_nonce = current + 1;
    env.storage()
        .persistent()
        .set(&StorageKey::Nonce(addr.clone()), &new_nonce);
    new_nonce
}

// ==================== Ownership History (Issue #111) ====================

/// Get the full ownership history for an address. Returns empty Vec if none.
///
/// # Parameters
/// - `env`: The environment context.
/// - `addr`: The account address.
///
/// # Returns
/// Returns a `Vec<OwnershipHistoryRecord>`.
pub fn get_token_history(env: &soroban_sdk::Env, addr: &Address) -> Vec<OwnershipHistoryRecord> {
    env.storage()
        .persistent()
        .get(&StorageKey::History(addr.clone()))
        .unwrap_or_else(|| soroban_sdk::Vec::new(env))
}

/// Append an ownership history record for an address.
///
/// # Parameters
/// - `env`: The environment context.
/// - `addr`: The account address.
/// - `record`: The history record to append.
pub fn append_token_history(
    env: &soroban_sdk::Env,
    addr: &Address,
    record: &OwnershipHistoryRecord,
) {
    let key = StorageKey::History(addr.clone());
    let mut history: Vec<OwnershipHistoryRecord> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| soroban_sdk::Vec::new(env));
    history.push_back(record.clone());
    env.storage().persistent().set(&key, &history);
}

// ==================== Allowance Expiration Extension ====================

/// Extend the expiration ledger of an existing allowance in place, leaving its amount untouched.
///
/// # Parameters
/// - `env`: The environment context.
/// - `from`: The owner of the funds.
/// - `spender`: The authorized spender.
/// - `new_expiration_ledger`: The new expiration ledger sequence.
pub fn extend_allowance_expiration(
    env: &soroban_sdk::Env,
    from: &Address,
    spender: &Address,
    new_expiration_ledger: u32,
) {
    let key = StorageKey::Allowance(from.clone(), spender.clone());
    if let Some(mut data) = env.storage().persistent().get::<_, AllowanceData>(&key) {
        data.expiration_ledger = new_expiration_ledger;
        env.storage().persistent().set(&key, &data);
    }
}
