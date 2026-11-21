#![no_std]

mod events;
mod types;

#[cfg(test)]
mod test;

use types::StorageKey;
pub use types::{Error, HolderShare, SettlementInvoice};

use soroban_sdk::{contract, contractimpl, token, Address, BytesN, Env, Vec};

const MAX_FEE_BPS: u32 = 10_000;

fn admin(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&StorageKey::Admin)
        .ok_or(Error::NotInitialized)
}

fn invoice(env: &Env, invoice_id: &BytesN<32>) -> Result<SettlementInvoice, Error> {
    env.storage()
        .persistent()
        .get(&StorageKey::Invoice(invoice_id.clone()))
        .ok_or(Error::InvoiceNotFound)
}

fn save_invoice(env: &Env, data: &SettlementInvoice) {
    env.storage()
        .persistent()
        .set(&StorageKey::Invoice(data.invoice_id.clone()), data);
}

fn fee_for(env: &Env, amount: i128) -> Result<i128, Error> {
    let bps: u32 = env
        .storage()
        .instance()
        .get(&StorageKey::FeeBps)
        .unwrap_or(0);
    amount
        .checked_mul(bps as i128)
        .ok_or(Error::Overflow)?
        .checked_div(MAX_FEE_BPS as i128)
        .ok_or(Error::Overflow)
}

fn net_amount(env: &Env, gross: i128) -> Result<i128, Error> {
    let fee = fee_for(env, gross)?;
    gross.checked_sub(fee).ok_or(Error::Overflow)
}

fn holder_payout(
    data: &SettlementInvoice,
    wallet: &Address,
    distributable: i128,
) -> Result<i128, Error> {
    if distributable <= 0 || data.total_fractions <= 0 {
        return Ok(0);
    }
    for i in 0..data.holders.len() {
        let holder = data.holders.get(i).unwrap();
        if holder.wallet == *wallet {
            return holder
                .fractions
                .checked_mul(distributable)
                .ok_or(Error::Overflow)?
                .checked_div(data.total_fractions)
                .ok_or(Error::Overflow);
        }
    }
    Err(Error::HolderNotFound)
}

#[contract]
pub struct InvoiceSettlement;

