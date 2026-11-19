use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidAmount = 3,
    InvalidCoverage = 4,
    PolicyAlreadyExists = 5,
    PayoutAlreadyTriggered = 6,
    InsufficientPool = 7,
    Overflow = 8,
    InvalidLoss = 9,
    /// No trusted position registry has been configured.
    RegistryNotSet = 10,
    /// The investor has no eligible principal for this invoice, or supplied
    /// principal exceeds the authoritative eligible amount.
    InvalidPosition = 11,
    /// The configured position registry could not be queried.
    RegistryCallFailed = 12,
}
