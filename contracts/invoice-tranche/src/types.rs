use soroban_sdk::{contracttype, Address, BytesN};

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum TrancheType {
    Senior = 1,
    Junior = 2,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrancheConfig {
    pub issuer: Address,
    pub target_amount: i128,
    pub senior_pct: u32,
    pub senior_capacity: i128,
    pub junior_capacity: i128,
    pub junior_yield_premium_bps: u32,
    pub senior_filled: i128,
    pub junior_filled: i128,
    pub is_settled: bool,
    pub senior_payout: i128,
    pub junior_payout: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrancheStatus {
    pub target_amount: i128,
    pub senior_pct: u32,
    pub junior_pct: u32,
    pub senior_capacity: i128,
    pub junior_capacity: i128,
    pub senior_filled: i128,
    pub junior_filled: i128,
    pub junior_yield_premium_bps: u32,
    pub is_settled: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Tranche(BytesN<32>),
    Investment(BytesN<32>, Address, u32),
}
