//! Error types for the platform treasury management contract.

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
    /// amount must be strictly positive.
    InvalidAmount = 5,
    /// allocate requested more than the current total balance.
    InsufficientBalance = 6,
    /// purpose string must not be empty.
    InvalidPurpose = 7,
    /// Arithmetic overflow while updating a balance or count.
    Overflow = 8,
}
