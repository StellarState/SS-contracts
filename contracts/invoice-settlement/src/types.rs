use soroban_sdk::{contracterror, contracttype, Address, BytesN, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    Treasury,
    FeeBps,
    Invoice(BytesN<32>),
    PayoutDone(BytesN<32>),
    EvidenceUsed(BytesN<32>),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HolderShare {
    pub wallet: Address,
    pub fractions: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettlementInvoice {
    pub invoice_id: BytesN<32>,
    pub issuer: Address,
    pub token: Address,
    pub holders: Vec<HolderShare>,
    pub total_fractions: i128,
    pub outstanding_balance: i128,
    pub repaid_amount: i128,
    pub distributed: bool,
    pub payer: Option<Address>,
    pub settlement_evidence_hash: Option<BytesN<32>>,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidAmount = 4,
    InvalidFeeBps = 5,
    InvoiceExists = 6,
    InvoiceNotFound = 7,
    AlreadyRepaid = 8,
    NothingToDistribute = 9,
    AlreadyDistributed = 10,
    HolderNotFound = 11,
    Overflow = 12,
    InvalidHolders = 13,
    InvalidEvidence = 14,
    EvidenceAlreadyUsed = 15,
    EvidenceAlreadySet = 16,
    EvidenceMismatch = 17,
}
