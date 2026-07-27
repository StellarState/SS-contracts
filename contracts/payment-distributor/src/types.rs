use soroban_sdk::contracttype;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    Distribution(soroban_sdk::Address, soroban_sdk::Symbol),
    FeeTiers,
    InvestorBonusBps,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DistributionState {
    pub paid_distributed: i128,
    pub refund_distributed: bool,
}

/// A platform fee tier applying `fee_bps` (basis points, 0-10000) to payment
/// amounts within the inclusive range [`min_amount`, `max_amount`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeTier {
    pub min_amount: i128,
    pub max_amount: i128,
    pub fee_bps: u32,
}
