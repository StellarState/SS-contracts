//! Invoice Escrow contract for StellarSettle.
//!
//! Handles escrow creation, funding by investors, payment settlement,
//! and refunds when invoices are not paid by due date.

#![no_std]
#![allow(clippy::too_many_arguments)]

mod errors;
mod events;
mod storage;
mod types;

use soroban_sdk::{contract, contractimpl, token, Address, Env, IntoVal, Symbol};

// EscrowStatus and InvoiceCategory are re-exported publicly for client use.
pub use types::{EscrowStatus, InvoiceCategory};
// CategoryFeeSchedule and Config / EscrowData remain crate-private.
use types::{CategoryFeeSchedule, Config, EscrowData};

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
            whitelist_enabled: false,
        };
        storage::set_config(&env, &config);
        Ok(())
    }

    // ── Buyer whitelist ───────────────────────────────────────────────────────

    /// Admin-only: enable/disable buyer whitelist enforcement on `fund_escrow`.
    pub fn set_whitelist_enabled(env: Env, admin: Address, enabled: bool) -> Result<(), Error> {
        admin.require_auth();
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        config.whitelist_enabled = enabled;
        storage::set_config(&env, &config);
        Ok(())
    }

    /// Admin-only: add or remove a buyer from the whitelist.
    pub fn set_buyer_whitelisted(
        env: Env,
        admin: Address,
        buyer: Address,
        allowed: bool,
    ) -> Result<(), Error> {
        admin.require_auth();
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        if config.admin != admin {
            return Err(Error::Unauthorized);
        }
        storage::set_whitelisted(&env, &buyer, allowed);
        Ok(())
    }

    /// View: is `buyer` whitelisted to fund escrows.
    pub fn is_buyer_whitelisted(env: Env, buyer: Address) -> bool {
        storage::is_whitelisted(&env, &buyer)
    }

    // ── Category fee schedule ─────────────────────────────────────────────────

    /// Set (or update) a per-category fee schedule override. Admin only.
    ///
    /// When a category fee schedule is set, any escrow created under that
    /// `category` will use `fee_bps` instead of the global `Config.fee_bps`.
    /// Existing escrows are not affected — they retain the effective fee that
    /// was stamped at creation time.
    ///
    /// Emits `cat_fee_set(category_u32, old_fee_bps, new_fee_bps)`.
    pub fn set_category_fee(
        env: Env,
        category: InvoiceCategory,
        fee_bps: u32,
    ) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.require_auth();
        if fee_bps > MAX_BPS {
            return Err(Error::InvalidFeeBps);
        }
        let old = storage::get_category_fee(&env, category).map(|s| s.fee_bps);
        storage::set_category_fee(&env, category, &CategoryFeeSchedule { fee_bps });
        events::category_fee_set(&env, category, old, fee_bps);
        Ok(())
    }

    /// View: return the fee schedule for a given category, or `CategoryFeeNotFound`
    /// if no override has been set.
    pub fn get_category_fee_schedule(
        env: Env,
        category: InvoiceCategory,
    ) -> Result<CategoryFeeSchedule, Error> {
        storage::get_category_fee(&env, category).ok_or(Error::CategoryFeeNotFound)
    }

    // ── Escrow lifecycle ──────────────────────────────────────────────────────

    /// Create an escrow for an invoice. Caller (seller) must be authenticated.
    ///
    /// `face_value`     – what the debtor owes (amount to be paid at settlement).
    /// `purchase_price` – what the investor pays (discount applied here).
    /// `commitment`     – immutable on-chain anchor (SHA-256 hash of off-chain invoice data).
    /// `category`       – invoice category; selects per-category fee override if one is set.
    ///
    /// The effective fee basis points are resolved at creation time:
    /// - If a `CategoryFeeSchedule` exists for `category`, that `fee_bps` is used.
    /// - Otherwise, `Config.fee_bps` (the global default) is used.
    ///
    /// The resolved fee is stored in `EscrowData.effective_fee_bps` so that
    /// subsequent fee changes do not affect outstanding escrows.
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
        category: InvoiceCategory,
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
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        if storage::has_escrow(&env, invoice_id.clone()) {
            return Err(Error::EscrowExists);
        }

        // Resolve effective fee: category override takes precedence over global default.
        let effective_fee_bps = storage::get_category_fee(&env, category)
            .map(|s| s.fee_bps)
            .unwrap_or(config.fee_bps);

        let data = EscrowData {
            inv_id: invoice_id.clone(),
            seller: seller.clone(),
            debtor: debtor.clone(),
            face_value,
            purchase_price,
            funded_amt: 0,
            funder: None,
            due_dt: due_date,
            token: payment_token.clone(),
            inv_token: invoice_token.clone(),
            paid_amt: 0,
            status: EscrowStatus::Created,
            commitment: commitment.clone(),
            category,
            effective_fee_bps,
        };
        storage::set_escrow(&env, invoice_id.clone(), &data);
        events::escrow_created(
            &env,
            invoice_id.clone(),
            &seller,
            &debtor,
            face_value,
            purchase_price,
            due_date,
            &payment_token,
            &invoice_token,
            &commitment,
            category,
            effective_fee_bps,
        );
        events::escrow_status_changed(&env, invoice_id, EscrowStatus::Created, current_timestamp);
        Ok(())
    }

    /// Cancel an unfunded escrow. Only the seller may cancel, and only while status is Created.
    ///
    /// Emits `escrow_cancelled` and `escrow_status_changed`.
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
        events::escrow_cancelled(&env, invoice_id.clone(), &seller);
        events::escrow_status_changed(
            &env,
            invoice_id,
            EscrowStatus::Cancelled,
            env.ledger().timestamp(),
        );
        Ok(())
    }

    /// Fund the escrow (investor buys part or all of the invoice at purchase_price).
    /// Transfers `amount` from buyer to this contract. Multiple investors can fund until fully subscribed.
    /// If whitelist is enabled, buyer must be whitelisted.
    pub fn fund_escrow(
        env: Env,
        invoice_id: Symbol,
        buyer: Address,
        amount: i128,
    ) -> Result<(), Error> {
        buyer.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        if config.whitelist_enabled && !storage::is_whitelisted(&env, &buyer) {
            return Err(Error::NotWhitelisted);
        }

        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;
        if data.status == EscrowStatus::Cancelled {
            return Err(Error::EscrowCancelled);
        }
        if data.status != EscrowStatus::Created {
            return Err(Error::EscrowFunded);
        }

        let new_funded = data.funded_amt.checked_add(amount).ok_or(Error::Overflow)?;
        if new_funded > data.purchase_price {
            return Err(Error::InvalidAmount);
        }

        let token = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();
        token.transfer(&buyer, &contract, &amount);

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

        let current_funder_amt = storage::get_funder_amount(&env, invoice_id.clone(), &buyer);
        let new_funder_amt = current_funder_amt
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        storage::set_funder_amount(&env, invoice_id.clone(), &buyer, new_funder_amt);

        data.funded_amt = new_funded;

        if data.funder.is_none() {
            data.funder = Some(buyer.clone());
        }

        if data.funded_amt == data.purchase_price {
            data.status = EscrowStatus::Funded;
        }

        storage::set_escrow(&env, invoice_id.clone(), &data);
        events::escrow_funded(
            &env,
            invoice_id.clone(),
            &buyer,
            amount,
            data.funded_amt,
            data.purchase_price,
        );
        if data.status == EscrowStatus::Funded {
            events::escrow_status_changed(
                &env,
                invoice_id,
                EscrowStatus::Funded,
                env.ledger().timestamp(),
            );
        }
        Ok(())
    }

    /// Record payment: distribute to investors and platform fee. Payer must auth.
    ///
    /// Payer must be the authorized debtor for this invoice.
    /// Payment is applied toward `face_value`; fees are calculated on the payment
    /// amount using the **per-escrow** `effective_fee_bps` that was resolved at
    /// creation time (which may reflect a category-level override).
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

        if payer != data.debtor {
            return Err(Error::InvalidPayer);
        }

        if data.status != EscrowStatus::Funded {
            return Err(Error::AlreadySettled);
        }

        let remaining = data
            .face_value
            .checked_sub(data.paid_amt)
            .ok_or(Error::Overflow)?;
        if amount > remaining {
            return Err(Error::InvalidAmount);
        }

        // Use the effective fee stamped at creation time (may reflect a category override).
        let fee_bps = i128::from(data.effective_fee_bps);
        let platform_fee = amount
            .checked_mul(fee_bps)
            .ok_or(Error::Overflow)?
            .checked_div(i128::from(MAX_BPS))
            .ok_or(Error::Overflow)?;
        let investor_amount = amount.checked_sub(platform_fee).ok_or(Error::Overflow)?;

        let token = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();

        token.transfer(&payer, &contract, &amount);

        data.paid_amt = data.paid_amt.checked_add(amount).ok_or(Error::Overflow)?;

        if data.paid_amt == data.face_value {
            data.status = EscrowStatus::Settled;
        }

        storage::set_escrow(&env, invoice_id.clone(), &data);

        let funder_opt = data.funder.clone();

        if let Some(distributor) = config.payment_distributor.as_ref() {
            let total_to_distributor = investor_amount
                .checked_add(platform_fee)
                .ok_or(Error::Overflow)?;
            token.transfer(&contract, distributor, &total_to_distributor);
            env.invoke_contract::<()>(
                distributor,
                &Symbol::new(&env, DISTRIBUTE_PAYMENT_FN),
                soroban_sdk::vec![
                    &env,
                    contract.to_val(),
                    invoice_id.clone().into_val(&env),
                    soroban_sdk::vec![
                        &env,
                        data.token.clone(),
                        data.seller.clone(),
                        funder_opt.clone().into_val(&env),
                        config.admin.clone()
                    ]
                    .into_val(&env),
                    soroban_sdk::vec![&env, data.paid_amt, amount, investor_amount, platform_fee]
                        .into_val(&env),
                    (data.status as u32).into_val(&env)
                ],
            );
        } else {
            token.transfer(&contract, &config.admin, &platform_fee);

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

            token.transfer(&contract, &data.seller, &amount);
        }

        if data.status == EscrowStatus::Settled {
            env.invoke_contract::<()>(
                &data.inv_token,
                &Symbol::new(&env, "set_transfer_locked"),
                soroban_sdk::vec![&env, contract.to_val(), false.into_val(&env)],
            );
        }

        events::payment_settled(&env, invoice_id.clone(), amount, platform_fee, investor_amount);
        if data.status == EscrowStatus::Settled {
            events::escrow_status_changed(
                &env,
                invoice_id,
                EscrowStatus::Settled,
                env.ledger().timestamp(),
            );
        }
        Ok(())
    }

    /// Refund the investors if the invoice was not paid by due date. Anyone may call.
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

        let amount_to_refund = data
            .purchase_price
            .checked_sub(data.paid_amt)
            .ok_or(Error::Overflow)?;

        let token = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();

        let funder_opt = data.funder.clone();

        data.status = EscrowStatus::Refunded;
        storage::set_escrow(&env, invoice_id.clone(), &data);

        if amount_to_refund > 0 {
            if let Some(distributor) = config.payment_distributor.as_ref() {
                token.transfer(&contract, distributor, &amount_to_refund);
                env.invoke_contract::<()>(
                    distributor,
                    &Symbol::new(&env, DISTRIBUTE_REFUND_FN),
                    soroban_sdk::vec![
                        &env,
                        contract.to_val(),
                        invoice_id.clone().into_val(&env),
                        soroban_sdk::vec![
                            &env,
                            data.token.clone(),
                            funder_opt.clone().into_val(&env)
                        ]
                        .into_val(&env),
                        soroban_sdk::vec![&env, amount_to_refund].into_val(&env),
                        (data.status as u32).into_val(&env)
                    ],
                );
            } else {
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

        env.invoke_contract::<()>(
            &data.inv_token,
            &Symbol::new(&env, "set_transfer_locked"),
            soroban_sdk::vec![&env, contract.to_val(), false.into_val(&env)],
        );

        events::escrow_refunded(&env, invoice_id.clone(), amount_to_refund);
        events::escrow_status_changed(
            &env,
            invoice_id,
            EscrowStatus::Refunded,
            env.ledger().timestamp(),
        );
        Ok(())
    }

    // ── Admin configuration ───────────────────────────────────────────────────

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

    // ── View functions ────────────────────────────────────────────────────────

    /// View: return escrow data for an invoice.
    pub fn get_escrow(env: Env, invoice_id: Symbol) -> Result<EscrowData, Error> {
        storage::get_escrow(&env, invoice_id).ok_or(Error::EscrowNotFound)
    }

    /// View: return current config.
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
