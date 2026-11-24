//! Data types for the invoice token contract (SEP-41).
//! Storage key names respect Soroban's 10-character limit for contracttype.

use soroban_sdk::{contracttype, xdr::ToXdr, Address, Env};

/// Largest supported number of fractional digits for an invoice sub-asset.
pub const MAX_DECIMALS: u32 = 18;

/// Return whether an address has an all-zero account or contract payload.
pub fn is_zero_address(env: &Env, address: &Address) -> bool {
    let encoded = address.to_xdr(env);
    let (payload_start, payload_end) = match encoded.len() {
        // ScVal::Address + ScAddress::Account + PublicKey::Ed25519.
        44 => (12, 44),
        // ScVal::Address + ScAddress::Contract.
        40 => (8, 40),
        _ => return false,
    };

    (payload_start..payload_end).all(|index| encoded.get(index) == Some(0))
}

/// Storage key enum for instance and persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Instance: token metadata and config.
    Metadata,
    /// Instance: total supply.
    TotalSupply,
    /// Instance: immutable supply cap derived from invoice terms.
    MaxSupply,
    /// Persistent: balance by holder address.
    Balance(soroban_sdk::Address),
    /// Persistent: allowance (from, spender) -> AllowanceData.
    Allowance(soroban_sdk::Address, soroban_sdk::Address),
    /// Instance: fee basis points.
    FeeBps,
    /// Instance: authorized secondary-market settlement contract.
    SecSettle,
    /// Instance: secondary-sale platform-fee treasury.
    SecTreasry,
    /// Instance: invoice issuer receiving secondary-sale royalties.
    SecIssuer,
    /// Instance: secondary-sale platform fee in basis points.
    SecPlatBps,
    /// Instance: secondary-sale issuer royalty in basis points.
    SecRoyBps,
    /// Persistent: unwithdrawn secondary-sale earnings by account.
    Earnings(soroban_sdk::Address),
    /// Instance: role admin mapping (role -> admin address).
    RoleAdmin(soroban_sdk::Symbol),
    /// Instance: role grant mapping (role, account) -> bool.
    RoleGrant(soroban_sdk::Symbol, soroban_sdk::Address),
    /// Persistent: nonce per address for permit-style transfers.
    Nonce(soroban_sdk::Address),
    /// Persistent: ownership history records for a token holder.
    History(soroban_sdk::Address),
    /// Persistent: whether an account is restricted from token operations.
    Frozen(soroban_sdk::Address),
    /// Instance: whether direct transfer_from is allowed without recipient consent.
    DirectPol,
    /// Instance: auto-incrementing ID for pending transfer requests.
    NextXferId,
    /// Persistent: pending consent-gated transfer by ID.
    PendXfer(u64),
    /// Persistent: consumed transfer consent nonce by (from, nonce).
    UsedXNonce(soroban_sdk::Address, u64),
}

/// Secondary-sale payout configuration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecondaryConfig {
    pub settlement: soroban_sdk::Address,
    pub treasury: soroban_sdk::Address,
    pub issuer: soroban_sdk::Address,
    pub platform_fee_bps: u32,
    pub royalty_bps: u32,
}

/// Pending consent-gated position transfer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingTransfer {
    pub from: soroban_sdk::Address,
    pub to: soroban_sdk::Address,
    pub amount: i128,
    pub nonce: u64,
    pub expiry_ledger: u32,
}

/// Token metadata and admin config (instance storage).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenMetadata {
    /// Admin address (can mint, set minter, set transfer_locked).
    pub admin: soroban_sdk::Address,
    /// Escrow contract (or minter) address allowed to mint.
    pub minter: soroban_sdk::Address,
    /// Token name (e.g. "Invoice INV-001 Token").
    pub name: soroban_sdk::String,
    /// Token symbol (e.g. "INV001").
    pub symbol: soroban_sdk::String,
    /// Number of decimals (e.g. 7).
    pub decimals: u32,
    /// Invoice identifier this token represents (Symbol for storage efficiency).
    pub invoice_id: soroban_sdk::Symbol,
    /// Maximum token supply, derived from face value / minimum fraction size.
    pub max_supply: i128,
    /// If true, transfers restricted until settlement (admin can still transfer).
    pub transfer_locked: bool,
    /// Emergency pause flag for sensitive token operations.
    pub paused: bool,
    /// Optional lock-up contract consulted before holder transfers.
    pub transfer_restriction: Option<soroban_sdk::Address>,
}

/// Allowance entry: amount and expiration ledger.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllowanceData {
    pub amount: i128,
    pub expiration_ledger: u32,
}

/// A single ownership history record for a token holder.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnershipHistoryRecord {
    /// The previous owner before this transfer.
    pub from: soroban_sdk::Address,
    /// The new owner (recipient) of the tokens.
    pub to: soroban_sdk::Address,
    /// Amount of tokens transferred in this event.
    pub amount: i128,
    /// Ledger sequence at the time of the transfer.
    pub ledger: u32,
}
