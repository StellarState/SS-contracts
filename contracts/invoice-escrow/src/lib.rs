//! Invoice Escrow contract for StellarSettle.
//!
//! Handles escrow creation, funding by investors, payment settlement,
//! and refunds when invoices are not paid by due date.

#![allow(clippy::too_many_arguments)]

mod errors;
mod events;
mod storage;
mod types;

use soroban_sdk::{contract, contractimpl, token, Address, Env, Symbol, Vec};

// EscrowStatus is re-exported publicly; Config and EscrowData are crate-private.
pub use types::EscrowStatus;
use types::{Config, EscrowData};

use errors::Error;

const MAX_BPS: u32 = 10_000;
const DISTRIBUTE_PAYMENT_FN: &str = "distribute_payment";
const DISTRIBUTE_REFUND_FN: &str = "distribute_refund";

#[contract]
pub struct InvoiceEscrow;

fn ensure_not_paused(config: &Config) -> Result<(), Error> {
    if config.paused {
        return Err(Error::Paused);
    }
    Ok(())
}

/// Return the `Some` value inside an `Option<Address>` wrapped as a Soroban Val.
/// Soroban SDK does not implement `IntoVal<Env, Val>` for `Option<Address>` directly,
/// so we convert via a helper that maps `None` → an error instead of panicking.
fn require_funder(funder: Option<Address>) -> Result<Address, Error> {
    funder.ok_or(Error::EscrowNotFunded)
}

