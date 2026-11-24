#![allow(deprecated)]
//! Events emitted by the KYC attestation contract.

use soroban_sdk::{Address, BytesN, Env, Symbol};

/// Emitted when a wallet is successfully attested by the oracle.
///
/// Topics: (`WalletAttested`, wallet)
/// Data:   (credential_hash, attested_ledger, expiry_ledger)
pub fn wallet_attested(
    env: &Env,
    wallet: &Address,
    credential_hash: &BytesN<32>,
    attested_ledger: u32,
    expiry_ledger: u32,
) {
    let topics = (Symbol::new(env, "WalletAttested"), wallet.clone());
    env.events().publish(
        topics,
        (credential_hash.clone(), attested_ledger, expiry_ledger),
    );
}

/// Emitted when the oracle revokes a wallet's attestation before expiry.
///
/// Topics: (`AttestationRevoked`, wallet)
/// Data:   revoked_at_ledger
pub fn attestation_revoked(env: &Env, wallet: &Address, revoked_at_ledger: u32) {
    let topics = (Symbol::new(env, "AttestationRevoked"), wallet.clone());
    env.events().publish(topics, revoked_at_ledger);
}
