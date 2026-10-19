use soroban_sdk::{contracttype, Address, BytesN};

pub const DEFAULT_LIMIT_BPS: u32 = 3_000; // 30% default
pub const MAX_BPS: u32 = 10_000;          // 100%

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Limit(Address),
    Position(Address, BytesN<32>),
    Total(Address),
    PortfolioBase(Address),
    Invoices(Address),
}

/// Breach and concentration check details returned by `check_diversification`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiversificationCheck {
    pub eligible: bool,
    pub current_bps: u32,
    pub projected_bps: u32,
    pub max_bps: u32,
    pub breach: bool,
}

/// Per-invoice portfolio concentration details.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceConcentration {
    pub invoice_id: BytesN<32>,
    pub amount: i128,
    pub concentration_bps: u32,
}
