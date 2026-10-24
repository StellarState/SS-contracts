//! Error types for the invoice issuer verification badge contract.

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
    /// Caller is not the admin.
    Unauthorized = 3,
    /// Address is the zero address (all-zero 32-byte key).
    InvalidAddress = 4,
    /// expiry_timestamp must be in the future relative to the current ledger time.
    InvalidExpiry = 5,
    /// No verification record exists for this issuer.
    NotVerified = 6,
}
