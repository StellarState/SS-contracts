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
    /// Fee source is not on the approved allowlist.
    SourceNotApproved = 9,
    /// Fee receipt would exceed the source's configured cap.
    SourceCapExceeded = 10,
    /// Configured source cap must be strictly positive.
    InvalidCap = 11,
    /// A previous allocation is still within the configured cooldown.
    AllocationCooldownActive = 12,
    /// `expected_revision` does not match the current treasury balance
    /// revision, so the allocation was computed against a stale balance.
    StaleRevision = 13,
    /// The allocation would push the ledger day's cumulative total above
    /// the configured daily allocation limit.
    DailyLimitExceeded = 14,
    /// Daily allocation limit must be strictly positive when set.
    InvalidDailyLimit = 15,
}
