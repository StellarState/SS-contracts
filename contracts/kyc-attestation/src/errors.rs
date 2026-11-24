//! Error types for the KYC attestation contract.

use soroban_sdk::contracterror;

/// Errors that can occur during contract execution.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    /// Contract has already been initialized.
    AlreadyInit = 1,
    /// Contract has not been initialized.
    NotInit = 2,
    /// Caller is not the configured KYC oracle.
    Unauthorized = 3,
    /// Address is the zero address (all-zero 32-byte key).
    InvalidAddress = 4,
    /// The expiry ledger must be strictly greater than the current ledger.
    InvalidExpiry = 5,
    /// No attestation exists for the given wallet.
    NotAttested = 6,
}
