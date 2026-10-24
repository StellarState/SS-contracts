//! Secondary market trade price history contract for StellarSettle.
//!
//! Records each executed secondary market trade price for an invoice's
//! fractions on-chain, enabling on-chain price discovery and chart data
//! for the backend without relying solely on event indexing.
//!
//! `record_trade` is restricted to a single authorized recorder address
//! (set at deploy time, normally the secondary-market contract itself),
//! since an unrestricted writer would let anyone inject fabricated trade
//! prices that corrupt the price history this contract exists to provide.

#![no_std]

mod errors;
mod events;
mod storage;
mod types;

pub use types::TradeRecord;

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String, Vec};

use errors::Error;

/// Reject the zero address (all-zero 32-byte Ed25519 key).
fn ensure_non_zero_address(env: &Env, address: &Address) -> Result<(), Error> {
    let zero_str = String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    );
    let zero = Address::from_string(&zero_str);
    if *address == zero {
        return Err(Error::InvalidAddress);
    }
    Ok(())
}

#[contract]
pub struct SecondaryTradeHistory;

#[contractimpl]
impl SecondaryTradeHistory {
    /// Initialize the contract with the admin (allowed to change the
    /// recorder) and the initial recorder address (normally the deployed
    /// secondary-market contract's address).
    pub fn initialize(env: Env, admin: Address, recorder: Address) -> Result<(), Error> {
        ensure_non_zero_address(&env, &admin)?;
        ensure_non_zero_address(&env, &recorder)?;
        if storage::get_admin(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        storage::set_admin(&env, &admin);
        storage::set_recorder(&env, &recorder);
        events::initialized(&env, &admin);
        Ok(())
    }

    /// Admin-only: change the authorized recorder address, e.g. after
    /// redeploying the secondary-market contract.
    pub fn set_recorder(env: Env, admin: Address, recorder: Address) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();
        ensure_non_zero_address(&env, &recorder)?;
        storage::set_recorder(&env, &recorder);
        Ok(())
    }

    /// Recorder-only: record one executed trade for `invoice_id`. Storage
    /// is pruned to the most recent 100 trades per invoice; the oldest is
    /// dropped on overflow.
    pub fn record_trade(
        env: Env,
        invoice_id: BytesN<32>,
        price: i128,
        amount: i128,
        timestamp: u64,
    ) -> Result<(), Error> {
        let recorder = storage::get_recorder(&env).ok_or(Error::NotInit)?;
        recorder.require_auth();

        if price <= 0 || amount <= 0 {
            return Err(Error::InvalidTrade);
        }

        storage::push_trade(
            &env,
            &invoice_id,
            TradeRecord {
                price,
                amount,
                timestamp,
            },
        );
        events::trade_recorded(&env, &invoice_id, price, amount, timestamp);
        Ok(())
    }

    /// View: return up to `limit` of `invoice_id`'s most recent trades,
    /// newest first. `limit` is clamped to the number of trades actually
    /// stored (and to the 100-trade retention cap).
    pub fn get_price_history(env: Env, invoice_id: BytesN<32>, limit: u32) -> Vec<TradeRecord> {
        let history = storage::get_history(&env, &invoice_id);
        let take = core::cmp::min(limit, history.len());
        let mut result = Vec::new(&env);
        for i in 0..take {
            result.push_back(history.get(i).unwrap());
        }
        result
    }

    /// View: return the most recent trade price for `invoice_id`.
    pub fn get_last_price(env: Env, invoice_id: BytesN<32>) -> Result<i128, Error> {
        let history = storage::get_history(&env, &invoice_id);
        history
            .get(0)
            .map(|record| record.price)
            .ok_or(Error::NoTrades)
    }

    /// View: return the current admin.
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)
    }

    /// View: return the current authorized recorder.
    pub fn get_recorder(env: Env) -> Result<Address, Error> {
        storage::get_recorder(&env).ok_or(Error::NotInit)
    }
}

#[cfg(test)]
mod test;
