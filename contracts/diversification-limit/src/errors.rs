use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    InvalidAmount = 1,
    InvalidLimit = 2,
    DiversificationLimitExceeded = 3,
    ArithmeticOverflow = 4,
}
