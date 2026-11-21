use soroban_sdk::{contracttype, BytesN};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    pub amount: i128,
    pub description_hash: BytesN<32>,
    pub evidence_expiry_ledger: u32,
    pub verified: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneStatus {
    pub index: u32,
    pub amount: i128,
    pub description_hash: BytesN<32>,
    pub evidence_expiry_ledger: u32,
    pub evidence_valid: bool,
    pub remaining_ledger_life: u32,
    pub verified: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    PaymentToken,
    Milestones(BytesN<32>),
    Issuer(BytesN<32>),
    ExpiryReported(BytesN<32>, u32),
}
