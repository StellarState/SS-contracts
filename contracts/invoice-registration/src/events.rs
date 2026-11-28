#![allow(deprecated)]
//! Events emitted by the invoice registration contract.

use soroban_sdk::{Address, BytesN, Env, String, Symbol};

/// Emitted exactly once, on the first successful registration of a key.
/// Topics: (invoice_registered, issuer); Data: (external_invoice_id, invoice_id).
pub fn invoice_registered(
    env: &Env,
    issuer: &Address,
    external_invoice_id: &String,
    invoice_id: &BytesN<32>,
) {
    let topics = (Symbol::new(env, "invoice_registered"), issuer.clone());
    env.events()
        .publish(topics, (external_invoice_id.clone(), invoice_id.clone()));
}
