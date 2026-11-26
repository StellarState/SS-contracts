use soroban_sdk::{contracttype, Address};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollateralRecord {
    /// Timestamp when the current over-collateralisation streak started.
    pub streak_start: u64,
    pub is_overcollateral: bool,
    pub excess_bps: u32,
    pub discount_pending: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewardStatus {
    pub streak_days: u32,
    pub discount_pending: bool,
    pub discount_consumed: bool,
    pub discount_eligible: bool,
    pub discount_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    Record(Address),
    QualificationCycle(Address),
    RewardedCycle(Address),
}
