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
    /// Purpose: Stores global token configuration and metadata (admin, symbol, decimals, etc.).
    /// Storage Type: Instance storage.
    /// Access Pattern: Read frequently on metadata queries; written during initialization or by admin.
    /// TTL Policy: Bumped automatically with the contract instance.
    Metadata,
    
    /// Purpose: Tracks the total circulating supply of the token.
    /// Storage Type: Instance storage.
    /// Access Pattern: Read on `total_supply` queries; written during minting or burning operations.
    /// TTL Policy: Bumped automatically with the contract instance.
    TotalSupply,
    
    /// Purpose: Tracks the token balance for a specific holder address.
    /// Storage Type: Persistent storage.
    /// Access Pattern: Read on balance queries and transfers; written during transfers, minting, or burning.
    /// TTL Policy: Must be explicitly bumped to prevent loss of user funds.
    Balance(soroban_sdk::Address),
    
    /// Purpose: Tracks the delegated spending allowance (`AllowanceData`) from an owner to a spender.
    /// Storage Type: Persistent storage.
    /// Access Pattern: Read on `allowance` queries and `transfer_from`; written during `approve`.
    /// TTL Policy: Explicitly bumped; expires based on the `expiration_ledger` in `AllowanceData`.
    Allowance(soroban_sdk::Address, soroban_sdk::Address),
    
    /// Purpose: Stores the fee basis points applied to transfers (if applicable).
    /// Storage Type: Instance storage.
    /// Access Pattern: Read during fee-enabled transfers; written by admin.
    /// TTL Policy: Bumped automatically with the contract instance.
    FeeBps,
    
    /// Purpose: Maps a specific role (e.g., minter) to its administrator address.
    /// Storage Type: Instance storage.
    /// Access Pattern: Read when checking administrative rights; written by super admin.
    /// TTL Policy: Bumped automatically with the contract instance.
    RoleAdmin(soroban_sdk::Symbol),
    
    /// Purpose: Tracks whether a specific account has been granted a particular role.
    /// Storage Type: Instance storage.
    /// Access Pattern: Read during role-restricted operations; written when granting/revoking roles.
    /// TTL Policy: Bumped automatically with the contract instance.
    RoleGrant(soroban_sdk::Symbol, soroban_sdk::Address),
    
    /// Purpose: Tracks the current nonce for permit-style operations or replay protection.
    /// Storage Type: Persistent storage.
    /// Access Pattern: Read during signature verification; incremented on successful operation.
    /// TTL Policy: Explicitly bumped to maintain sequence history.
    Nonce(soroban_sdk::Address),
    
    /// Purpose: Stores a list of ownership history records (`OwnershipHistoryRecord`) for an address.
    /// Storage Type: Persistent storage.
    /// Access Pattern: Read on history queries; appended to during transfers.
    /// TTL Policy: Explicitly bumped to preserve historical data.
    History(soroban_sdk::Address),
    
    /// Purpose: Tracks whether an account is restricted/frozen from token operations.
    /// Storage Type: Persistent storage.
    /// Access Pattern: Read before transfers; written by admin to freeze/unfreeze accounts.
    /// TTL Policy: Explicitly bumped to enforce compliance rules.
    Frozen(soroban_sdk::Address),
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
    /// If true, transfers restricted until settlement (admin can still transfer).
    pub transfer_locked: bool,
    /// Emergency pause flag for sensitive token operations.
    pub paused: bool,
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
