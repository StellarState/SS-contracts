use soroban_sdk::{contracttype, Address, BytesN};

pub const MIN_RATINGS_FOR_ADJUSTMENT: u32 = 5;
pub const BONUS_BPS: i32 = 25;       // +0.25%
pub const NEUTRAL_BPS: i32 = 0;      // 0.00%
pub const DISCOUNT_BPS: i32 = -10;   // -0.10%

pub const RATING_HIGH_X100: u32 = 450; // 4.50
pub const RATING_MID_X100: u32 = 400;  // 4.00

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Admin,
    RatingData(BytesN<32>),
    UserRating(BytesN<32>, Address),
}

/// Rating statistics for an invoice.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceRatingData {
    pub total_ratings: u32,
    pub score_sum: u32,
}

/// Yield adjustment details and basis rating.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YieldAdjustmentInfo {
    pub adjustment_bps: i32,
    pub basis_rating_x100: u32,
    pub total_ratings: u32,
}
