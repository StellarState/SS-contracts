//! Data types for the invoice metadata registry contract.

use soroban_sdk::{contracttype, Address, BytesN};

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: registry admin.
    Admin,
    /// Persistent: full metadata for an invoice.
    Invoice(BytesN<32>),
    /// Persistent: append-only metadata revision history for an invoice.
    Revisions(BytesN<32>),
}

/// Lifecycle status of a registered invoice.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum InvoiceStatus {
    /// Registered but not yet opened for investment.
    Pending = 0,
    /// Open for investment.
    Live = 1,
    /// Fully funded; awaiting settlement.
    Funded = 2,
    /// Repaid and closed. Terminal.
    Settled = 3,
    /// Did not reach `Funded` before its maturity date, or was withdrawn.
    /// Terminal.
    Expired = 4,
}

impl InvoiceStatus {
    /// Whether moving from `self` to `next` is a valid state machine
    /// transition.
    ///
    /// Allowed edges: `Pending -> Live`, `Pending -> Expired`,
    /// `Live -> Funded`, `Live -> Expired`, `Funded -> Settled`. `Settled`
    /// and `Expired` are terminal: no edge leaves them. Self-transitions
    /// (`status == next`) are never valid; a caller re-applying the same
    /// status is almost certainly a mistake, and treating it as a no-op
    /// would silently swallow that mistake rather than surfacing it.
    pub fn can_transition_to(self, next: InvoiceStatus) -> bool {
        matches!(
            (self, next),
            (InvoiceStatus::Pending, InvoiceStatus::Live)
                | (InvoiceStatus::Pending, InvoiceStatus::Expired)
                | (InvoiceStatus::Live, InvoiceStatus::Funded)
                | (InvoiceStatus::Live, InvoiceStatus::Expired)
                | (InvoiceStatus::Funded, InvoiceStatus::Settled)
        )
    }
}

/// Immutable registration parameters for an invoice, supplied at
/// `register_invoice` time. `face_value` and `yield_amount` are expressed in
/// the smallest unit of whatever asset the invoice is denominated in (e.g.
/// stroops), matching how amounts are represented elsewhere in this
/// workspace (see `invoice-escrow`).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceMetadataInput {
    /// Face value of the invoice.
    pub face_value: i128,
    /// Yield offered to investors over the invoice's term.
    pub yield_amount: i128,
    /// Unix timestamp the invoice matures.
    pub maturity_date: u64,
    /// Address of the invoice issuer.
    pub issuer: Address,
}

/// Full on-chain record for a registered invoice, as returned by
/// `get_invoice`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceMetadata {
    /// Invoice this record describes.
    pub invoice_id: BytesN<32>,
    /// Face value of the invoice.
    pub face_value: i128,
    /// Yield offered to investors over the invoice's term.
    pub yield_amount: i128,
    /// Unix timestamp the invoice matures.
    pub maturity_date: u64,
    /// Address of the invoice issuer.
    pub issuer: Address,
    /// Current lifecycle status.
    pub status: InvoiceStatus,
    /// Address authorized to edit metadata for this invoice.
    pub metadata_editor: Address,
    /// Current metadata revision, starting at 1 on registration.
    pub revision: u32,
}

/// One immutable entry in an invoice's metadata revision history.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataRevision {
    /// Revision number, starting at 1.
    pub revision: u32,
    /// Metadata values at this revision.
    pub metadata: InvoiceMetadataInput,
    /// Account that authorized this revision.
    pub updated_by: Address,
    /// Ledger timestamp when the revision was created.
    pub timestamp: u64,
}
