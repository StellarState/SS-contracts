//! Data types for the invoice issuer verification badge contract.

use soroban_sdk::{contracttype, Address};

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: contract admin.
    Admin,
    /// Persistent: verification record for an issuer wallet.
    Verification(Address),
}

/// Verification tier granted to an issuer after due diligence.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum VerificationTier {
    /// Baseline KYC/KYB checks passed.
    BasicVerified = 0,
    /// Additional document and financial review passed.
    EnhancedVerified = 1,
    /// Institutional-grade due diligence passed (e.g. regulated entity).
    Institutional = 2,
}

/// A verification record stored for one issuer wallet.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationRecord {
    pub tier: VerificationTier,
    /// Unix timestamp after which this verification is no longer valid.
    pub expiry_timestamp: u64,
}

/// Result of `is_verified`: whether the issuer currently holds a
/// non-expired verification, and if so, its tier and expiry.
///
/// `tier`/`expiry_timestamp` are only meaningful when `verified` is true;
/// when `verified` is false they are `VerificationTier::BasicVerified` and
/// `0` respectively (sentinel values, not a real grant), since
/// `Option<VerificationTier>` cannot currently round-trip through
/// `#[contracttype]`'s generated `ScVal` conversion for a custom enum.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationStatus {
    pub verified: bool,
    pub tier: VerificationTier,
    pub expiry_timestamp: u64,
}