/// Check that `token` is present in the `accepted_tokens` list.
fn is_token_accepted(accepted: &Vec<Address>, token: &Address) -> bool {
    for i in 0..accepted.len() {
        if &accepted.get(i).unwrap() == token {
            return true;
        }
    }
    false
}

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
            payment_distributor: None,
            paused: false,
        };
        storage::set_config(&env, &config);
        Ok(())
    }

    /// Create an escrow for an invoice.  Caller (seller) must be authenticated.
    ///
    /// # Parameters
    /// * `accepted_tokens` – non-empty list of token contract addresses accepted
    ///   for funding and payment.  The first element is the canonical token; all
    ///   tokens in the list are equally valid.  Pass a one-element Vec to keep the
    ///   original single-token behaviour.
    /// * `face_value` – what the debtor owes (amount to be paid at settlement).
    /// * `purchase_price` – what the investor(s) pay in total (discount applied here).
    /// * `commitment` – immutable on-chain anchor (SHA-256 hash of off-chain invoice data).
    pub fn create_escrow(
        env: Env,
        invoice_id: Symbol,
        seller: Address,
        debtor: Address,
        face_value: i128,
        purchase_price: i128,
        due_date: u64,
        payment_token: Address,
        invoice_token: Address,
        commitment: soroban_sdk::BytesN<32>,
        accepted_tokens: Vec<Address>,
    ) -> Result<(), Error> {
        seller.require_auth();
        if face_value <= 0 || purchase_price <= 0 {
            return Err(Error::InvalidAmount);
        }
        if due_date == 0 {
            return Err(Error::InvalidDueDate);
        }
        let current_timestamp = env.ledger().timestamp();
        if due_date <= current_timestamp {
            return Err(Error::InvalidDueDate);
        }
        // accepted_tokens must be non-empty.
        if accepted_tokens.is_empty() {
            return Err(Error::InvalidAmount);
        }
        // payment_token must be in the accepted_tokens list.
        if !is_token_accepted(&accepted_tokens, &payment_token) {
            return Err(Error::TokenNotAccepted);
        }

        storage::get_config(&env).ok_or(Error::NotInit).and_then(|cfg| ensure_not_paused(&cfg))?;
        if storage::has_escrow(&env, invoice_id.clone()) {
            return Err(Error::EscrowExists);
        }
        let data = EscrowData {
            inv_id: invoice_id.clone(),
            seller: seller.clone(),
            debtor: debtor.clone(),
            face_value,
            purchase_price,
            funded_amt: 0,
            funder: None,
            due_dt: due_date,
            // `token` is set to `payment_token` (canonical token) until fund_escrow
            // locks it to whichever accepted token the first funder uses.
            token: payment_token.clone(),
            inv_token: invoice_token.clone(),
            paid_amt: 0,
            status: EscrowStatus::Created,
            commitment: commitment.clone(),
            accepted_tokens: accepted_tokens.clone(),
        };
        storage::set_escrow(&env, invoice_id.clone(), &data);
        events::escrow_created(
            &env,
            invoice_id,
            &seller,
            &debtor,
            face_value,
            purchase_price,
            due_date,
            &payment_token,
            &invoice_token,
            &commitment,
            &accepted_tokens,
        );
        Ok(())
    }

    /// Cancel an unfunded escrow. Only the seller may cancel, and only while status is Created.
    ///
    /// Emits `escrow_cancelled` with `(invoice_id, seller)`.
    pub fn cancel_escrow(env: Env, invoice_id: Symbol, seller: Address) -> Result<(), Error> {
        seller.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if data.seller != seller {
            return Err(Error::Unauthorized);
        }
        if data.status != EscrowStatus::Created {
            return Err(Error::EscrowFunded);
        }
        data.status = EscrowStatus::Cancelled;
        storage::set_escrow(&env, invoice_id.clone(), &data);
        events::escrow_cancelled(&env, invoice_id, &seller);
        Ok(())
    }

    /// Fund the escrow (investor buys part or all of the invoice at purchase_price).
    ///
    /// The `funding_token` parameter must be one of the escrow's `accepted_tokens`.
    /// All funding must use the **same** token: once the first funder's token is
    /// recorded (stored as `data.token`), subsequent partial funders must also use
    /// that token.
    ///
    /// Transfers `amount` from buyer to this contract.  Multiple investors can fund
    /// until fully subscribed.
    pub fn fund_escrow(
        env: Env,
        invoice_id: Symbol,
        buyer: Address,
        amount: i128,
        funding_token: Address,
    ) -> Result<(), Error> {
        buyer.require_auth();
        // Fail fast: validate amount before hitting storage.
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;

        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if data.status == EscrowStatus::Cancelled {
            return Err(Error::EscrowCancelled);
        }
        if data.status != EscrowStatus::Created {
            return Err(Error::EscrowFunded);
        }

        // Validate that the funding token is accepted.
        if !is_token_accepted(&data.accepted_tokens, &funding_token) {
            return Err(Error::TokenNotAccepted);
        }

        // Once the first funder has chosen a token, all subsequent partial funders
        // must use the same token (stored in data.token after first fund).
        if data.funded_amt > 0 && funding_token != data.token {
            return Err(Error::TokenNotAccepted);
        }

        // Check that funding doesn't exceed purchase_price
        let new_funded = data.funded_amt.checked_add(amount).ok_or(Error::Overflow)?;
        if new_funded > data.purchase_price {
            return Err(Error::InvalidAmount);
        }

        let token = token::Client::new(&env, &funding_token);
        let contract = env.current_contract_address();
        token.transfer(&buyer, &contract, &amount);

        // Mint invoice tokens to the buyer to represent their ownership share
        env.invoke_contract::<()>(
            &data.inv_token,
            &Symbol::new(&env, "mint"),
            soroban_sdk::vec![
                &env,
                buyer.to_val(),
                soroban_sdk::IntoVal::into_val(&amount, &env),
                contract.to_val()
            ],
        );

        // Track this funder's contribution
        let current_funder_amt = storage::get_funder_amount(&env, invoice_id.clone(), &buyer);
        let new_funder_amt = current_funder_amt
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        storage::set_funder_amount(&env, invoice_id.clone(), &buyer, new_funder_amt);

        data.funded_amt = new_funded;

        // Lock in the funding token for this escrow (first funder determines it).
        if data.funded_amt == amount {
            // This is the first funding contribution — record the chosen token.
            data.token = funding_token.clone();
        }

        // MVP: Store the first funder for direct distribution
        if data.funder.is_none() {
            data.funder = Some(buyer.clone());
        }

        // If fully funded, transition to Funded status
        if data.funded_amt == data.purchase_price {
            data.status = EscrowStatus::Funded;
        }

        storage::set_escrow(&env, invoice_id.clone(), &data);
        events::escrow_funded(
            &env,
            invoice_id,
            &buyer,
            amount,
            data.funded_amt,
            data.purchase_price,
        );
        Ok(())
    }

    /// Record payment: distribute to investors and platform fee. Payer must auth.
    /// Payer must be the authorized debtor for this invoice.
    /// Payment is applied toward face_value; fees are calculated on the payment amount.
    /// MVP: Distributes pro-rata to all funders based on their contribution.
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
        ensure_not_paused(&config)?;
        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;

        // Enforce payer role: payer must be the authorized debtor
        if payer != data.debtor {
            return Err(Error::InvalidPayer);
        }

        if data.status != EscrowStatus::Funded {
            return Err(Error::AlreadySettled);
        }

        // Remaining balance toward face_value
        let remaining = data
            .face_value
            .checked_sub(data.paid_amt)
            .ok_or(Error::Overflow)?;
        if amount > remaining {
            return Err(Error::InvalidAmount);
        }

        let fee_bps = i128::from(config.fee_bps);
        // Fee is calculated on the payment amount (not face_value)
        let platform_fee = amount
            .checked_mul(fee_bps)
            .ok_or(Error::Overflow)?
            .checked_div(i128::from(MAX_BPS))
            .ok_or(Error::Overflow)?;
        let investor_amount = amount.checked_sub(platform_fee).ok_or(Error::Overflow)?;

        // Payments always use the locked-in funding token (data.token).
        let token = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();

        // 1. Pull payer's funds into escrow
        token.transfer(&payer, &contract, &amount);

        data.paid_amt = data.paid_amt.checked_add(amount).ok_or(Error::Overflow)?;

        // Settlement occurs when paid_amt reaches face_value
        if data.paid_amt == data.face_value {
            data.status = EscrowStatus::Settled;
        }

        storage::set_escrow(&env, invoice_id.clone(), &data);

        // Extract funder address before branching so it is available in both paths.
        let funder_opt = data.funder.clone();

        if let Some(distributor) = config.payment_distributor.as_ref() {
            // Forward the full payment amount to the distributor contract.
            let total_to_distributor = investor_amount
                .checked_add(platform_fee)
                .ok_or(Error::Overflow)?;
            token.transfer(&contract, distributor, &total_to_distributor);

            // Resolve Option<Address> to Address before building the Vec<Address>
            // because Soroban SDK cannot convert Option<Address> into Val directly.
            let funder_addr = require_funder(funder_opt.clone())?;

            env.invoke_contract::<()>(
                distributor,
                &Symbol::new(&env, DISTRIBUTE_PAYMENT_FN),
                soroban_sdk::vec![
                    &env,
                    contract.to_val(),
                    soroban_sdk::IntoVal::into_val(&invoice_id.clone(), &env),
                    soroban_sdk::vec![
                        &env,
                        data.token.clone(),
                        data.seller.clone(),
                        funder_addr,
                        config.admin.clone()
                    ]
                    .to_val(),
                    soroban_sdk::vec![&env, data.paid_amt, amount, investor_amount, platform_fee]
                        .to_val(),
                    soroban_sdk::IntoVal::into_val(&(data.status as u32), &env)
                ],
            );
        } else {
            // 2. Platform fee to admin
            token.transfer(&contract, &config.admin, &platform_fee);

            // 3. Pro-rata investor distribution
            if let Some(funder) = &funder_opt {
                if data.funded_amt > 0 && investor_amount > 0 {
                    let funder_amt =
                        storage::get_funder_amount(&env, invoice_id.clone(), funder);
                    let pro_rata_share = investor_amount
                        .checked_mul(funder_amt)
                        .ok_or(Error::Overflow)?
                        .checked_div(data.funded_amt)
                        .ok_or(Error::Overflow)?;
                    if pro_rata_share > 0 {
                        token.transfer(&contract, funder, &pro_rata_share);
                    }
                }
            }

            // 4. Release the purchase_price collateral back to the seller
            token.transfer(&contract, &data.seller, &amount);
        }

        if data.status == EscrowStatus::Settled {
            // Unlock invoice token transfers only when the invoice is completely settled.
            env.invoke_contract::<()>(
                &data.inv_token,
                &Symbol::new(&env, "set_transfer_locked"),
                soroban_sdk::vec![&env, contract.to_val(), soroban_sdk::IntoVal::into_val(&false, &env)],
            );
        }

        events::payment_settled(&env, invoice_id, amount, platform_fee, investor_amount);
        Ok(())
    }

    /// Refund the investors if the invoice was not paid by due date. Anyone may call.
    /// Refunds are distributed pro-rata based on each investor's contribution.
    pub fn refund(env: Env, invoice_id: Symbol) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if data.status != EscrowStatus::Funded {
            return Err(Error::RefundNotAllowed);
        }
        let ledger_ts = env.ledger().timestamp();
        if ledger_ts < data.due_dt {
            return Err(Error::RefundNotAllowed);
        }

        // Refund the remaining collateral (purchase_price minus already released partial payments)
        let amount_to_refund = data
            .purchase_price
            .checked_sub(data.paid_amt)
            .ok_or(Error::Overflow)?;

        // Refunds always use the locked-in funding token (data.token).
        let token = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();

        // Extract funder address before status mutation so it is available in both paths.
        let funder_opt = data.funder.clone();

        data.status = EscrowStatus::Refunded;
        storage::set_escrow(&env, invoice_id.clone(), &data);

        if amount_to_refund > 0 {
            if let Some(distributor) = config.payment_distributor.as_ref() {
                token.transfer(&contract, distributor, &amount_to_refund);

                // Resolve Option<Address> before building Vec<Address>.
                let funder_addr = require_funder(funder_opt.clone())?;

                env.invoke_contract::<()>(
                    distributor,
                    &Symbol::new(&env, DISTRIBUTE_REFUND_FN),
                    soroban_sdk::vec![
                        &env,
                        contract.to_val(),
                        soroban_sdk::IntoVal::into_val(&invoice_id.clone(), &env),
                        soroban_sdk::vec![&env, data.token.clone(), funder_addr].to_val(),
                        soroban_sdk::vec![&env, amount_to_refund].to_val(),
                        soroban_sdk::IntoVal::into_val(&(data.status as u32), &env)
                    ],
                );
            } else {
                // Pro-rata refund to funders
                if let Some(funder) = &funder_opt {
                    if data.funded_amt > 0 {
                        let funder_amt =
                            storage::get_funder_amount(&env, invoice_id.clone(), funder);
                        let pro_rata_refund = amount_to_refund
                            .checked_mul(funder_amt)
                            .ok_or(Error::Overflow)?
                            .checked_div(data.funded_amt)
                            .ok_or(Error::Overflow)?;
                        if pro_rata_refund > 0 {
                            token.transfer(&contract, funder, &pro_rata_refund);
                        }
                    }
                }
            }
        }

        // Unlock invoice token transfers now that the invoice is refunded
        env.invoke_contract::<()>(
            &data.inv_token,
            &Symbol::new(&env, "set_transfer_locked"),
            soroban_sdk::vec![&env, contract.to_val(), soroban_sdk::IntoVal::into_val(&false, &env)],
        );

        events::escrow_refunded(&env, invoice_id, amount_to_refund);
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
        let old_fee_bps = config.fee_bps;
        config.fee_bps = new_fee_bps;
        storage::set_config(&env, &config);
        events::platform_fee_updated(&env, old_fee_bps, new_fee_bps);
        Ok(())
    }

    /// Set the payment distributor used for settlement/refund fan-out. Admin only.
    pub fn set_payment_distributor(env: Env, payment_distributor: Address) -> Result<(), Error> {
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let admin = config.admin.clone();
        admin.require_auth();
        let old_distributor = config.payment_distributor.clone();
        config.payment_distributor = Some(payment_distributor.clone());
        storage::set_config(&env, &config);
        events::payment_distributor_updated(&env, old_distributor.is_some(), &payment_distributor);
        Ok(())
    }

    /// Toggle the emergency pause flag. Admin only.
    pub fn set_paused(env: Env, paused: bool) -> Result<(), Error> {
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        let admin = config.admin.clone();
        admin.require_auth();
        let old_paused = config.paused;
        config.paused = paused;
        storage::set_config(&env, &config);
        events::paused_updated(&env, old_paused, paused);
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

    /// View: return the current pause state.
    pub fn paused(env: Env) -> Result<bool, Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        Ok(config.paused)
    }
}

#[cfg(test)]
mod integration_test;
#[cfg(test)]
mod test;
