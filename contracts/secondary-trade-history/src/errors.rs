//! Error types for the secondary market trade history contract.

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
    /// price or amount must be strictly positive.
    InvalidTrade = 5,
    /// Caller is not the authorized recorder.
    NotRecorder = 6,
    /// No trade has ever been recorded for this invoice.
    NoTrades = 7,
}
