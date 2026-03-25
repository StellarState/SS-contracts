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
            paid_amt: 0,
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

        let remaining = data
            .amount
            .checked_sub(data.paid_amt)
            .ok_or(Error::Overflow)?;
        if amount > remaining {
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

        // 1. Transfer payer funds into escrow
        token.transfer(&payer, &contract, &amount);

        // 2. Distribute payer's funds out (investor + platform fee)
        token.transfer(&contract, funder, &investor_amount);
        token.transfer(&contract, &config.admin, &platform_fee);

        // 3. Release corresponding funding from initial buy-in back to the seller
        token.transfer(&contract, &data.seller, &amount);

        data.paid_amt = data
            .paid_amt
            .checked_add(amount)
            .ok_or(Error::Overflow)?;

        if data.paid_amt == data.amount {
            data.status = EscrowStatus::Settled;
            // Unlock invoice token transfers only when the invoice is completely settled
            env.invoke_contract::<()>(
                &data.inv_token,
                &Symbol::new(&env, "set_transfer_locked"),
                soroban_sdk::vec![&env, contract.to_val(), false.into_val(&env)],
            );
        }

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

        // Refund the remaining collateral (initial buy-in minus already released partial payments)
        let amount_to_refund = data
            .amount
            .checked_sub(data.paid_amt)
            .ok_or(Error::Overflow)?;

        let token = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();

        if amount_to_refund > 0 {
            token.transfer(&contract, funder, &amount_to_refund);
        }

        data.status = EscrowStatus::Refunded;
        storage::set_escrow(&env, invoice_id.clone(), &data);

        // Unlock invoice token transfers now that the invoice is refunded
        env.invoke_contract::<()>(
            &data.inv_token,
            &Symbol::new(&env, "set_transfer_locked"),
            soroban_sdk::vec![&env, contract.to_val(), false.into_val(&env)],
        );

        events::escrow_refunded(&env, invoice_id, funder, amount_to_refund);
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
mod integration_test;
#[cfg(test)]
mod test;
