//! Data types for the platform treasury management contract.

use soroban_sdk::{contracttype, Address, String};

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: contract admin.
    Admin,
    /// Instance: running total across all sources.
    TotalBalance,
    /// Instance: ordered, deduplicated list of every source address that
    /// has ever called `receive_fee`, used to build the per-source
    /// breakdown in `get_treasury_balance` (storage has no key
    /// enumeration, so the set of known sources must be tracked
    /// explicitly).
    KnownSources,
    /// Persistent: current balance attributed to one fee source.
    SourceBalance(Address),
    /// Instance: number of allocation records ever written, used as
    /// both the next record's index and the paginated log's length.
    AllocationCount,
    /// Persistent: one allocation record by its index.
    Allocation(u32),
}

/// One historical allocation of treasury funds to a destination.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllocationRecord {
    pub destination: Address,
    pub amount: i128,
    pub purpose: String,
    pub timestamp: u64,
}

/// Result of `get_treasury_balance`: the total balance and a per-source
/// breakdown, returned as parallel vectors since `Map` with an `Address`
/// key is not directly returnable the same way across every soroban-sdk
/// client binding, while parallel vectors are.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreasuryBalance {
    pub total: i128,
    pub sources: soroban_sdk::Vec<Address>,
    pub amounts: soroban_sdk::Vec<i128>,
}
