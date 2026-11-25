use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidTrancheConfig = 4,
    InvalidTrancheType = 5,
    InvalidAmount = 6,
    TrancheAlreadyExists = 7,
    TrancheNotFound = 8,
    AlreadySettled = 9,
    QuotaExceeded = 10,
}
