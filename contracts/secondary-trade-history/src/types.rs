//! Data types for the secondary market trade history contract.

use soroban_sdk::{contracttype, BytesN};

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: contract admin.
    Admin,
    /// Instance: the sole address authorized to call `record_trade`
    /// (normally the deployed secondary-market contract). Fake trade
    /// records would corrupt on-chain price discovery, so recording is
    /// gated to one trusted caller rather than left open.
    Recorder,
    /// Persistent: trade history for an invoice, newest-first.
    History(BytesN<32>),
}

/// One executed secondary market trade for a given invoice.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TradeRecord {
    pub price: i128,
    pub amount: i128,
    pub timestamp: u64,
}
