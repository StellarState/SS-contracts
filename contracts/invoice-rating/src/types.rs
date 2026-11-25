use soroban_sdk::{contracttype, Address, BytesN, Vec};

/// Minimum number of ratings before an aggregate is considered valid.
pub const MIN_RATINGS: u32 = 3;
pub const MIN_SCORE: u32 = 1;
pub const MAX_SCORE: u32 = 5;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceInfo {
    pub issuer: Address,
    pub settled: bool,
}

/// Legacy running totals kept under the original invoice and issuer storage keys.
///
/// New aggregates are stored as `RatingStatsV2`; legacy values are read and
/// converted when no v2 value exists.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RatingStats {
    pub total_ratings: u32,
    pub score_sum: u32,
    /// Count of ratings per score; index 0 holds score 1.
    pub distribution: Vec<u32>,
}

/// Running totals stored under versioned keys so legacy u32 sums remain decodable.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RatingStatsV2 {
    pub total_ratings: u32,
    pub score_sum: u64,
    /// Count of ratings per score; index 0 holds score 1.
    pub distribution: Vec<u32>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AggregateRating {
    /// Average score scaled by 100 (e.g. 425 = 4.25).
    pub average_x100: u32,
    pub total_ratings: u32,
    pub distribution: Vec<u32>,
    /// True once at least `MIN_RATINGS` ratings have been submitted.
    pub valid: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    Operator,
    Invoice(BytesN<32>),
    Holder(BytesN<32>, Address),
    Rating(BytesN<32>, Address),
    InvoiceStats(BytesN<32>),
    IssuerStats(Address),
    InvoiceStatsV2(BytesN<32>),
    IssuerStatsV2(Address),
}
