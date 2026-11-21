//! Error types for the invoice metadata registry contract.

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
    /// No invoice exists for the given ID.
    NotRegistered = 4,
    /// An invoice with this ID is already registered.
    AlreadyRegistered = 5,
    /// Address is the zero address (all-zero 32-byte key).
    InvalidAddress = 6,
    /// Face value or yield supplied at registration is not positive.
    InvalidAmount = 7,
    /// The requested status transition is not a valid move in the invoice
    /// state machine (see `InvoiceStatus::can_transition_to`).
    InvalidTransition = 8,
    /// Metadata cannot be edited once an invoice is funded or terminal.
    MetadataLocked = 9,
    /// Metadata revision counter overflowed.
    RevisionOverflow = 10,
}
