//! Deprecation metadata and policy for the Invoice Escrow contract.
//!
//! This module defines the deprecation lifecycle for public entrypoints.
//! Each deprecated method is annotated with its replacement, the version
//! it was deprecated in, and the target removal version.

use soroban_sdk::{contracttype, String, Symbol, Vec, Env};

/// Deprecation status for a public entrypoint.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum DeprecationStatus {
    /// Current, recommended API.
    Current = 0,
    /// Deprecated but still functional; will be removed in a future version.
    Deprecated = 1,
    /// Removed; calls will fail with `DeprecatedRemoved`.
    Removed = 2,
}

/// Metadata for a deprecated entrypoint.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeprecationRecord {
    /// Entrypoint name (e.g., "create_escrow_legacy").
    pub method: Symbol,
    /// Contract version when this method was deprecated.
    pub deprecated_since: String,
    /// Contract version targeted for removal (semver).
    pub removal_target: String,
    /// Replacement entrypoint name.
    pub replacement: Symbol,
    /// Human-readable migration guidance.
    pub guidance: String,
    /// Minimum warning period in days before removal.
    pub min_warning_days: u32,
}

/// Creates the deprecation record for `create_escrow_legacy`.
pub fn create_escrow_legacy_record(env: &Env) -> DeprecationRecord {
    DeprecationRecord {
        method: Symbol::new(env, "create_escrow_legacy"),
        deprecated_since: String::from_str(env, "1.2.0"),
        removal_target: String::from_str(env, "2.0.0"),
        replacement: Symbol::new(env, "create_escrow"),
        guidance: String::from_str(
            env,
            "Use create_escrow with accepted_tokens parameter for multi-token support",
        ),
        min_warning_days: 90,
    }
}

/// Creates the deprecation record for `fund_escrow_legacy`.
pub fn fund_escrow_legacy_record(env: &Env) -> DeprecationRecord {
    DeprecationRecord {
        method: Symbol::new(env, "fund_escrow_legacy"),
        deprecated_since: String::from_str(env, "1.2.0"),
        removal_target: String::from_str(env, "2.0.0"),
        replacement: Symbol::new(env, "fund_escrow"),
        guidance: String::from_str(
            env,
            "Use fund_escrow with funding_token parameter to specify the payment token explicitly",
        ),
        min_warning_days: 90,
    }
}

/// Creates the deprecation record for `create_escrow_with_tokens`.
pub fn create_escrow_with_tokens_record(env: &Env) -> DeprecationRecord {
    DeprecationRecord {
        method: Symbol::new(env, "create_escrow_with_tokens"),
        deprecated_since: String::from_str(env, "1.2.0"),
        removal_target: String::from_str(env, "2.0.0"),
        replacement: Symbol::new(env, "create_escrow"),
        guidance: String::from_str(
            env,
            "Alias for create_escrow; use create_escrow directly",
        ),
        min_warning_days: 90,
    }
}

/// Creates the deprecation record for `fund_escrow_with_token`.
pub fn fund_escrow_with_token_record(env: &Env) -> DeprecationRecord {
    DeprecationRecord {
        method: Symbol::new(env, "fund_escrow_with_token"),
        deprecated_since: String::from_str(env, "1.2.0"),
        removal_target: String::from_str(env, "2.0.0"),
        replacement: Symbol::new(env, "fund_escrow"),
        guidance: String::from_str(
            env,
            "Alias for fund_escrow; use fund_escrow directly",
        ),
        min_warning_days: 90,
    }
}

/// Returns all deprecation records for this contract.
pub fn all_deprecation_records(env: &Env) -> Vec<DeprecationRecord> {
    let mut vec = Vec::new(env);
    vec.push_back(create_escrow_legacy_record(env));
    vec.push_back(fund_escrow_legacy_record(env));
    vec.push_back(create_escrow_with_tokens_record(env));
    vec.push_back(fund_escrow_with_token_record(env));
    vec
}

/// Checks if an entrypoint is deprecated and returns its record if so.
pub fn get_deprecation_record(env: &Env, method: &Symbol) -> Option<DeprecationRecord> {
    for record in all_deprecation_records(env).iter() {
        if &record.method == method {
            return Some(record);
        }
    }
    None
}

/// Returns true if the given method is deprecated.
pub fn is_deprecated(env: &Env, method: &Symbol) -> bool {
    get_deprecation_record(env, method).is_some()
}