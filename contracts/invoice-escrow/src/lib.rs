//! Invoice Escrow contract for StellarSettle.
//!
//! Handles escrow creation, funding by investors, payment settlement,
//! and refunds when invoices are not paid by due date.

#![allow(clippy::too_many_arguments)]

mod errors;
mod events;
mod storage;
mod types;

use soroban_sdk::{contract, contractimpl, token, Address, Bytes, Env, IntoVal, Symbol, Val};

// EscrowStatus is re-exported publicly; Config and EscrowData are crate-private.
pub use types::EscrowStatus;
use types::{Config, DisputeData, EscrowData};

use errors::Error;

const MAX_BPS: u32 = 10_000;
const DISTRIBUTE_PAYMENT_FN: &str = "distribute_payment";
const DISTRIBUTE_REFUND_FN: &str = "distribute_refund";

/// Default dispute resolution timeout: 7 days in seconds.
const DEFAULT_DISPUTE_TIMEOUT_SECS: u64 = 7 * 24 * 60 * 60; // 604_800

#[contract]
pub struct InvoiceEscrow;

fn ensure_not_paused(config: &Config) -> Result<(), Error> {
    if config.paused {
        return Err(Error::Paused);
    }
    Ok(())
}

/// Return the effective dispute timeout (falls back to DEFAULT when stored as 0).
fn effective_dispute_timeout(config: &Config) -> u64 {
    if config.dispute_timeout_secs == 0 {
        DEFAULT_DISPUTE_TIMEOUT_SECS
    } else {
        config.dispute_timeout_secs
    }
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
            dispute_timeout_secs: 0, // 0 → use DEFAULT_DISPUTE_TIMEOUT_SECS
        };
        storage::set_config(&env, &config);
        Ok(())
    }

    /// Create an escrow for an invoice. Caller (seller) must be authenticated.
    /// face_value: what the debtor owes (amount to be paid at settlement)
    /// purchase_price: what the investor pays (discount applied here)
    /// commitment: immutable on-chain anchor (SHA-256 hash of off-chain invoice data)
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
    /// Transfers `amount` from buyer to this contract. Multiple investors can fund until fully subscribed.
    pub fn fund_escrow(
        env: Env,
        invoice_id: Symbol,
        buyer: Address,
        amount: i128,
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

        // Check that funding doesn't exceed purchase_price
        let new_funded = data.funded_amt.checked_add(amount).ok_or(Error::Overflow)?;
        if new_funded > data.purchase_price {
            return Err(Error::InvalidAmount);
        }

        let token = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();
        token.transfer(&buyer, &contract, &amount);

        // Mint invoice tokens to the buyer to represent their ownership share
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

        // Track this funder's contribution
        let current_funder_amt = storage::get_funder_amount(&env, invoice_id.clone(), &buyer);
        let new_funder_amt = current_funder_amt
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        storage::set_funder_amount(&env, invoice_id.clone(), &buyer, new_funder_amt);

        data.funded_amt = new_funded;

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
            // Forward both the payment amount (from payer) AND the collateral release
            // (from purchase_price) to the distributor so it can pay seller, investor, and admin.
            // The distributor receives: investor_amount + platform_fee (from payer) + amount (collateral)
            // = amount + amount = 2 * amount for a fully-funded invoice at parity.
            let total_to_distributor = investor_amount
                .checked_add(platform_fee)
                .ok_or(Error::Overflow)?
                .checked_add(amount)
                .ok_or(Error::Overflow)?;
            token.transfer(&contract, distributor, &total_to_distributor);
            let funder_val: Val = match funder_opt.clone() {
                Some(addr) => addr.into_val(&env),
                None => Val::from_void().into(),
            };
            env.invoke_contract::<()>(
                distributor,
                &Symbol::new(&env, DISTRIBUTE_PAYMENT_FN),
                soroban_sdk::vec![
                    &env,
                    contract.to_val(),
                    invoice_id.clone().into_val(&env),
                    soroban_sdk::vec![
                        &env,
                        data.token.clone().into_val(&env),
                        data.seller.clone().into_val(&env),
                        funder_val,
                        config.admin.clone().into_val(&env)
                    ]
                    .into_val(&env),
                    soroban_sdk::vec![&env, data.paid_amt, amount, investor_amount, platform_fee]
                        .into_val(&env),
                    (data.status as u32).into_val(&env)
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
                soroban_sdk::vec![&env, contract.to_val(), false.into_val(&env)],
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

        let token = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();

        // Extract funder address before status mutation so it is available in both paths.
        let funder_opt = data.funder.clone();

        data.status = EscrowStatus::Refunded;
        storage::set_escrow(&env, invoice_id.clone(), &data);

        if amount_to_refund > 0 {
            if let Some(distributor) = config.payment_distributor.as_ref() {
                token.transfer(&contract, distributor, &amount_to_refund);
                let refund_funder_val: Val = match funder_opt.clone() {
                    Some(addr) => addr.into_val(&env),
                    None => Val::from_void().into(),
                };
                env.invoke_contract::<()>(
                    distributor,
                    &Symbol::new(&env, DISTRIBUTE_REFUND_FN),
                    soroban_sdk::vec![
                        &env,
                        contract.to_val(),
                        invoice_id.clone().into_val(&env),
                        soroban_sdk::vec![
                            &env,
                            data.token.clone().into_val(&env),
                            refund_funder_val
                        ]
                        .into_val(&env),
                        soroban_sdk::vec![&env, amount_to_refund].into_val(&env),
                        (data.status as u32).into_val(&env)
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
            soroban_sdk::vec![&env, contract.to_val(), false.into_val(&env)],
        );

        events::escrow_refunded(&env, invoice_id, amount_to_refund);
        Ok(())
    }

    // ─── Dispute Resolution ────────────────────────────────────────────────────

    /// Raise a dispute on a funded (but not yet settled/refunded) escrow.
    ///
    /// Only the admin may raise a dispute.  The escrow must currently be in
    /// the `Funded` status.  Once raised the status transitions to `Disputed`
    /// and the dispute timestamp is recorded.
    ///
    /// * `invoice_id` – the invoice to dispute.
    /// * `reason`     – a short byte-string reason (e.g. b"delivery_failure").
    ///
    /// Emits `dispute_raised`.
    pub fn raise_dispute(
        env: Env,
        invoice_id: Symbol,
        reason: Bytes,
    ) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;
        // Only the admin is authorised to raise a dispute.
        config.admin.require_auth();

        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;

        // Can only dispute a funded escrow.
        if data.status == EscrowStatus::Disputed {
            return Err(Error::AlreadyDisputed);
        }
        if data.status != EscrowStatus::Funded {
            return Err(Error::EscrowNotFunded);
        }

        let raised_at = env.ledger().timestamp();

        // Persist the dispute data.
        let dispute = DisputeData {
            raiser: config.admin.clone(),
            reason: reason.clone(),
            raised_at,
            resolved: false,
        };
        storage::set_dispute_data(&env, invoice_id.clone(), &dispute);

        // Transition escrow to Disputed.
        data.status = EscrowStatus::Disputed;
        storage::set_escrow(&env, invoice_id.clone(), &data);

        events::dispute_raised(&env, invoice_id, &config.admin, &reason, raised_at);
        Ok(())
    }

    /// Resolve a disputed escrow.
    ///
    /// There are two resolution paths:
    ///
    /// **Admin resolution (within timeout)**
    /// The admin may call before the dispute timeout has elapsed and specify
    /// `favour` as either `"seller"` or `"buyer"`:
    /// - `"seller"` → the held `purchase_price` funds are released to the
    ///   seller (the delivery was accepted).
    /// - `"buyer"` → the held `purchase_price` funds are refunded to the
    ///   funder/investor (the delivery was rejected).
    ///
    /// **Default fallback (after timeout)**
    /// Anyone may call once the dispute timeout has elapsed.  The default
    /// fallback always refunds the buyer/funder — protecting investors from
    /// a dispute that is never resolved.  In this path `favour` is ignored.
    ///
    /// Emits `dispute_resolved`.
    pub fn resolve_dispute(
        env: Env,
        invoice_id: Symbol,
        favour: Symbol,
    ) -> Result<(), Error> {
        let config = storage::get_config(&env).ok_or(Error::NotInit)?;
        ensure_not_paused(&config)?;

        let mut data =
            storage::get_escrow(&env, invoice_id.clone()).ok_or(Error::EscrowNotFound)?;

        // Check if dispute data exists at all — if so, check if already resolved
        // before checking the current escrow status.
        if let Some(ref existing_dispute) = storage::get_dispute_data(&env, invoice_id.clone()) {
            if existing_dispute.resolved {
                return Err(Error::DisputeAlreadyResolved);
            }
        }

        // Must be in Disputed status.
        if data.status != EscrowStatus::Disputed {
            return Err(Error::NotDisputed);
        }

        let dispute = storage::get_dispute_data(&env, invoice_id.clone())
            .ok_or(Error::NotDisputed)?;

        let now = env.ledger().timestamp();
        let timeout = effective_dispute_timeout(&config);
        let timeout_elapsed = now >= dispute.raised_at.saturating_add(timeout);

        let favour_seller = Symbol::new(&env, "seller");
        let favour_buyer = Symbol::new(&env, "buyer");

        // Determine the actual resolution direction and whether auth is needed.
        let (resolve_to_seller, is_fallback) = if timeout_elapsed {
            // Default fallback: no auth required, always refund buyer.
            (false, true)
        } else {
            // Admin must explicitly resolve before timeout.
            config.admin.require_auth();
            if favour == favour_seller {
                (true, false)
            } else if favour == favour_buyer {
                (false, false)
            } else {
                return Err(Error::InvalidDisputeFavour);
            }
        };

        // Amount held in escrow for this invoice = purchase_price - paid_amt.
        let amount_held = data
            .purchase_price
            .checked_sub(data.paid_amt)
            .ok_or(Error::Overflow)?;

        let token = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();

        // Mark dispute as resolved before transfers (reentrancy guard pattern).
        let mut updated_dispute = dispute.clone();
        updated_dispute.resolved = true;
        storage::set_dispute_data(&env, invoice_id.clone(), &updated_dispute);

        // Update escrow status.
        if resolve_to_seller {
            data.status = EscrowStatus::Settled;
        } else {
            data.status = EscrowStatus::Refunded;
        }
        storage::set_escrow(&env, invoice_id.clone(), &data);

        // Execute transfers.
        let actual_favour: Symbol;
        if resolve_to_seller {
            // Seller wins: release the held purchase_price to the seller.
            actual_favour = favour_seller;
            if amount_held > 0 {
                token.transfer(&contract, &data.seller, &amount_held);
            }
            // Unlock invoice tokens on settlement.
            env.invoke_contract::<()>(
                &data.inv_token,
                &Symbol::new(&env, "set_transfer_locked"),
                soroban_sdk::vec![&env, contract.to_val(), false.into_val(&env)],
            );
        } else {
            // Buyer wins (or default fallback): refund the funder.
            actual_favour = favour_buyer;
            if amount_held > 0 {
                if let Some(ref funder) = data.funder {
                    // Pro-rata refund (MVP: single funder gets everything).
                    if data.funded_amt > 0 {
                        let funder_amt =
                            storage::get_funder_amount(&env, invoice_id.clone(), funder);
                        let pro_rata_refund = amount_held
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
            // Unlock invoice tokens on refund.
            env.invoke_contract::<()>(
                &data.inv_token,
                &Symbol::new(&env, "set_transfer_locked"),
                soroban_sdk::vec![&env, contract.to_val(), false.into_val(&env)],
            );
        }

        let resolver = if is_fallback {
            // The caller is the resolver for the fallback path; use env.current_contract_address()
            // as a neutral sentinel — no single external caller owns this.
            env.current_contract_address()
        } else {
            config.admin.clone()
        };

        events::dispute_resolved(
            &env,
            invoice_id,
            &resolver,
            actual_favour,
            amount_held,
            is_fallback,
        );
        Ok(())
    }

    /// Set the dispute resolution timeout (seconds). Admin only.
    /// Pass 0 to revert to the default (7 days / 604_800 seconds).
    pub fn set_dispute_timeout(env: Env, timeout_secs: u64) -> Result<(), Error> {
        let mut config = storage::get_config(&env).ok_or(Error::NotInit)?;
        config.admin.clone().require_auth();
        config.dispute_timeout_secs = timeout_secs;
        storage::set_config(&env, &config);
        Ok(())
    }

    // ─── Admin / config operations ─────────────────────────────────────────────

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

    // ─── View functions ────────────────────────────────────────────────────────

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

    /// View: return dispute data for an invoice, or error if none exists.
    pub fn get_dispute_data(env: Env, invoice_id: Symbol) -> Result<DisputeData, Error> {
        storage::get_dispute_data(&env, invoice_id).ok_or(Error::NotDisputed)
    }
}

#[cfg(test)]
mod integration_test;
#[cfg(test)]
mod test;
