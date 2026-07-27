//! Data types for the invoice escrow contract.
//! All names respect Soroban's 10-character limit for contracttype.

use soroban_sdk::contracttype;

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: global config.
    Config,
    /// Persistent: escrow data by invoice id.
    Escrow(soroban_sdk::Symbol),
    /// Persistent: funder amounts by (invoice_id, funder_address).
    FunderAmount(soroban_sdk::Symbol, soroban_sdk::Address),
    /// Persistent: whether a given address is whitelisted to fund (buy) escrows.
    BuyerWhitelist(soroban_sdk::Address),
    /// Instance: per-category fee schedule overrides.
    CategoryFee(InvoiceCategory),
}

/// Invoice category used to select a per-category fee schedule override.
///
/// When a `CategoryFeeSchedule` is set for a given category, escrows created
/// under that category use the category-level fee instead of the global
/// `Config.fee_bps`.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum InvoiceCategory {
    /// Standard commercial invoice (default).
    Standard = 0,
    /// Invoice factoring: seller sells the receivable at a discount.
    Factoring = 1,
    /// Reverse factoring / supply-chain finance: buyer-initiated.
    Reverse = 2,
    /// Government / public-sector invoice.
    Government = 3,
}

/// Per-category fee schedule. Stored in instance storage keyed by
/// `StorageKey::CategoryFee(category)`.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CategoryFeeSchedule {
    /// Platform fee in basis points for this category (e.g. 250 = 2.5%).
    pub fee_bps: u32,
}

/// Global contract configuration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    /// Admin address (fee updates, platform recipient).
    pub admin: soroban_sdk::Address,
    /// Platform fee in basis points (e.g. 300 = 3%).
    pub fee_bps: u32,
    /// Optional payment distributor contract used for settlement/refund fan-out.
    pub payment_distributor: Option<soroban_sdk::Address>,
    /// Emergency pause flag for lifecycle-changing operations.
    pub paused: bool,
    /// When true, `fund_escrow` requires the buyer to be on the whitelist.
    /// Defaults to false (opt-in) so existing deployments/tests are unaffected
    /// until an admin explicitly enables it.
    pub whitelist_enabled: bool,
}

/// Lifecycle status of an escrow.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EscrowStatus {
    /// Created, awaiting funding.
    Created = 0,
    /// Funded by investor.
    Funded = 1,
    /// Payment recorded and distributed.
    Settled = 2,
    /// Refunded to investor after due date.
    Refunded = 3,
    /// Cancelled by seller while still in Created state (never funded).
    Cancelled = 4,
}

/// Per-invoice escrow data stored in persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowData {
    /// Invoice identifier (Symbol, ≤10 chars when used as key).
    pub inv_id: soroban_sdk::Symbol,
    /// Seller (invoice owner).
    pub seller: soroban_sdk::Address,
    /// Debtor (authorized payer of the invoice).
    pub debtor: soroban_sdk::Address,
    /// Face value: what the debtor owes (amount to be paid at settlement).
    pub face_value: i128,
    /// Purchase price: total amount to be funded by all investors (discount applied here).
    pub purchase_price: i128,
    /// Total amount funded so far by all investors.
    pub funded_amt: i128,
    /// Primary funder address (MVP: single funder for now).
    pub funder: Option<soroban_sdk::Address>,
    /// Due date (ledger timestamp).
    pub due_dt: u64,
    /// Payment token contract address.
    pub token: soroban_sdk::Address,
    /// Invoice token contract address (ownership/claim).
    pub inv_token: soroban_sdk::Address,
    /// Amount already paid by payer.
    pub paid_amt: i128,
    /// Current status.
    pub status: EscrowStatus,
    /// Commitment hash: immutable on-chain anchor for off-chain invoice data (PDF hash, ERP ID, etc.).
    /// Set at creation, cannot be modified. SHA-256 hash (32 bytes).
    pub commitment: soroban_sdk::BytesN<32>,
    /// Invoice category used to determine per-category fee override.
    pub category: InvoiceCategory,
    /// Effective platform fee in basis points stamped at creation time.
    /// Derived from the per-category override if one is set, otherwise the
    /// global `Config.fee_bps`. Stored so that fee changes after creation
    /// do not affect outstanding escrows.
    pub effective_fee_bps: u32,
}
