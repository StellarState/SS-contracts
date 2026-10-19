use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    BatchTooLarge = 3,
    EmptyBatch = 4,
    OrderNotFound = 5,
    Unauthorized = 6,
    OrderNotOpen = 7,
}
