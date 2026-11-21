#![allow(deprecated)]
//! Events emitted by the invoice metadata registry contract.

use soroban_sdk::{Address, BytesN, Env, Symbol};

use crate::types::InvoiceStatus;

/// Emitted when the registry is initialized.
pub fn initialized(env: &Env, admin: &Address) {
    let topics = (Symbol::new(env, "initialized"),);
    env.events().publish(topics, admin.clone());
}

/// Emitted when a new invoice is registered.
/// Topics: (InvoiceRegistered, invoice_id); Data: issuer.
pub fn invoice_registered(env: &Env, invoice_id: &BytesN<32>, issuer: &Address) {
    let topics = (Symbol::new(env, "InvoiceRegistered"), invoice_id.clone());
    env.events().publish(topics, issuer.clone());
}

/// Emitted when an invoice's status changes.
/// Topics: (InvoiceStatusUpdated, invoice_id); Data: (old_status, new_status).
pub fn invoice_status_updated(
    env: &Env,
    invoice_id: &BytesN<32>,
    old_status: InvoiceStatus,
    new_status: InvoiceStatus,
) {
    let topics = (Symbol::new(env, "InvoiceStatusUpdated"), invoice_id.clone());
    env.events()
        .publish(topics, (old_status as u32, new_status as u32));
}

/// Emitted when invoice metadata changes.
pub fn metadata_updated(env: &Env, invoice_id: &BytesN<32>, old_revision: u32, new_revision: u32) {
    let topics = (Symbol::new(env, "metadata_updated"), invoice_id.clone());
    env.events().publish(topics, (old_revision, new_revision));
}
