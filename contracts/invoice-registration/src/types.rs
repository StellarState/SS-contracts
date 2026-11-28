//! Data types for the invoice registration contract.

use soroban_sdk::{contracttype, Address, BytesN, String};

/// Storage key enum for persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Persistent: registration for an `(issuer, external_invoice_id)` key.
    ///
    /// The issuer is part of the key, so two issuers may each register the
    /// same business identifier (e.g. `"INV-2026-001"`) without colliding.
    Registration(Address, String),
    /// Persistent: key bound to an internal invoice id.
    InvoiceBinding(BytesN<32>),
}

/// Explicit outcome of a successful registration.
///
/// Returned by `register_invoice` and by the resolve views, so callers can
/// confirm both identifiers and the issuer that owns the registration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistrationResult {
    /// Internal on-chain invoice id the registration resolves to.
    pub invoice_id: BytesN<32>,
    /// Stable, caller-supplied business identifier.
    pub external_invoice_id: String,
    /// Issuer that owns the registration key.
    pub issuer: Address,
    /// Ledger timestamp of the successful registration.
    pub registered_at: u64,
}

/// Reverse index entry: which key is bound to an internal invoice id.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceBinding {
    /// Issuer that owns the registration key.
    pub issuer: Address,
    /// Stable, caller-supplied business identifier.
    pub external_invoice_id: String,
}
