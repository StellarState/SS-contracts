//! KYC attestation contract for StellarSettle.
//!
//! Stores verified credential *hashes* for wallets, allowing the invoice and
//! investment contracts to check KYC status on-chain without ever exposing
//! personal data. The raw identity document never touches the ledger; only the
//! 32-byte SHA-256 hash of the off-chain credential is recorded.
//!
//! # Roles
//! * **Oracle** – the platform's KYC service account; the only address allowed
//!   to call [`attest`] and [`revoke`].
//!
//! # Public interface
//! | Function | Description |
//! |---|---|
//! | `initialize(oracle)` | One-time setup; records the oracle address. |
//! | `attest(wallet, credential_hash, expiry_ledger)` | Oracle attests KYC for a wallet. |
//! | `revoke(wallet)` | Oracle immediately invalidates an attestation. |
//! | `is_verified(wallet)` | Returns `true` iff the wallet has a live, unexpired, non-revoked attestation. |
//! | `get_attestation(wallet)` | Returns `(credential_hash, attested_ledger, expiry_ledger)`. |
//! | `get_oracle()` | Returns the configured oracle address. |

#![no_std]

mod errors;
mod events;
mod storage;
mod types;

pub use errors::Error;
pub use types::{Attestation, StorageKey};

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String};

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
pub struct KycAttestation;

#[contractimpl]
impl KycAttestation {
    // ── Initialisation ────────────────────────────────────────────────────────

    /// One-time setup. Records the KYC oracle address that is exclusively
    /// allowed to call `attest` and `revoke`.
    ///
    /// # Errors
    /// * [`Error::AlreadyInit`]  – called more than once.
    /// * [`Error::InvalidAddress`] – `oracle` is the zero address.
    pub fn initialize(env: Env, oracle: Address) -> Result<(), Error> {
        ensure_non_zero_address(&env, &oracle)?;
        if storage::get_oracle(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        oracle.require_auth();
        storage::set_oracle(&env, &oracle);
        Ok(())
    }

    // ── Oracle-only mutations ─────────────────────────────────────────────────

    /// Record (or renew) a KYC attestation for `wallet`.
    ///
    /// Calling `attest` on a wallet that already has a live attestation
    /// *replaces* it with the new data (re-attestation / credential rotation).
    ///
    /// # Arguments
    /// * `oracle` – must match the configured oracle address.
    /// * `wallet` – the investor's on-chain account.
    /// * `credential_hash` – 32-byte hash of the off-chain KYC credential.
    /// * `expiry_ledger` – ledger sequence at which the attestation expires;
    ///   must be strictly greater than the current ledger.
    ///
    /// # Errors
    /// * [`Error::NotInit`] – contract not yet initialised.
    /// * [`Error::Unauthorized`] – `oracle` does not match the stored oracle.
    /// * [`Error::InvalidAddress`] – `wallet` is the zero address.
    /// * [`Error::InvalidExpiry`] – `expiry_ledger` ≤ current ledger.
    ///
    /// # Events
    /// Emits [`WalletAttested`](events::wallet_attested).
    pub fn attest(
        env: Env,
        oracle: Address,
        wallet: Address,
        credential_hash: BytesN<32>,
        expiry_ledger: u32,
    ) -> Result<(), Error> {
        let stored_oracle = storage::get_oracle(&env).ok_or(Error::NotInit)?;
        if oracle != stored_oracle {
            return Err(Error::Unauthorized);
        }
        oracle.require_auth();
        ensure_non_zero_address(&env, &wallet)?;

        let current_ledger = env.ledger().sequence();
        if expiry_ledger <= current_ledger {
            return Err(Error::InvalidExpiry);
        }

        let attested_ledger = current_ledger;
        let record = Attestation {
            credential_hash: credential_hash.clone(),
            attested_ledger,
            expiry_ledger,
            revoked: false,
        };
        storage::set_attestation(&env, &wallet, &record);

        events::wallet_attested(
            &env,
            &wallet,
            &credential_hash,
            attested_ledger,
            expiry_ledger,
        );
        Ok(())
    }

    /// Oracle-only: immediately invalidate `wallet`'s attestation.
    ///
    /// After this call, [`is_verified`] returns `false` for `wallet`
    /// regardless of the recorded expiry.
    ///
    /// # Errors
    /// * [`Error::NotInit`] – contract not yet initialised.
    /// * [`Error::Unauthorized`] – `oracle` does not match the stored oracle.
    /// * [`Error::InvalidAddress`] – `wallet` is the zero address.
    /// * [`Error::NotAttested`] – no attestation record exists for `wallet`.
    ///
    /// # Events
    /// Emits [`AttestationRevoked`](events::attestation_revoked).
    pub fn revoke(env: Env, oracle: Address, wallet: Address) -> Result<(), Error> {
        let stored_oracle = storage::get_oracle(&env).ok_or(Error::NotInit)?;
        if oracle != stored_oracle {
            return Err(Error::Unauthorized);
        }
        oracle.require_auth();
        ensure_non_zero_address(&env, &wallet)?;

        let mut record = storage::get_attestation(&env, &wallet).ok_or(Error::NotAttested)?;
        record.revoked = true;
        storage::set_attestation(&env, &wallet, &record);

        let revoked_at_ledger = env.ledger().sequence();
        events::attestation_revoked(&env, &wallet, revoked_at_ledger);
        Ok(())
    }

    // ── View functions ────────────────────────────────────────────────────────

    /// Returns `true` iff the wallet has a valid, unexpired, non-revoked
    /// KYC attestation.
    ///
    /// This is the primary hook for the invoice and investment contracts.
    /// No auth required – callable by anyone (including cross-contract calls).
    pub fn is_verified(env: Env, wallet: Address) -> bool {
        match storage::get_attestation(&env, &wallet) {
            None => false,
            Some(record) => !record.revoked && env.ledger().sequence() < record.expiry_ledger,
        }
    }

    /// Returns the full attestation record for `wallet`.
    ///
    /// # Returns
    /// `(credential_hash, attested_ledger, expiry_ledger)`
    ///
    /// # Errors
    /// * [`Error::NotInit`] – contract not yet initialised.
    /// * [`Error::NotAttested`] – no attestation record exists for `wallet`.
    pub fn get_attestation(env: Env, wallet: Address) -> Result<(BytesN<32>, u32, u32), Error> {
        storage::get_oracle(&env).ok_or(Error::NotInit)?;
        let record = storage::get_attestation(&env, &wallet).ok_or(Error::NotAttested)?;
        Ok((
            record.credential_hash,
            record.attested_ledger,
            record.expiry_ledger,
        ))
    }

    /// Returns the configured KYC oracle address.
    ///
    /// # Errors
    /// * [`Error::NotInit`] – contract not yet initialised.
    pub fn get_oracle(env: Env) -> Result<Address, Error> {
        storage::get_oracle(&env).ok_or(Error::NotInit)
    }
}

#[cfg(test)]
mod test;
