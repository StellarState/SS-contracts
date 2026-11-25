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
    /// Instance: minimum ledger interval between treasury allocations.
    AllocationCooldown,
    /// Instance: ledger of the last treasury allocation.
    LastAllocationLedger,
    /// Persistent: one allocation record by its index.
    Allocation(u32),
    /// Instance: whether only approved sources can call `receive_fee`.
    SourceAllowlistEnforced,
    /// Persistent: whether a source address is approved.
    ApprovedSource(Address),
    /// Persistent: optional cumulative cap for a source address.
    SourceCap(Address),
    /// Persistent: cumulative fees received from a source.
    SourceCumulativeTotal(Address),
    /// Persistent: fees received from a source on a specific ledger day.
    SourceDailyTotal(Address, u64),
    /// Instance: monotonic treasury balance revision, bumped on every
    /// balance mutation (`receive_fee` credit or `allocate` debit). Used to
    /// bind allocation receipts and reject stale allocation attempts.
    TreasuryRevision,
    /// Instance: number of allocation receipts ever written.
    ReceiptCount,
    /// Persistent: one signed-style audit receipt by its id.
    Receipt(u64),
    /// Instance: optional per-ledger-day cap on total allocations.
    DailyAllocationLimit,
    /// Persistent: cumulative allocations and breach-notification flag for
    /// one ledger day (`timestamp / 86_400`).
    DailyAllocation(u64),
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

/// Immutable audit receipt written for every treasury allocation (#825).
///
/// Binds the allocator, destination, amount, purpose and ledger to the
/// treasury balance revision observed after the allocation was applied, so
/// reconciliation can detect any intervening balance change.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllocationReceipt {
    /// Monotonic receipt id (0-based, matches allocation log order).
    pub receipt_id: u64,
    /// Admin address that authorised the allocation.
    pub allocator: Address,
    /// Destination the funds were routed to.
    pub destination: Address,
    /// Amount allocated, in stroops. Always equals the allocation's
    /// recorded `AllocationRecord::amount`.
    pub amount: i128,
    /// Free-form purpose string supplied by the allocator.
    pub purpose: String,
    /// Ledger sequence the allocation was applied at.
    pub ledger: u32,
    /// Ledger timestamp the allocation was applied at.
    pub ledger_timestamp: u64,
    /// Treasury balance revision after this allocation was applied.
    pub treasury_revision: u64,
}

/// One page of the cursor-paginated receipt history (#825).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptPage {
    /// Receipts in ascending id order.
    pub receipts: soroban_sdk::Vec<AllocationReceipt>,
    /// Cursor to pass to the next call, or `None` when the history is
    /// exhausted.
    pub next_cursor: Option<u64>,
}

/// Per-ledger-day cumulative allocation state (#824).
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DailyAllocation {
    /// Total allocated so far on this ledger day.
    pub total: i128,
    /// Whether `daily_treasury_limit_reached` was already emitted for this
    /// day, so the event fires once per breach window.
    pub limit_notified: bool,
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
