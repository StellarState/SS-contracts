#![no_std]

mod errors;
mod events;
mod types;

pub use errors::Error;
pub use types::{DataKey, NavRecord, NavVerification, OracleInputs};

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, Symbol, Vec};

const SECONDS_PER_DAY: u64 = 86400;

/// Default maximum accepted age of NAV oracle inputs (1 hour).
const DEFAULT_ORACLE_MAX_AGE_SECS: u64 = 3600;

#[contract]
pub struct PortfolioNavSnapshotContract;

#[contractimpl]
impl PortfolioNavSnapshotContract {
    pub fn initialize(env: Env, admin: Address, operator: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Operator, &operator);
        Ok(())
    }

    pub fn set_invoice_price(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
        price: i128,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        let operator: Address = env
            .storage()
            .instance()
            .get(&DataKey::Operator)
            .ok_or(Error::NotInitialized)?;

        if caller != admin && caller != operator {
            return Err(Error::Unauthorized);
        }
        caller.require_auth();

        if price < 0 {
            return Err(Error::InvalidAmount);
        }

        env.storage()
            .persistent()
            .set(&DataKey::InvoicePrice(invoice_id), &price);
        Ok(())
    }

    pub fn set_investor_balance(
        env: Env,
        caller: Address,
        wallet: Address,
        invoice_id: BytesN<32>,
        balance: i128,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        let operator: Address = env
            .storage()
            .instance()
            .get(&DataKey::Operator)
            .ok_or(Error::NotInitialized)?;

        if caller != admin && caller != operator {
            return Err(Error::Unauthorized);
        }
        caller.require_auth();

        if balance < 0 {
            return Err(Error::InvalidAmount);
        }

        let held_key = DataKey::HeldInvoices(wallet.clone());
        let mut held: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&held_key)
            .unwrap_or(Vec::new(&env));

        let mut exists = false;
        for id in held.iter() {
            if id == invoice_id {
                exists = true;
                break;
            }
        }
        if !exists {
            held.push_back(invoice_id.clone());
            env.storage().persistent().set(&held_key, &held);
        }

        env.storage()
            .persistent()
            .set(&DataKey::FractionBalance(wallet, invoice_id), &balance);
        Ok(())
    }

    pub fn take_portfolio_snapshot(env: Env, wallet: Address) -> Result<i128, Error> {
        let operator: Address = env
            .storage()
            .instance()
            .get(&DataKey::Operator)
            .ok_or(Error::NotInitialized)?;
        operator.require_auth();

        let now = env.ledger().timestamp();
        let date = (now / SECONDS_PER_DAY) * SECONDS_PER_DAY;

        let snapshot_key = DataKey::Snapshot(wallet.clone(), date);
        if env.storage().persistent().has(&snapshot_key) {
            return Err(Error::DuplicateSnapshot);
        }

        let held_key = DataKey::HeldInvoices(wallet.clone());
        let held: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&held_key)
            .unwrap_or(Vec::new(&env));

        let mut total_nav: i128 = 0;
        for invoice_id in held.iter() {
            let balance: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::FractionBalance(
                    wallet.clone(),
                    invoice_id.clone(),
                ))
                .unwrap_or(0);
            let price: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::InvoicePrice(invoice_id))
                .unwrap_or(0);

            let value = balance.checked_mul(price).unwrap_or(0);
            total_nav = total_nav.checked_add(value).unwrap_or(total_nav);
        }

        env.storage().persistent().set(&snapshot_key, &total_nav);

        let dates_key = DataKey::SnapshotDates(wallet.clone());
        let mut dates: Vec<u64> = env
            .storage()
            .persistent()
            .get(&dates_key)
            .unwrap_or(Vec::new(&env));
        dates.push_back(date);
        env.storage().persistent().set(&dates_key, &dates);

        events::portfolio_snapshot_taken(&env, &wallet, date, total_nav);

        Ok(total_nav)
    }

    pub fn get_portfolio_nav(env: Env, wallet: Address, date: u64) -> Result<i128, Error> {
        let snapshot_key = DataKey::Snapshot(wallet, date);
        env.storage()
            .persistent()
            .get(&snapshot_key)
            .ok_or(Error::SnapshotNotFound)
    }

    pub fn get_portfolio_nav_range(
        env: Env,
        wallet: Address,
        from_date: u64,
        to_date: u64,
    ) -> Result<Vec<NavRecord>, Error> {
        if from_date > to_date {
            return Err(Error::InvalidDateRange);
        }

        let dates_key = DataKey::SnapshotDates(wallet.clone());
        let dates: Vec<u64> = env
            .storage()
            .persistent()
            .get(&dates_key)
            .unwrap_or(Vec::new(&env));

        let mut records = Vec::new(&env);
        for d in dates.iter() {
            if d >= from_date && d <= to_date {
                if let Some(nav) = env
                    .storage()
                    .persistent()
                    .get::<DataKey, i128>(&DataKey::Snapshot(wallet.clone(), d))
                {
                    records.push_back(NavRecord {
                        date: d,
                        nav_amount: nav,
                    });
                }
            }
        }

        Ok(records)
    }

    /// Admin/operator-only: report the outstanding supply and reserve
    /// snapshot components used to verify NAV snapshots (#817). The ledger
    /// timestamp is recorded automatically; inputs older than
    /// `get_oracle_max_age` are rejected as stale by `verify_nav_snapshot`.
    pub fn set_nav_oracle_inputs(
        env: Env,
        caller: Address,
        outstanding_supply: i128,
        reserve: i128,
    ) -> Result<(), Error> {
        ensure_admin_or_operator(&env, &caller)?;
        caller.require_auth();
        if outstanding_supply <= 0 || reserve < 0 {
            return Err(Error::InvalidAmount);
        }
        let inputs = OracleInputs {
            outstanding_supply,
            reserve,
            timestamp: env.ledger().timestamp(),
        };
        env.storage().instance().set(&DataKey::OracleInputs, &inputs);
        Ok(())
    }

    /// Admin-only: set the maximum accepted age (seconds) of NAV oracle
    /// inputs. Must be greater than zero.
    pub fn set_oracle_max_age(env: Env, caller: Address, max_age_secs: u64) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        if caller != admin {
            return Err(Error::Unauthorized);
        }
        caller.require_auth();
        if max_age_secs == 0 {
            return Err(Error::InvalidAmount);
        }
        env.storage()
            .instance()
            .set(&DataKey::OracleMaxAge, &max_age_secs);
        Ok(())
    }

    /// Verify a recorded portfolio snapshot against the outstanding supply
    /// and reserve oracle inputs (#817).
    ///
    /// The snapshot's NAV per share (`nav_amount / outstanding_supply`) must
    /// equal the reserve-implied per-share NAV (`reserve / outstanding_supply`)
    /// within bounded rounding: the check is `|nav_amount - reserve| <=
    /// outstanding_supply`, i.e. less than one share-unit of NAV.
    ///
    /// Verification failures (uninitialized or stale oracle inputs, NAV
    /// mismatch) are *reported*, not thrown: the call returns
    /// `NavVerification { verified: false, reason }` and emits
    /// `nav_snapshot_rejected`, because a contract error would roll the
    /// event back. Only hard failures (unauthorized caller, missing
    /// snapshot) return `Err`.
    ///
    /// On success the wallet's `CurrentNav` is updated and
    /// `nav_snapshot_verified` is emitted; on failure the current NAV is
    /// left untouched.
    pub fn verify_nav_snapshot(
        env: Env,
        caller: Address,
        wallet: Address,
        date: u64,
    ) -> Result<NavVerification, Error> {
        ensure_admin_or_operator(&env, &caller)?;
        caller.require_auth();

        let nav_amount: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Snapshot(wallet.clone(), date))
            .ok_or(Error::SnapshotNotFound)?;

        let max_age = env
            .storage()
            .instance()
            .get(&DataKey::OracleMaxAge)
            .unwrap_or(DEFAULT_ORACLE_MAX_AGE_SECS);
        let now = env.ledger().timestamp();
        let inputs: Option<OracleInputs> =
            env.storage().instance().get(&DataKey::OracleInputs);

        let (verified, reason, supply, reserve) = match inputs {
            None => (false, Some(Symbol::new(&env, "oracle_not_set")), 0, 0),
            Some(inp) => {
                let supply = inp.outstanding_supply;
                let reserve = inp.reserve;
                if now.saturating_sub(inp.timestamp) > max_age {
                    (false, Some(Symbol::new(&env, "stale_oracle")), supply, reserve)
                } else {
                    let diff = if nav_amount >= reserve {
                        nav_amount.checked_sub(reserve).ok_or(Error::InvalidAmount)?
                    } else {
                        reserve.checked_sub(nav_amount).ok_or(Error::InvalidAmount)?
                    };
                    if diff <= supply {
                        (true, None, supply, reserve)
                    } else {
                        (false, Some(Symbol::new(&env, "nav_mismatch")), supply, reserve)
                    }
                }
            }
        };

        let verification = NavVerification {
            wallet: wallet.clone(),
            date,
            nav_amount,
            outstanding_supply: supply,
            reserve,
            verified,
            reason: reason.clone(),
            ledger: env.ledger().sequence(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::LastVerification(wallet.clone()), &verification);

        if verified {
            env.storage()
                .persistent()
                .set(&DataKey::CurrentNav(wallet.clone()), &nav_amount);
            events::nav_snapshot_verified(&env, &wallet, date, nav_amount, supply, reserve);
        } else if let Some(reason) = reason {
            events::nav_snapshot_rejected(
                &env, &wallet, date, nav_amount, supply, reserve, &reason,
            );
        }

        Ok(verification)
    }

    /// View: latest NAV verification outcome for a wallet.
    pub fn get_last_nav_verification(env: Env, wallet: Address) -> Option<NavVerification> {
        env.storage()
            .persistent()
            .get(&DataKey::LastVerification(wallet))
    }

    /// View: latest verified NAV amount for a wallet, if any. Rejected
    /// snapshots never update this value.
    pub fn get_current_nav(env: Env, wallet: Address) -> Option<i128> {
        env.storage().persistent().get(&DataKey::CurrentNav(wallet))
    }

    /// View: latest oracle inputs (supply, reserve, timestamp).
    pub fn get_oracle_inputs(env: Env) -> Option<OracleInputs> {
        env.storage().instance().get(&DataKey::OracleInputs)
    }

    /// View: configured maximum oracle input age in seconds.
    pub fn get_oracle_max_age(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::OracleMaxAge)
            .unwrap_or(DEFAULT_ORACLE_MAX_AGE_SECS)
    }
}

/// Reject unless `caller` is the admin or the operator.
fn ensure_admin_or_operator(env: &Env, caller: &Address) -> Result<(), Error> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotInitialized)?;
    let operator: Address = env
        .storage()
        .instance()
        .get(&DataKey::Operator)
        .ok_or(Error::NotInitialized)?;
    if caller != &admin && caller != &operator {
        return Err(Error::Unauthorized);
    }
    Ok(())
}

#[cfg(test)]
mod test;
