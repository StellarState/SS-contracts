#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, Vec};
pub use types::{
    DiversificationCheck, InvoiceConcentration, StorageKey, DEFAULT_LIMIT_BPS, MAX_BPS,
};

#[contract]
pub struct DiversificationLimit;

fn limit(env: &Env, wallet: &Address) -> u32 {
    env.storage()
        .persistent()
        .get(&StorageKey::Limit(wallet.clone()))
        .unwrap_or(DEFAULT_LIMIT_BPS)
}

fn position(env: &Env, wallet: &Address, invoice_id: &BytesN<32>) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::Position(wallet.clone(), invoice_id.clone()))
        .unwrap_or(0)
}

fn total(env: &Env, wallet: &Address) -> i128 {
    let invested_total: i128 = env
        .storage()
        .persistent()
        .get(&StorageKey::Total(wallet.clone()))
        .unwrap_or(0);
    let base_total: i128 = env
        .storage()
        .persistent()
        .get(&StorageKey::PortfolioBase(wallet.clone()))
        .unwrap_or(0);
    invested_total.max(base_total)
}

#[contractimpl]
impl DiversificationLimit {
    /// Configures the diversification limit in basis points for the calling wallet.
    /// Default is 3000 bps (30%).
    pub fn set_diversification_limit(
        env: Env,
        wallet: Address,
        limit_bps: u32,
    ) -> Result<(), Error> {
        wallet.require_auth();
        if limit_bps == 0 || limit_bps > MAX_BPS {
            return Err(Error::InvalidLimit);
        }
        env.storage()
            .persistent()
            .set(&StorageKey::Limit(wallet.clone()), &limit_bps);
        events::diversification_limit_set(&env, &wallet, limit_bps);
        Ok(())
    }

    /// Returns the diversification limit in basis points for `wallet`.
    pub fn get_diversification_limit(env: Env, wallet: Address) -> u32 {
        limit(&env, &wallet)
    }

    /// Sets base total portfolio capital for `wallet` (e.g. deposited portfolio capital).
    pub fn set_portfolio_total(
        env: Env,
        wallet: Address,
        total_amount: i128,
    ) -> Result<(), Error> {
        wallet.require_auth();
        if total_amount < 0 {
            return Err(Error::InvalidAmount);
        }
        env.storage()
            .persistent()
            .set(&StorageKey::PortfolioBase(wallet), &total_amount);
        Ok(())
    }

    /// Returns total portfolio value for `wallet`.
    pub fn get_portfolio_total(env: Env, wallet: Address) -> i128 {
        total(&env, &wallet)
    }

    /// Returns current position in `invoice_id` for `wallet`.
    pub fn get_position(env: Env, wallet: Address, invoice_id: BytesN<32>) -> i128 {
        position(&env, &wallet, &invoice_id)
    }

    /// Checks if a proposed investment of `new_amount` into `invoice_id` respects the wallet's
    /// diversification limit. Returns eligibility and breach details.
    pub fn check_diversification(
        env: Env,
        wallet: Address,
        invoice_id: BytesN<32>,
        new_amount: i128,
    ) -> Result<DiversificationCheck, Error> {
        if new_amount < 0 {
            return Err(Error::InvalidAmount);
        }

        let current_pos = position(&env, &wallet, &invoice_id);
        let current_total = total(&env, &wallet);
        let max_bps = limit(&env, &wallet);

        let current_bps = if current_total == 0 {
            0
        } else {
            ((current_pos
                .checked_mul(MAX_BPS as i128)
                .ok_or(Error::ArithmeticOverflow)?)
                / current_total) as u32
        };

        let projected_pos = current_pos
            .checked_add(new_amount)
            .ok_or(Error::ArithmeticOverflow)?;
        let projected_total = current_total
            .checked_add(new_amount)
            .ok_or(Error::ArithmeticOverflow)?;

        if projected_total == 0 {
            return Ok(DiversificationCheck {
                eligible: true,
                current_bps,
                projected_bps: 0,
                max_bps,
                breach: false,
            });
        }

        let projected_bps = ((projected_pos
            .checked_mul(MAX_BPS as i128)
            .ok_or(Error::ArithmeticOverflow)?)
            / projected_total) as u32;

        let eligible = projected_bps <= max_bps;
        let breach = !eligible;

        Ok(DiversificationCheck {
            eligible,
            current_bps,
            projected_bps,
            max_bps,
            breach,
        })
    }

    /// Invests `amount` into `invoice_id`. Calls `check_diversification` before accepting funds.
    /// Emits `DiversificationLimitExceeded` and returns an error if limit is breached.
    pub fn invest(
        env: Env,
        wallet: Address,
        invoice_id: BytesN<32>,
        amount: i128,
    ) -> Result<(), Error> {
        wallet.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let check = Self::check_diversification(
            env.clone(),
            wallet.clone(),
            invoice_id.clone(),
            amount,
        )?;

        if !check.eligible {
            events::diversification_limit_exceeded(
                &env,
                &wallet,
                &invoice_id,
                check.current_bps,
                check.projected_bps,
                check.max_bps,
            );
            return Err(Error::DiversificationLimitExceeded);
        }

        let next_pos = position(&env, &wallet, &invoice_id)
            .checked_add(amount)
            .ok_or(Error::ArithmeticOverflow)?;
        let invested_total: i128 = env
            .storage()
            .persistent()
            .get(&StorageKey::Total(wallet.clone()))
            .unwrap_or(0);
        let next_total = invested_total
            .checked_add(amount)
            .ok_or(Error::ArithmeticOverflow)?;

        env.storage().persistent().set(
            &StorageKey::Position(wallet.clone(), invoice_id.clone()),
            &next_pos,
        );
        env.storage()
            .persistent()
            .set(&StorageKey::Total(wallet.clone()), &next_total);

        // Track invoice in list for get_portfolio_concentration
        let mut invoices: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&StorageKey::Invoices(wallet.clone()))
            .unwrap_or(Vec::new(&env));
        let mut found = false;
        for inv in invoices.iter() {
            if inv == invoice_id {
                found = true;
                break;
            }
        }
        if !found {
            invoices.push_back(invoice_id.clone());
            env.storage()
                .persistent()
                .set(&StorageKey::Invoices(wallet.clone()), &invoices);
        }

        events::invested(&env, &wallet, &invoice_id, amount, check.projected_bps);
        Ok(())
    }

    /// Returns per-invoice portfolio concentration percentages for `wallet`.
    pub fn get_portfolio_concentration(
        env: Env,
        wallet: Address,
    ) -> Result<Vec<InvoiceConcentration>, Error> {
        let total_val = total(&env, &wallet);
        let invoices: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&StorageKey::Invoices(wallet.clone()))
            .unwrap_or(Vec::new(&env));

        let mut concentrations = Vec::new(&env);
        for inv in invoices.iter() {
            let amt = position(&env, &wallet, &inv);
            let bps = if total_val == 0 {
                0
            } else {
                ((amt
                    .checked_mul(MAX_BPS as i128)
                    .ok_or(Error::ArithmeticOverflow)?)
                    / total_val) as u32
            };
            concentrations.push_back(InvoiceConcentration {
                invoice_id: inv,
                amount: amt,
                concentration_bps: bps,
            });
        }
        Ok(concentrations)
    }
}
