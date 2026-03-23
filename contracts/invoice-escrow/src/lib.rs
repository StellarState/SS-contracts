//! Invoice Escrow contract for StellarSettle.
//!
//! Handles escrow creation, funding by investors, payment settlement,
//! and refunds when invoices are not paid by due date.

mod errors;
mod events;
mod storage;
mod types;

use soroban_sdk::{contract, contractimpl, token, Address, Env, IntoVal, Symbol};

use errors::Error;
use types::{Config, EscrowData, EscrowStatus};

const MAX_BPS: u32 = 10_000;

#[contract]
pub struct InvoiceEscrow;

#[contractimpl]
impl InvoiceEscrow {
    /// Initialize the contract with admin and platform fee (basis points, e.g. 300 = 3%).
    pub fn initialize(env: Env, admin: Address, platform_fee_bps: u32) -> Result<(), Error> {
        if storage::get_config(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        if platform_fee_bps > MAX_BPS {
            return Err(Error::InvalidFeeBps);
        }
        let config = Config {
            admin: admin.clone(),
            fee_bps: platform_fee_bps,
        };
        storage::set_config(&env, &config);
        Ok(())
    }

    /// Create an escrow for an invoice. Caller (seller) must be authenticated.
    pub fn create_escrow(
        env: Env,
        invoice_id: Symbol,
        seller: Address,
        amount: i128,
        due_date: u64,
        payment_token: Address,
        invoice_token: Address,
    ) -> Result<(), Error> {
        seller.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        storage::get_config(&env).ok_or(Error::NotInit)?;
        if storage::has_escrow(&env, invoice_id.clone()) {
            return Err(Error::EscrowExists);
        }
        let data = EscrowData {
            inv_id: invoice_id.clone(),
            seller: seller.clone(),
            amount,
            due_dt: due_date,
            token: payment_token.clone(),
            inv_token: invoice_token.clone(),
            funder: None,
            status: EscrowStatus::Created,
        };
        storage::set_escrow(&env, invoice_id.clone(), &data);
        events::escrow_created(
            &env,
            invoice_id,
            &seller,
            amount,
            due_date,
            &payment_token,
            &invoice_token,
        );
        Ok(())
    }

    /// Fund the escrow (investor buys the invoice). Transfers `amount` from buyer to this contract.
    pub fn fund_escrow(env: Env, invoice_id: Symbol, buyer: Address) -> Result<(), Error> {
        buyer.require_auth();
        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if data.status != EscrowStatus::Created {
            return Err(Error::EscrowFunded);
        }
        let amount = data.amount;
        let token = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();
        token.transfer(&buyer, &contract, &amount);

        // Mint invoice tokens to the buyer to represent ownership
        env.invoke_contract::<()>(
            &data.inv_token,
            &Symbol::new(&env, "mint"),
            soroban_sdk::vec![
                &env,
                buyer.to_val(),
                amount.into_val(&env),
                contract.to_val()
            ],
        );

        data.funder = Some(buyer.clone());
        data.status = EscrowStatus::Funded;
        storage::set_escrow(&env, invoice_id.clone(), &data);
        events::escrow_funded(&env, invoice_id, &buyer, amount);
        Ok(())
    }

    /// Record payment: distribute to investor (amount - fee) and platform (fee). Payer must auth.
    pub fn record_payment(
        env: Env,
        invoice_id: Symbol,
        payer: Address,
        amount: i128,
    ) -> Result<(), Error> {
        payer.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if data.status != EscrowStatus::Funded {
            return Err(Error::AlreadySettled);
        }
        if amount > data.amount {
            return Err(Error::InvalidAmount);
        }
        let funder = data.funder.as_ref().ok_or(Error::EscrowNotFunded)?;
        let fee_bps = i128::from(config.fee_bps);
        let platform_fee = amount
            .checked_mul(fee_bps)
            .ok_or(Error::Overflow)?
            .checked_div(i128::from(MAX_BPS))
            .ok_or(Error::Overflow)?;
        let investor_amount = amount.checked_sub(platform_fee).ok_or(Error::Overflow)?;
        let token = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();
        token.transfer(&contract, funder, &investor_amount);
        token.transfer(&contract, &config.admin, &platform_fee);
        data.status = EscrowStatus::Settled;
        storage::set_escrow(&env, invoice_id.clone(), &data);
        events::payment_settled(&env, invoice_id, amount, platform_fee, investor_amount);
        Ok(())
    }

    /// Refund the investor if the invoice was not paid by due date. Anyone may call.
    pub fn refund(env: Env, invoice_id: Symbol) -> Result<(), Error> {
        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if data.status != EscrowStatus::Funded {
            return Err(Error::RefundNotAllowed);
        }
        let ledger_ts = env.ledger().timestamp();
        if ledger_ts < data.due_dt {
            return Err(Error::RefundNotAllowed);
        }
        let funder = data.funder.as_ref().ok_or(Error::EscrowNotFunded)?;
        let amount = data.amount;
        let token = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();
        token.transfer(&contract, funder, &amount);
        data.status = EscrowStatus::Refunded;
        storage::set_escrow(&env, invoice_id.clone(), &data);
        events::escrow_refunded(&env, invoice_id, funder, amount);
        Ok(())
    }

    /// Update platform fee (basis points). Admin only.
    pub fn update_platform_fee_bps(env: Env, new_fee_bps: u32) -> Result<(), Error> {
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let admin = config.admin.clone();
        admin.require_auth();
        if new_fee_bps > MAX_BPS {
            return Err(Error::InvalidFeeBps);
        }
        config.fee_bps = new_fee_bps;
        storage::set_config(&env, &config);
        Ok(())
    }

    /// View: return escrow data for an invoice, or None if not found.
    pub fn get_escrow(env: Env, invoice_id: Symbol) -> Result<EscrowData, Error> {
        storage::get_escrow(&env, invoice_id).ok_or(Error::EscrowNotFound)
    }

    /// View: return current config (admin and fee_bps).
    pub fn get_config(env: Env) -> Result<Config, Error> {
        storage::get_config(&env).ok_or(Error::NotInit)
    }

    /// View: return escrow status for an invoice.
    pub fn get_escrow_status(env: Env, invoice_id: Symbol) -> Result<EscrowStatus, Error> {
        let data = storage::get_escrow(&env, invoice_id).ok_or(Error::EscrowNotFound)?;
        Ok(data.status)
    }
}

#[cfg(test)]
mod test;
