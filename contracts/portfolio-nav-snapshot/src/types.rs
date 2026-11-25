use soroban_sdk::{contracttype, Address, BytesN, Symbol};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NavRecord {
    pub date: u64,
    pub nav_amount: i128,
}

/// Oracle inputs reported for NAV snapshot verification (#817).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleInputs {
    /// Outstanding supply of shares/units the NAV is measured against.
    pub outstanding_supply: i128,
    /// Reserve (total backing assets) snapshot component.
    pub reserve: i128,
    /// Ledger timestamp the inputs were reported at.
    pub timestamp: u64,
}

/// Outcome of `verify_nav_snapshot` (#817).
///
/// Verification failures are reported as `verified: false` with a `reason`
/// rather than a contract error, so the `nav_snapshot_rejected` event is
/// committed on-chain (a rejected transaction would roll its events back).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NavVerification {
    pub wallet: Address,
    /// Snapshot date that was verified.
    pub date: u64,
    /// NAV amount recorded in the snapshot.
    pub nav_amount: i128,
    /// Outstanding supply component used for the check.
    pub outstanding_supply: i128,
    /// Reserve component used for the check.
    pub reserve: i128,
    /// Whether the snapshot NAV matched reserve/supply within rounding.
    pub verified: bool,
    /// Machine-readable rejection reason when `verified` is false.
    pub reason: Option<Symbol>,
    /// Ledger sequence the verification ran at.
    pub ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Operator,
    Snapshot(Address, u64),
    SnapshotDates(Address),
    HeldInvoices(Address),
    FractionBalance(Address, BytesN<32>),
    InvoicePrice(BytesN<32>),
    /// Instance: latest oracle inputs used for NAV verification —
    /// outstanding supply, reserve total and the ledger timestamp they
    /// were reported at.
    OracleInputs,
    /// Instance: maximum accepted age (seconds) of oracle inputs.
    OracleMaxAge,
    /// Persistent: last NAV verification outcome per wallet.
    LastVerification(Address),
    /// Persistent: latest verified NAV amount per wallet. Only updated
    /// when verification succeeds, so a rejected snapshot never replaces
    /// the current NAV.
    CurrentNav(Address),
}
