//! Invoice issuer verification badge contract for StellarSettle.
//!
//! The admin grants a verification tier to an issuer wallet after it has
//! passed off-chain due diligence. The badge, visible via `is_verified`, is
//! a trust signal displayed alongside an issuer's invoices to investors.
//! Verification expires automatically at `expiry_timestamp` and can also be
//! revoked immediately by the admin before that.

#![no_std]

mod errors;
mod events;
mod storage;
mod types;

pub use types::{VerificationRecord, VerificationStatus, VerificationTier};

use soroban_sdk::{contract, contractimpl, Address, Env, String};

use errors::Error;

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
pub struct InvoiceIssuerVerification;

#[contractimpl]
impl InvoiceIssuerVerification {
    /// Initialize the contract with the admin allowed to grant/revoke verification.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        ensure_non_zero_address(&env, &admin)?;
        if storage::get_admin(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        storage::set_admin(&env, &admin);
        events::initialized(&env, &admin);
        Ok(())
    }

    /// Admin-only: grant (or re-grant, overwriting any existing record)
    /// a verification tier to `issuer_wallet`, valid until `expiry_timestamp`.
    pub fn grant_verification(
        env: Env,
        admin: Address,
        issuer_wallet: Address,
        tier: VerificationTier,
        expiry_timestamp: u64,
    ) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();
        ensure_non_zero_address(&env, &issuer_wallet)?;

        if expiry_timestamp <= env.ledger().timestamp() {
            return Err(Error::InvalidExpiry);
        }

        let record = VerificationRecord {
            tier,
            expiry_timestamp,
        };
        storage::set_verification(&env, &issuer_wallet, &record);
        events::verification_granted(&env, &issuer_wallet, tier, expiry_timestamp);
        Ok(())
    }

    /// Admin-only: immediately revoke `issuer_wallet`'s verification,
    /// regardless of its current expiry. `is_verified` returns false for
    /// this wallet right after this call, in the same transaction.
    pub fn revoke_verification(
        env: Env,
        admin: Address,
        issuer_wallet: Address,
    ) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();

        if storage::get_verification(&env, &issuer_wallet).is_none() {
            return Err(Error::NotVerified);
        }
        storage::remove_verification(&env, &issuer_wallet);
        events::verification_revoked(&env, &issuer_wallet);
        Ok(())
    }

    /// View: whether `issuer_wallet` currently holds a non-expired
    /// verification, and if so, its tier and expiry. A verification whose
    /// `expiry_timestamp` is at or before the current ledger time is
    /// treated as not verified without needing a separate expiry call —
    /// expiry is read-time, not an action that mutates storage.
    pub fn is_verified(env: Env, issuer_wallet: Address) -> VerificationStatus {
        match storage::get_verification(&env, &issuer_wallet) {
            Some(record) if record.expiry_timestamp > env.ledger().timestamp() => {
                VerificationStatus {
                    verified: true,
                    tier: record.tier,
                    expiry_timestamp: record.expiry_timestamp,
                }
            }
            _ => VerificationStatus {
                verified: false,
                tier: VerificationTier::BasicVerified,
                expiry_timestamp: 0,
            },
        }
    }

    /// View: return the current admin.
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)
    }
}

#[cfg(test)]
mod test;
