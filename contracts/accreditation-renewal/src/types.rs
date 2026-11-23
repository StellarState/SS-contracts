use soroban_sdk::{contracttype, Address};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReminderRecord {
    pub interval_days: u32,
    pub timestamp: u64,
    pub expiry_timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Operator,
    WalletList,
    Expiry(Address),
    // Retained so upgraded instances can identify and remove pre-cycle logs.
    ReminderLog(Address),
    ReminderLogForExpiry(Address, u64),
}