#[contractimpl]
impl InvoiceSettlement {
    pub fn initialize(
        env: Env,
        admin: Address,
        treasury: Address,
        fee_bps: u32,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        if fee_bps > MAX_FEE_BPS {
            return Err(Error::InvalidFeeBps);
        }
        admin.require_auth();
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&StorageKey::Treasury, &treasury);
        env.storage().instance().set(&StorageKey::FeeBps, &fee_bps);
        Ok(())
    }

    pub fn register_invoice(
        env: Env,
        invoice_id: BytesN<32>,
        issuer: Address,
        token: Address,
        outstanding_balance: i128,
        holders: Vec<HolderShare>,
    ) -> Result<(), Error> {
        admin(&env)?;
        issuer.require_auth();
        if env
            .storage()
            .persistent()
            .has(&StorageKey::Invoice(invoice_id.clone()))
        {
            return Err(Error::InvoiceExists);
        }
        if holders.is_empty() {
            return Err(Error::InvalidHolders);
        }
        if outstanding_balance <= 0 {
            return Err(Error::InvalidAmount);
        }
        let mut total: i128 = 0;
        for holder in holders.iter() {
            if holder.fractions <= 0 {
                return Err(Error::InvalidHolders);
            }
            total = total.checked_add(holder.fractions).ok_or(Error::Overflow)?;
        }
        let data = SettlementInvoice {
            invoice_id,
            issuer,
            token,
            holders,
            total_fractions: total,
            outstanding_balance,
            repaid_amount: 0,
            distributed: false,
            payer: None,
            settlement_evidence_hash: None,
        };
        save_invoice(&env, &data);
        Ok(())
    }

    /// Record the verified off-chain evidence reference before settlement.
    /// Both the invoice issuer and the payer must authorize the evidence.
    pub fn record_settlement_evidence(
        env: Env,
        invoice_id: BytesN<32>,
        payer: Address,
        evidence_hash: BytesN<32>,
    ) -> Result<(), Error> {
        let mut data = invoice(&env, &invoice_id)?;
        data.issuer.require_auth();
        payer.require_auth();
        if evidence_hash == BytesN::from_array(&env, &[0; 32]) {
            return Err(Error::InvalidEvidence);
        }
        if data.settlement_evidence_hash.is_some() {
            return Err(Error::EvidenceAlreadySet);
        }
        let evidence_key = StorageKey::EvidenceUsed(evidence_hash.clone());
        if env.storage().persistent().has(&evidence_key) {
            return Err(Error::EvidenceAlreadyUsed);
        }
        env.storage().persistent().set(&evidence_key, &invoice_id);
        data.payer = Some(payer.clone());
        data.settlement_evidence_hash = Some(evidence_hash.clone());
        save_invoice(&env, &data);
        events::settlement_evidenced(&env, &invoice_id, &payer, &evidence_hash);
        Ok(())
    }

    pub fn repay(
        env: Env,
        invoice_id: BytesN<32>,
        amount: i128,
        payer: Address,
        evidence_hash: BytesN<32>,
    ) -> Result<(), Error> {
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let mut data = invoice(&env, &invoice_id)?;
        if data.repaid_amount > 0 {
            return Err(Error::AlreadyRepaid);
        }
        data.issuer.require_auth();
        payer.require_auth();
        if data.payer.as_ref() != Some(&payer)
            || data.settlement_evidence_hash.as_ref() != Some(&evidence_hash)
        {
            return Err(Error::EvidenceMismatch);
        }
        if amount != data.outstanding_balance {
            return Err(Error::InvalidAmount);
        }
        let token_client = token::Client::new(&env, &data.token);
        token_client.transfer(&payer, &env.current_contract_address(), &amount);
        data.repaid_amount = amount;
        data.outstanding_balance = 0;
        save_invoice(&env, &data);
        events::settle_with_evidence(&env, &invoice_id, &payer, &evidence_hash, amount);
        events::repayment_received(&env, &invoice_id, &data.issuer, amount);
        Ok(())
    }

    pub fn distribute_payouts(env: Env, invoice_id: BytesN<32>) -> Result<i128, Error> {
        let mut data = invoice(&env, &invoice_id)?;
        if data.repaid_amount <= 0 {
            return Err(Error::NothingToDistribute);
        }
        if data.distributed
            || env
                .storage()
                .persistent()
                .has(&StorageKey::PayoutDone(invoice_id.clone()))
        {
            return Err(Error::AlreadyDistributed);
        }

        let fee = fee_for(&env, data.repaid_amount)?;
        let distributable = net_amount(&env, data.repaid_amount)?;
        let token_client = token::Client::new(&env, &data.token);
        let contract = env.current_contract_address();
        let mut distributed: i128 = 0;

        for holder in data.holders.iter() {
            let payout = holder
                .fractions
                .checked_mul(distributable)
                .ok_or(Error::Overflow)?
                .checked_div(data.total_fractions)
                .ok_or(Error::Overflow)?;
            if payout > 0 {
                token_client.transfer(&contract, &holder.wallet, &payout);
                distributed = distributed.checked_add(payout).ok_or(Error::Overflow)?;
            }
        }

        if fee > 0 {
            let treasury: Address = env
                .storage()
                .instance()
                .get(&StorageKey::Treasury)
                .ok_or(Error::NotInitialized)?;
            token_client.transfer(&contract, &treasury, &fee);
        }

        data.distributed = true;
        save_invoice(&env, &data);
        env.storage()
            .persistent()
            .set(&StorageKey::PayoutDone(invoice_id.clone()), &true);
        events::settlement_complete(&env, &invoice_id, distributed, fee, data.repaid_amount);
        Ok(distributed)
    }

    pub fn get_payout(env: Env, invoice_id: BytesN<32>, wallet: Address) -> Result<i128, Error> {
        let data = invoice(&env, &invoice_id)?;
        let distributable = net_amount(&env, data.repaid_amount)?;
        holder_payout(&data, &wallet, distributable)
    }

    pub fn get_invoice(env: Env, invoice_id: BytesN<32>) -> Result<SettlementInvoice, Error> {
        invoice(&env, &invoice_id)
    }

    pub fn get_treasury(env: Env) -> Result<Address, Error> {
        admin(&env)?;
        env.storage()
            .instance()
            .get(&StorageKey::Treasury)
            .ok_or(Error::NotInitialized)
    }
}
