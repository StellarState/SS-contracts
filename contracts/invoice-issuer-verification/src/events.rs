#![allow(deprecated)]
//! Events emitted by the invoice issuer verification badge contract.

use soroban_sdk::{Address, Env, Symbol};

use crate::types::VerificationTier;

/// Emitted when the contract is initialized.
pub fn initialized(env: &Env, admin: &Address) {
    let topics = (Symbol::new(env, "initialized"),);
    env.events().publish(topics, admin.clone());
}

/// Emitted when an issuer is granted (or re-granted) verification.
/// Topics: (VerificationGranted, issuer); Data: (tier, expiry_timestamp).
pub fn verification_granted(
    env: &Env,
    issuer: &Address,
    tier: VerificationTier,
    expiry_timestamp: u64,
) {
    let topics = (Symbol::new(env, "VerificationGranted"), issuer.clone());
    env.events()
        .publish(topics, (tier as u32, expiry_timestamp));
}

/// Emitted when an issuer's verification is revoked.
/// Topics: (VerificationRevoked, issuer); Data: ().
pub fn verification_revoked(env: &Env, issuer: &Address) {
    let topics = (Symbol::new(env, "VerificationRevoked"), issuer.clone());
    env.events().publish(topics, ());
}
