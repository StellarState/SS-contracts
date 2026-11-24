//! Data types for the KYC attestation contract.

use soroban_sdk::{contracttype, Address, BytesN};

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: KYC oracle address.
    Oracle,
    /// Persistent: attestation record for a wallet.
    Attestation(Address),
}

/// On-chain attestation record stored per wallet.
///
/// `credential_hash` is the 32-byte SHA-256 (or equivalent) hash of the
/// off-chain KYC credential. The raw personal data never touches the chain.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attestation {
    /// 32-byte hash of the KYC credential document / identity proof.
    pub credential_hash: BytesN<32>,
    /// Ledger sequence number at which the attestation was recorded.
    pub attested_ledger: u32,
    /// Ledger sequence number at which the attestation expires.
    /// `is_verified` returns `false` once `env.ledger().sequence() >= expiry_ledger`.
    pub expiry_ledger: u32,
    /// Whether the oracle has explicitly revoked this attestation before expiry.
    pub revoked: bool,
}
