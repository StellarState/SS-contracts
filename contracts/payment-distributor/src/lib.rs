#![no_std]

mod errors;
mod events;
mod storage;
mod types;

pub use types::{DistributionState, FeeTier};

use soroban_sdk::{contract, contractimpl, token, Address, Env, Symbol, Vec};

use errors::Error;

const ESCROW_STATUS_FUNDED: u32 = 1;
const ESCROW_STATUS_SETTLED: u32 = 2;
const ESCROW_STATUS_REFUNDED: u32 = 3;
const MAX_FEE_BPS: u32 = 10_000;
const MAX_FANOUT_RECIPIENTS: u32 = 10;
const MAX_REFUND_RECIPIENTS: u32 = 10;

#[contract]
pub struct PaymentDistributor;

fn get_distribution_state(
    env: &Env,
    escrow_contract: &Address,
    invoice_id: &Symbol,
) -> types::DistributionState {
    storage::get_distribution(env, escrow_contract, invoice_id).unwrap_or(
        types::DistributionState {
            paid_distributed: 0,
            refund_distributed: false,
        },
    )
}

#[contractimpl]
impl PaymentDistributor {
    /// Initialize the contract with an admin.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if storage::get_admin(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        storage::set_admin(&env, &admin);
        events::initialized(&env, &admin);
        Ok(())
    }

    /// Distribute the latest settled payment delta for an escrow.
    ///
    /// The escrow contract must:
    /// 1. update its escrow state first,
    /// 2. transfer the settlement funds into this contract, and then
    /// 3. invoke this function as the configured distributor.
    ///
    /// `addresses`/`amounts` are `[token, seller, funder, admin, ...fanout]` /
    /// `[paid_amount, seller_amount, investor_amount, platform_fee, ...fanout_amounts]`.
    /// The optional trailing fanout entries split the platform fee across
    /// additional recipients (e.g. referral partners); their amounts are
    /// deducted from `platform_fee` and the remainder still goes to `admin`.
    pub fn distribute_payment(
        env: Env,
        escrow_contract: Address,
        invoice_id: Symbol,
        addresses: Vec<Address>,
        amounts: Vec<i128>,
        escrow_status: u32,
    ) -> Result<(), Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        escrow_contract.require_auth();

        if escrow_status != ESCROW_STATUS_FUNDED && escrow_status != ESCROW_STATUS_SETTLED {
            return Err(Error::InvalidEscrowStatus);
        }
        if addresses.len() != amounts.len() {
            return Err(Error::InvalidAmount);
        }
        if addresses.len() < 4 {
            return Err(Error::InvalidAmount);
        }
        let fanout_len = addresses.len() - 4;
        if fanout_len > MAX_FANOUT_RECIPIENTS {
            return Err(Error::TooManyFeeRecipients);
        }

        let token = addresses.get(0).ok_or(Error::InvalidAmount)?;
        let seller = addresses.get(1).ok_or(Error::InvalidAmount)?;
        let funder = addresses.get(2).ok_or(Error::InvalidAmount)?;
        let admin = addresses.get(3).ok_or(Error::InvalidAmount)?;
        let paid_amount = amounts.get(0).ok_or(Error::InvalidAmount)?;
        let mut state = get_distribution_state(&env, &escrow_contract, &invoice_id);
        let payment_amount = paid_amount
            .checked_sub(state.paid_distributed)
            .ok_or(Error::Overflow)?;

        if payment_amount <= 0 {
            return Err(Error::NothingToDistribute);
        }
        let seller_amount = amounts.get(1).ok_or(Error::InvalidAmount)?;
        let investor_amount = amounts.get(2).ok_or(Error::InvalidAmount)?;
        let platform_fee = amounts.get(3).ok_or(Error::InvalidAmount)?;
        if seller_amount != payment_amount {
            return Err(Error::InvalidAmount);
        }
        let total_payer_distribution = investor_amount
            .checked_add(platform_fee)
            .ok_or(Error::Overflow)?;
        if total_payer_distribution != payment_amount {
            return Err(Error::InvalidAmount);
        }

        let mut fanout_recipients: Vec<Address> = soroban_sdk::vec![&env];
        let mut fanout_amounts: Vec<i128> = soroban_sdk::vec![&env];
        let mut admin_fee = platform_fee;
        for i in 0..fanout_len {
            let idx = 4 + i;
            let recipient = addresses.get(idx).ok_or(Error::InvalidAmount)?;
            let recipient_amount = amounts.get(idx).ok_or(Error::InvalidAmount)?;
            if recipient_amount < 0 {
                return Err(Error::InvalidFeeSplit);
            }
            admin_fee = admin_fee
                .checked_sub(recipient_amount)
                .ok_or(Error::InvalidFeeSplit)?;
            if admin_fee < 0 {
                return Err(Error::InvalidFeeSplit);
            }
            fanout_recipients.push_back(recipient);
            fanout_amounts.push_back(recipient_amount);
        }

        // Investor yield bonus: an admin-configured cut of the platform fee is
        // redirected to the investor as a bonus, funded entirely out of
        // `admin_fee` so total token conservation is unaffected.
        let bonus_bps = storage::get_investor_bonus_bps(&env);
        let bonus = investor_amount
            .checked_mul(i128::from(bonus_bps))
            .ok_or(Error::Overflow)?
            .checked_div(i128::from(MAX_FEE_BPS))
            .ok_or(Error::Overflow)?;
        let bonus = if bonus > admin_fee { admin_fee } else { bonus };
        admin_fee = admin_fee.checked_sub(bonus).ok_or(Error::Overflow)?;
        let investor_payout = investor_amount.checked_add(bonus).ok_or(Error::Overflow)?;

        let token_client = token::Client::new(&env, &token);
        let contract_addr = env.current_contract_address();
        token_client.transfer(&contract_addr, &seller, &seller_amount);
        token_client.transfer(&contract_addr, &funder, &investor_payout);
        if admin_fee > 0 {
            token_client.transfer(&contract_addr, &admin, &admin_fee);
        }
        for i in 0..fanout_recipients.len() {
            let recipient = fanout_recipients.get(i).ok_or(Error::InvalidAmount)?;
            let recipient_amount = fanout_amounts.get(i).ok_or(Error::InvalidAmount)?;
            if recipient_amount > 0 {
                token_client.transfer(&contract_addr, &recipient, &recipient_amount);
            }
        }

        state.paid_distributed = paid_amount;
        storage::set_distribution(&env, &escrow_contract, &invoice_id, &state);

        let mut event_recipients: Vec<Address> = soroban_sdk::vec![&env, seller, funder, admin];
        event_recipients.append(&fanout_recipients);
        let mut event_amounts: Vec<i128> =
            soroban_sdk::vec![&env, seller_amount, investor_payout, admin_fee];
        event_amounts.append(&fanout_amounts);
        event_amounts.push_back(paid_amount);

        events::payment_distributed(
            &env,
            &escrow_contract,
            &invoice_id,
            &event_recipients,
            &event_amounts,
        );

        Ok(())
    }

    /// Configure the investor yield bonus rate (basis points, 0-10000).
    ///
    /// On each `distribute_payment` call, `bonus = investor_amount * bonus_bps
    /// / 10000` is redirected from the platform's fee share to the investor,
    /// capped at the fee actually available so `admin_fee` never goes
    /// negative. Total funds transferred are unchanged; only the split
    /// between admin and investor shifts.
    pub fn set_investor_bonus_bps(env: Env, admin: Address, bonus_bps: u32) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();

        if bonus_bps > MAX_FEE_BPS {
            return Err(Error::InvalidBonusRate);
        }

        storage::set_investor_bonus_bps(&env, bonus_bps);
        events::investor_bonus_rate_updated(&env, &admin, bonus_bps);
        Ok(())
    }

    /// View: return the configured investor yield bonus rate (basis points).
    pub fn get_investor_bonus_bps(env: Env) -> Result<u32, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        Ok(storage::get_investor_bonus_bps(&env))
    }

    /// Distribute the final refund for a refunded escrow.
    ///
    /// `addresses` is `[token, funder_1, .., funder_N]` (N >= 1). `amounts` is
    /// either `[refund_amount]` for a single funder (the legacy simple case,
    /// which receives the full amount), or `[refund_amount, weight_1, ..,
    /// weight_N]` to split `refund_amount` pro-rata across funders by
    /// contribution weight. Integer-division dust is assigned to the last
    /// funder so the full amount is always conserved.
    pub fn distribute_refund(
        env: Env,
        escrow_contract: Address,
        invoice_id: Symbol,
        addresses: Vec<Address>,
        amounts: Vec<i128>,
        escrow_status: u32,
    ) -> Result<(), Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        escrow_contract.require_auth();

        if escrow_status != ESCROW_STATUS_REFUNDED {
            return Err(Error::InvalidEscrowStatus);
        }
        if addresses.len() < 2 || amounts.is_empty() {
            return Err(Error::InvalidAmount);
        }

        let token = addresses.get(0).ok_or(Error::InvalidAmount)?;
        let funder_count = addresses.len() - 1;
        if funder_count > MAX_REFUND_RECIPIENTS {
            return Err(Error::TooManyRefundRecipients);
        }

        let mut state = get_distribution_state(&env, &escrow_contract, &invoice_id);
        if state.refund_distributed {
            return Err(Error::RefundAlreadyDistributed);
        }

        let refund_amount = amounts.get(0).ok_or(Error::InvalidAmount)?;
        if refund_amount <= 0 {
            return Err(Error::NothingToDistribute);
        }

        let mut recipients: Vec<Address> = soroban_sdk::vec![&env];
        let mut recipient_amounts: Vec<i128> = soroban_sdk::vec![&env];

        if funder_count == 1 && amounts.len() == 1 {
            let funder = addresses.get(1).ok_or(Error::InvalidAmount)?;
            recipients.push_back(funder);
            recipient_amounts.push_back(refund_amount);
        } else {
            if amounts.len() != addresses.len() {
                return Err(Error::InvalidAmount);
            }

            let mut total_weight: i128 = 0;
            for i in 0..funder_count {
                let weight = amounts.get(1 + i).ok_or(Error::InvalidAmount)?;
                if weight < 0 {
                    return Err(Error::InvalidRefundWeight);
                }
                total_weight = total_weight.checked_add(weight).ok_or(Error::Overflow)?;
            }
            if total_weight <= 0 {
                return Err(Error::InvalidRefundWeight);
            }

            let mut distributed: i128 = 0;
            for i in 0..funder_count {
                let funder = addresses.get(1 + i).ok_or(Error::InvalidAmount)?;
                let share = if i == funder_count - 1 {
                    // Last funder absorbs any rounding dust to conserve the total.
                    refund_amount
                        .checked_sub(distributed)
                        .ok_or(Error::Overflow)?
                } else {
                    let weight = amounts.get(1 + i).ok_or(Error::InvalidAmount)?;
                    refund_amount
                        .checked_mul(weight)
                        .ok_or(Error::Overflow)?
                        .checked_div(total_weight)
                        .ok_or(Error::Overflow)?
                };
                distributed = distributed.checked_add(share).ok_or(Error::Overflow)?;
                recipients.push_back(funder);
                recipient_amounts.push_back(share);
            }
        }

        let token_client = token::Client::new(&env, &token);
        let contract_addr = env.current_contract_address();
        for i in 0..recipients.len() {
            let funder = recipients.get(i).ok_or(Error::InvalidAmount)?;
            let share = recipient_amounts.get(i).ok_or(Error::InvalidAmount)?;
            if share > 0 {
                token_client.transfer(&contract_addr, &funder, &share);
            }
        }

        state.refund_distributed = true;
        storage::set_distribution(&env, &escrow_contract, &invoice_id, &state);

        events::refund_distributed(
            &env,
            &escrow_contract,
            &invoice_id,
            &recipients,
            &recipient_amounts,
        );
        Ok(())
    }

    /// Configure the tiered platform fee rate table.
    ///
    /// Tiers must be provided in ascending order, covering `[0, max_i128]`
    /// with no gaps or overlaps, so that every non-negative payment amount
    /// maps to exactly one fee rate. Each `fee_bps` must be at most 10000
    /// (100%).
    pub fn set_platform_fee(env: Env, admin: Address, tiers: Vec<FeeTier>) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();

        if tiers.is_empty() {
            return Err(Error::EmptyFeeTiers);
        }

        let mut expected_min: i128 = 0;
        for tier in tiers.iter() {
            if tier.min_amount != expected_min || tier.max_amount < tier.min_amount {
                return Err(Error::InvalidFeeTier);
            }
            if tier.fee_bps > MAX_FEE_BPS {
                return Err(Error::InvalidFeeTier);
            }
            expected_min = tier.max_amount.saturating_add(1);
        }

        storage::set_fee_tiers(&env, &tiers);
        events::platform_fee_updated(&env, &admin, &tiers);
        Ok(())
    }

    /// View: return the platform fee rate (in basis points) applicable to
    /// the given payment amount, based on the configured tier table.
    pub fn get_platform_fee_bps(env: Env, amount: i128) -> Result<u32, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        let tiers = storage::get_fee_tiers(&env).ok_or(Error::EmptyFeeTiers)?;
        for tier in tiers.iter() {
            if amount >= tier.min_amount && amount <= tier.max_amount {
                return Ok(tier.fee_bps);
            }
        }
        Err(Error::InvalidFeeTier)
    }

    /// View: return the current admin.
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)
    }

    /// View: return tracked distribution progress for an escrow invoice.
    pub fn get_distribution_state(
        env: Env,
        escrow_contract: Address,
        invoice_id: Symbol,
    ) -> Result<types::DistributionState, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)?;
        Ok(get_distribution_state(&env, &escrow_contract, &invoice_id))
    }
}

#[cfg(test)]
mod integration_test;
#[cfg(test)]
mod test;
