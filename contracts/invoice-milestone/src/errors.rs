use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidAmount = 4,
    InvoiceNotFound = 5,
    InvalidIndex = 6,
    AlreadyVerified = 7,
    MilestonesAlreadySet = 8,
    InvalidExpiry = 9,
    EvidenceExpired = 10,
    EvidenceNotExpired = 11,
    EvidenceAlreadyReported = 12,
    InvalidEvidenceHash = 13,
}
