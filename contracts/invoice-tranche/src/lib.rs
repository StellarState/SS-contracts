#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
pub use types::{DataKey, TrancheConfig, TrancheStatus, TrancheType};

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env};

const BPS_DENOMINATOR: i128 = 10_000;

#[contract]
pub struct InvoiceTrancheContract;

#[contractimpl]
impl InvoiceTrancheContract {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    pub fn create_tranches(
        env: Env,
        issuer: Address,
        invoice_id: BytesN<32>,
        target_amount: i128,
        senior_pct: u32,
        junior_yield_premium_bps: u32,
    ) -> Result<(), Error> {
        issuer.require_auth();

        if target_amount <= 0 || senior_pct == 0 || senior_pct >= 100 {
            return Err(Error::InvalidTrancheConfig);
        }

        let senior_capacity = target_amount
            .checked_div(100)
            .and_then(|whole| whole.checked_mul(senior_pct as i128))
            .and_then(|whole| {
                let remainder = (target_amount % 100).checked_mul(senior_pct as i128)?;
                whole.checked_add(remainder / 100)
            })
            .ok_or(Error::InvalidTrancheConfig)?;
        let junior_capacity = target_amount
            .checked_sub(senior_capacity)
            .ok_or(Error::InvalidTrancheConfig)?;

        let tranche_key = DataKey::Tranche(invoice_id.clone());
        if env.storage().persistent().has(&tranche_key) {
            return Err(Error::TrancheAlreadyExists);
        }

        let config = TrancheConfig {
            issuer,
            target_amount,
            senior_pct,
            senior_capacity,
            junior_capacity,
            junior_yield_premium_bps,
            senior_filled: 0,
            junior_filled: 0,
            is_settled: false,
            senior_payout: 0,
            junior_payout: 0,
        };

        env.storage().persistent().set(&tranche_key, &config);
        Ok(())
    }

    pub fn invest_tranche(
        env: Env,
        investor: Address,
        invoice_id: BytesN<32>,
        tranche_type: u32,
        amount: i128,
    ) -> Result<(), Error> {
        investor.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        if tranche_type != TrancheType::Senior as u32 && tranche_type != TrancheType::Junior as u32
        {
            return Err(Error::InvalidTrancheType);
        }

        let tranche_key = DataKey::Tranche(invoice_id.clone());
        let mut config: TrancheConfig = env
            .storage()
            .persistent()
            .get(&tranche_key)
            .ok_or(Error::TrancheNotFound)?;

        if config.is_settled {
            return Err(Error::AlreadySettled);
        }

        if tranche_type == TrancheType::Senior as u32 {
            let new_filled = config
                .senior_filled
                .checked_add(amount)
                .ok_or(Error::InvalidAmount)?;
            if new_filled > config.senior_capacity {
                return Err(Error::QuotaExceeded);
            }
            config.senior_filled = new_filled;
        } else {
            let new_filled = config
                .junior_filled
                .checked_add(amount)
                .ok_or(Error::InvalidAmount)?;
            if new_filled > config.junior_capacity {
                return Err(Error::QuotaExceeded);
            }
            config.junior_filled = new_filled;
        }

        let inv_key = DataKey::Investment(invoice_id.clone(), investor.clone(), tranche_type);
        let previous: i128 = env.storage().persistent().get(&inv_key).unwrap_or(0);
        let new_position = previous.checked_add(amount).ok_or(Error::InvalidAmount)?;

        env.storage().persistent().set(&tranche_key, &config);
        env.storage().persistent().set(&inv_key, &new_position);

        events::tranche_investment_recorded(&env, &invoice_id, &investor, tranche_type, amount);

        Ok(())
    }

    pub fn settle_tranches(
        env: Env,
        invoice_id: BytesN<32>,
        settlement_amount: i128,
    ) -> Result<(i128, i128), Error> {
        let tranche_key = DataKey::Tranche(invoice_id.clone());
        let mut config: TrancheConfig = env
            .storage()
            .persistent()
            .get(&tranche_key)
            .ok_or(Error::TrancheNotFound)?;

        config.issuer.require_auth();

        if config.is_settled {
            return Err(Error::AlreadySettled);
        }

        if settlement_amount < 0 {
            return Err(Error::InvalidAmount);
        }

        // Senior tranche repaid first
        let senior_payout = if settlement_amount >= config.senior_filled {
            config.senior_filled
        } else {
            settlement_amount
        };

        let mut remainder = settlement_amount - senior_payout;

        // Junior tranche receives remainder with yield premium applied if applicable
        let junior_payout = if remainder > 0 {
            if remainder >= config.junior_filled {
                let surplus = remainder - config.junior_filled;
                let bonus = (surplus * (config.junior_yield_premium_bps as i128)) / BPS_DENOMINATOR;
                config.junior_filled + bonus.min(surplus)
            } else {
                remainder
            }
        } else {
            0
        };

        config.senior_payout = senior_payout;
        config.junior_payout = junior_payout;
        config.is_settled = true;

        env.storage().persistent().set(&tranche_key, &config);

        events::tranche_settled(&env, &invoice_id, senior_payout, junior_payout);

        Ok((senior_payout, junior_payout))
    }

    pub fn get_tranche_status(env: Env, invoice_id: BytesN<32>) -> Result<TrancheStatus, Error> {
        let tranche_key = DataKey::Tranche(invoice_id);
        let config: TrancheConfig = env
            .storage()
            .persistent()
            .get(&tranche_key)
            .ok_or(Error::TrancheNotFound)?;

        let junior_pct = 100 - config.senior_pct;

        Ok(TrancheStatus {
            target_amount: config.target_amount,
            senior_pct: config.senior_pct,
            junior_pct,
            senior_capacity: config.senior_capacity,
            junior_capacity: config.junior_capacity,
            senior_filled: config.senior_filled,
            junior_filled: config.junior_filled,
            junior_yield_premium_bps: config.junior_yield_premium_bps,
            is_settled: config.is_settled,
        })
    }

    pub fn get_investor_tranche_amount(
        env: Env,
        invoice_id: BytesN<32>,
        investor: Address,
        tranche_type: u32,
    ) -> i128 {
        let inv_key = DataKey::Investment(invoice_id, investor, tranche_type);
        env.storage().persistent().get(&inv_key).unwrap_or(0)
    }
}
