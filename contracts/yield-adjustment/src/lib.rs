#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env};
pub use types::{
    InvoiceRatingData, StorageKey, YieldAdjustmentInfo, BONUS_BPS, DISCOUNT_BPS,
    MIN_RATINGS_FOR_ADJUSTMENT, NEUTRAL_BPS, RATING_HIGH_X100, RATING_MID_X100,
};

#[contract]
pub struct YieldAdjustment;

fn require_admin(env: &Env) -> Result<Address, Error> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&StorageKey::Admin)
        .ok_or(Error::NotInitialized)?;
    admin.require_auth();
    Ok(admin)
}

fn get_rating_data(env: &Env, invoice_id: &BytesN<32>) -> InvoiceRatingData {
    env.storage()
        .persistent()
        .get(&StorageKey::RatingData(invoice_id.clone()))
        .unwrap_or(InvoiceRatingData {
            total_ratings: 0,
            score_sum: 0,
        })
}

#[contractimpl]
impl YieldAdjustment {
    /// Initialize the contract with an administrator.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&StorageKey::Admin, &admin);
        Ok(())
    }

    /// Admin/Oracle hook to set aggregate rating directly for an invoice.
    /// `average_x100` is the rating scaled by 100 (e.g. 450 = 4.5, 380 = 3.8).
    pub fn set_invoice_rating(
        env: Env,
        invoice_id: BytesN<32>,
        average_x100: u32,
        total_ratings: u32,
    ) -> Result<(), Error> {
        require_admin(&env)?;
        if average_x100 > 500 {
            return Err(Error::InvalidScore);
        }
        let score_sum = average_x100
            .checked_mul(total_ratings)
            .ok_or(Error::ArithmeticOverflow)?;

        env.storage().persistent().set(
            &StorageKey::RatingData(invoice_id.clone()),
            &InvoiceRatingData {
                total_ratings,
                score_sum,
            },
        );
        events::rating_recorded(&env, &invoice_id, average_x100, total_ratings);
        Ok(())
    }

    /// Records an individual investor rating (score between 1 and 5, or 100 and 500).
    pub fn record_rating(
        env: Env,
        user: Address,
        invoice_id: BytesN<32>,
        score: u32,
    ) -> Result<(), Error> {
        user.require_auth();

        // Support either 1-5 scale or 100-500 scale
        let score_x100 = if (1..=5).contains(&score) {
            score * 100
        } else if (100..=500).contains(&score) {
            score
        } else {
            return Err(Error::InvalidScore);
        };

        let mut data = get_rating_data(&env, &invoice_id);

        // Check if user has already rated; if so, replace prior rating
        let key = StorageKey::UserRating(invoice_id.clone(), user.clone());
        if let Some(prev_score) = env.storage().persistent().get::<_, u32>(&key) {
            data.score_sum = data
                .score_sum
                .saturating_sub(prev_score)
                .checked_add(score_x100)
                .ok_or(Error::ArithmeticOverflow)?;
        } else {
            data.total_ratings = data
                .total_ratings
                .checked_add(1)
                .ok_or(Error::ArithmeticOverflow)?;
            data.score_sum = data
                .score_sum
                .checked_add(score_x100)
                .ok_or(Error::ArithmeticOverflow)?;
        }

        env.storage().persistent().set(&key, &score_x100);
        env.storage()
            .persistent()
            .set(&StorageKey::RatingData(invoice_id.clone()), &data);

        let avg = if data.total_ratings > 0 {
            data.score_sum / data.total_ratings
        } else {
            0
        };
        events::rating_recorded(&env, &invoice_id, avg, data.total_ratings);
        Ok(())
    }

    /// Computes yield adjustment in basis points from the aggregate rating.
    /// - Rating >= 4.5: +0.25% (+25 bps)
    /// - Rating 4.0 - 4.5: 0.00% (0 bps)
    /// - Rating < 4.0: -0.10% (-10 bps)
    /// - Fewer than 5 ratings: defaults to 0 bps
    pub fn compute_yield_adjustment(env: Env, invoice_id: BytesN<32>) -> i32 {
        let data = get_rating_data(&env, &invoice_id);
        if data.total_ratings < MIN_RATINGS_FOR_ADJUSTMENT {
            return NEUTRAL_BPS;
        }

        let avg_x100 = data.score_sum / data.total_ratings;
        if avg_x100 >= RATING_HIGH_X100 {
            BONUS_BPS
        } else if avg_x100 >= RATING_MID_X100 {
            NEUTRAL_BPS
        } else {
            DISCOUNT_BPS
        }
    }

    /// Returns the current yield adjustment and basis rating for `invoice_id`.
    pub fn get_yield_adjustment(env: Env, invoice_id: BytesN<32>) -> YieldAdjustmentInfo {
        let data = get_rating_data(&env, &invoice_id);
        let basis_rating_x100 = if data.total_ratings > 0 {
            data.score_sum / data.total_ratings
        } else {
            0
        };

        let adjustment_bps = Self::compute_yield_adjustment(env, invoice_id);

        YieldAdjustmentInfo {
            adjustment_bps,
            basis_rating_x100,
            total_ratings: data.total_ratings,
        }
    }

    /// Settlement contract hook: applies the rating-weighted yield adjustment to a base yield.
    pub fn apply_settlement_adjustment(
        env: Env,
        invoice_id: BytesN<32>,
        base_yield_bps: i128,
    ) -> Result<i128, Error> {
        let adjustment_bps = Self::compute_yield_adjustment(env.clone(), invoice_id.clone());
        let final_yield_bps = base_yield_bps
            .checked_add(adjustment_bps as i128)
            .ok_or(Error::ArithmeticOverflow)?
            .max(0);

        events::yield_adjusted_at_settlement(
            &env,
            &invoice_id,
            base_yield_bps,
            adjustment_bps,
            final_yield_bps,
        );

        Ok(final_yield_bps)
    }
}
