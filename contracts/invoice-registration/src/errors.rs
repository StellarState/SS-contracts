//! Error types for the invoice registration contract.

use soroban_sdk::contracterror;

/// Errors that can occur during contract execution.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    /// External invoice id is empty or exceeds the maximum supported length.
    InvalidExternalId = 1,
    /// This `(issuer, external_invoice_id)` key is already registered.
    AlreadyRegistered = 2,
    /// This internal invoice id is already bound to a registration.
    InvoiceAlreadyRegistered = 3,
    /// No registration exists for the supplied key or invoice id.
    NotRegistered = 4,
    /// Address is the zero address (all-zero 32-byte key).
    InvalidAddress = 5,
}
