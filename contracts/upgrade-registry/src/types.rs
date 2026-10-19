use soroban_sdk::{contracttype, Address, Symbol};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    Contract(Symbol),
    History(Symbol),
}

/// Information about the current active contract deployment.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractEntry {
    pub address: Address,
    pub version: u32,
}

/// Historical record of a contract deployment or upgrade.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeRecord {
    pub address: Address,
    pub version: u32,
    pub timestamp: u64,
}
