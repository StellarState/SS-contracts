use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    IssuerNotFound = 4,
    NotOvercollateral = 5,
    StreakTooShort = 6,
    DiscountAlreadyPending = 7,
    DiscountAlreadyRewarded = 8,
}
