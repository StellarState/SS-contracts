#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, Vec};
pub use types::{AggregateRating, InvoiceInfo, RatingStats, StorageKey};
use types::{RatingStatsV2, MAX_SCORE, MIN_RATINGS, MIN_SCORE};

#[contract]
pub struct InvoiceRating;

fn require_operator(env: &Env, caller: &Address) -> Result<(), Error> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&StorageKey::Admin)
        .ok_or(Error::NotInitialized)?;
    let operator: Address = env
        .storage()
        .instance()
        .get(&StorageKey::Operator)
        .ok_or(Error::NotInitialized)?;
    if *caller != admin && *caller != operator {
        return Err(Error::Unauthorized);
    }
    caller.require_auth();
    Ok(())
}

fn get_invoice_info(env: &Env, invoice_id: &BytesN<32>) -> Result<InvoiceInfo, Error> {
    env.storage()
        .persistent()
        .get(&StorageKey::Invoice(invoice_id.clone()))
        .ok_or(Error::InvoiceNotFound)
}

fn empty_stats(env: &Env) -> RatingStatsV2 {
    let mut distribution = Vec::new(env);
    for _ in 0..MAX_SCORE {
        distribution.push_back(0u32);
    }
    RatingStatsV2 {
        total_ratings: 0,
        score_sum: 0,
        distribution,
    }
}

fn load_stats(env: &Env, v2_key: &StorageKey, legacy_key: &StorageKey) -> RatingStatsV2 {
    if let Some(stats) = env.storage().persistent().get(v2_key) {
        return stats;
    }

    if let Some(legacy) = env.storage().persistent().get::<_, RatingStats>(legacy_key) {
        return RatingStatsV2 {
            total_ratings: legacy.total_ratings,
            score_sum: u64::from(legacy.score_sum),
            distribution: legacy.distribution,
        };
    }

    empty_stats(env)
}

fn add_score(
    env: &Env,
    v2_key: &StorageKey,
    legacy_key: &StorageKey,
    score: u32,
) -> Result<RatingStatsV2, Error> {
    let mut stats = load_stats(env, v2_key, legacy_key);
    stats.total_ratings = stats
        .total_ratings
        .checked_add(1)
        .ok_or(Error::CounterOverflow)?;
    stats.score_sum = stats
        .score_sum
        .checked_add(u64::from(score))
        .ok_or(Error::CounterOverflow)?;
    let idx = score - MIN_SCORE;
    let count = stats.distribution.get(idx).unwrap();
    stats
        .distribution
        .set(idx, count.checked_add(1).ok_or(Error::CounterOverflow)?);
    Ok(stats)
}

fn to_aggregate(env: &Env, v2_key: &StorageKey, legacy_key: &StorageKey) -> AggregateRating {
    let stats = load_stats(env, v2_key, legacy_key);
    let average_x100 = if stats.total_ratings == 0 {
        0
    } else {
        ((u128::from(stats.score_sum) * 100) / u128::from(stats.total_ratings)) as u32
    };
    AggregateRating {
        average_x100,
        total_ratings: stats.total_ratings,
        distribution: stats.distribution,
        valid: stats.total_ratings >= MIN_RATINGS,
    }
}

#[contractimpl]
impl InvoiceRating {
    pub fn initialize(env: Env, admin: Address, operator: Address) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&StorageKey::Operator, &operator);
        Ok(())
    }

    /// Admin/operator: register an invoice and its issuer so it can be rated.
    pub fn register_invoice(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
        issuer: Address,
    ) -> Result<(), Error> {
        require_operator(&env, &caller)?;
        let key = StorageKey::Invoice(invoice_id);
        if env.storage().persistent().has(&key) {
            return Err(Error::InvoiceAlreadyRegistered);
        }
        env.storage().persistent().set(
            &key,
            &InvoiceInfo {
                issuer,
                settled: false,
            },
        );
        Ok(())
    }

    /// Admin/operator: record that the invoice has been settled, opening it for ratings.
    pub fn mark_settled(env: Env, caller: Address, invoice_id: BytesN<32>) -> Result<(), Error> {
        require_operator(&env, &caller)?;
        let mut info = get_invoice_info(&env, &invoice_id)?;
        info.settled = true;
        env.storage()
            .persistent()
            .set(&StorageKey::Invoice(invoice_id), &info);
        Ok(())
    }

    /// Admin/operator: record whether a wallet held fractions of the invoice.
    pub fn set_fraction_holder(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
        wallet: Address,
        is_holder: bool,
    ) -> Result<(), Error> {
        require_operator(&env, &caller)?;
        get_invoice_info(&env, &invoice_id)?;
        env.storage()
            .persistent()
            .set(&StorageKey::Holder(invoice_id, wallet), &is_holder);
        Ok(())
    }

    /// Fraction holder only: rate a settled invoice with a score from 1 to 5.
    pub fn submit_rating(
        env: Env,
        wallet: Address,
        invoice_id: BytesN<32>,
        score: u32,
    ) -> Result<(), Error> {
        wallet.require_auth();

        if !(MIN_SCORE..=MAX_SCORE).contains(&score) {
            return Err(Error::InvalidScore);
        }
        let info = get_invoice_info(&env, &invoice_id)?;
        if !info.settled {
            return Err(Error::InvoiceNotSettled);
        }
        let is_holder: bool = env
            .storage()
            .persistent()
            .get(&StorageKey::Holder(invoice_id.clone(), wallet.clone()))
            .unwrap_or(false);
        if !is_holder {
            return Err(Error::NotFractionHolder);
        }
        let rating_key = StorageKey::Rating(invoice_id.clone(), wallet.clone());
        if env.storage().persistent().has(&rating_key) {
            return Err(Error::AlreadyRated);
        }

        let invoice_stats = add_score(
            &env,
            &StorageKey::InvoiceStatsV2(invoice_id.clone()),
            &StorageKey::InvoiceStats(invoice_id.clone()),
            score,
        )?;
        let issuer_stats = add_score(
            &env,
            &StorageKey::IssuerStatsV2(info.issuer.clone()),
            &StorageKey::IssuerStats(info.issuer.clone()),
            score,
        )?;

        env.storage().persistent().set(&rating_key, &score);
        env.storage().persistent().set(
            &StorageKey::InvoiceStatsV2(invoice_id.clone()),
            &invoice_stats,
        );
        env.storage()
            .persistent()
            .set(&StorageKey::IssuerStatsV2(info.issuer), &issuer_stats);

        events::rating_submitted(&env, &invoice_id, &wallet, score);
        Ok(())
    }

    /// View: aggregate rating for an invoice (`valid` once 3 ratings exist).
    pub fn get_aggregate_rating(env: Env, invoice_id: BytesN<32>) -> AggregateRating {
        to_aggregate(
            &env,
            &StorageKey::InvoiceStatsV2(invoice_id.clone()),
            &StorageKey::InvoiceStats(invoice_id),
        )
    }

    /// View: aggregate rating across all of an issuer's invoices.
    pub fn get_issuer_rating(env: Env, issuer_wallet: Address) -> AggregateRating {
        to_aggregate(
            &env,
            &StorageKey::IssuerStatsV2(issuer_wallet.clone()),
            &StorageKey::IssuerStats(issuer_wallet),
        )
    }
}
