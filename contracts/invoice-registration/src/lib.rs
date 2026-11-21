//! Invoice registration contract for StellarSettle.
//!
//! Maps a stable, caller-supplied business identifier (`external_invoice_id`)
//! to an internal on-chain invoice id, once per issuer. Registration is
//! idempotent in the sense that repeating it can never create a second
//! invoice: the first successful call wins, and every retry with the same
//! `(issuer, external_invoice_id)` key fails with `AlreadyRegistered`.
//!
//! Keys are namespaced by issuer, so two issuers may each use the same
//! business identifier without colliding. An internal invoice id is bound at
//! most once, so a single on-chain invoice cannot be registered under two
//! different keys or issuers.
//!
//! All validation and duplicate checks run before any storage write, so a
//! rejected registration leaves no invoice state behind. The contract never
//! mints tokens, creates escrows or mutates balances.

#![no_std]

mod errors;
mod events;
mod storage;
mod types;

pub use types::{InvoiceBinding, RegistrationResult, StorageKey};

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String};

use errors::Error;

/// Maximum length, in bytes, of an external invoice id.
const MAX_EXTERNAL_ID_LEN: u32 = 128;

/// Reject the zero address (all-zero 32-byte Ed25519 key).
fn ensure_non_zero_address(env: &Env, address: &Address) -> Result<(), Error> {
    let zero_str = String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    );
    let zero = Address::from_string(&zero_str);
    if *address == zero {
        return Err(Error::InvalidAddress);
    }
    Ok(())
}

#[contract]
pub struct InvoiceRegistration;

#[contractimpl]
impl InvoiceRegistration {
    /// Issuer-authorized: register `external_invoice_id` as resolving to
    /// `invoice_id`.
    ///
    /// `issuer` must authorize the call, so a key cannot be claimed on behalf
    /// of another issuer. The registration key is the pair
    /// `(issuer, external_invoice_id)`; it can be registered once, after which
    /// every retry returns `AlreadyRegistered` before anything is written.
    ///
    /// Fails with:
    /// - `InvalidExternalId` for an empty or over-long identifier;
    /// - `InvalidAddress` when `issuer` is the zero address;
    /// - `AlreadyRegistered` when the key already resolves to an invoice;
    /// - `InvoiceAlreadyRegistered` when `invoice_id` is already bound.
    ///
    /// On success emits a single `invoice_registered` event.
    pub fn register_invoice(
        env: Env,
        issuer: Address,
        external_invoice_id: String,
        invoice_id: BytesN<32>,
    ) -> Result<RegistrationResult, Error> {
        issuer.require_auth();
        ensure_non_zero_address(&env, &issuer)?;

        if external_invoice_id.is_empty() || external_invoice_id.len() > MAX_EXTERNAL_ID_LEN {
            return Err(Error::InvalidExternalId);
        }

        // Duplicate checks precede every write so a rejected retry cannot
        // leave a partial registration (or a second invoice) behind.
        if storage::get_registration(&env, &issuer, &external_invoice_id).is_some() {
            return Err(Error::AlreadyRegistered);
        }
        if storage::get_invoice_binding(&env, &invoice_id).is_some() {
            return Err(Error::InvoiceAlreadyRegistered);
        }

        let result = RegistrationResult {
            invoice_id: invoice_id.clone(),
            external_invoice_id: external_invoice_id.clone(),
            issuer: issuer.clone(),
            registered_at: env.ledger().timestamp(),
        };
        storage::set_registration(&env, &result);
        storage::set_invoice_binding(
            &env,
            &invoice_id,
            &InvoiceBinding {
                issuer: issuer.clone(),
                external_invoice_id: external_invoice_id.clone(),
            },
        );
        events::invoice_registered(&env, &issuer, &external_invoice_id, &invoice_id);
        Ok(result)
    }

    /// View: resolve `(issuer, external_invoice_id)` to its registration.
    ///
    /// Returns `NotRegistered` when this issuer has no registration for the
    /// identifier; registrations owned by a different issuer are invisible
    /// here rather than being disclosed or reused.
    pub fn resolve_registration(
        env: Env,
        issuer: Address,
        external_invoice_id: String,
    ) -> Result<RegistrationResult, Error> {
        storage::get_registration(&env, &issuer, &external_invoice_id).ok_or(Error::NotRegistered)
    }

    /// View: whether `(issuer, external_invoice_id)` is registered and
    /// resolves to `invoice_id`.
    pub fn verify_registration(
        env: Env,
        issuer: Address,
        external_invoice_id: String,
        invoice_id: BytesN<32>,
    ) -> bool {
        match storage::get_registration(&env, &issuer, &external_invoice_id) {
            Some(record) => record.invoice_id == invoice_id,
            None => false,
        }
    }

    /// View: resolve an internal invoice id back to its registration.
    pub fn get_registration_by_invoice(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<RegistrationResult, Error> {
        let binding =
            storage::get_invoice_binding(&env, &invoice_id).ok_or(Error::NotRegistered)?;
        storage::get_registration(&env, &binding.issuer, &binding.external_invoice_id)
            .ok_or(Error::NotRegistered)
    }
}

#[cfg(test)]
mod test;
