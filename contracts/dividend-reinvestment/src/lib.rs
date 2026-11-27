#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, token, Address, BytesN,
    Env, Vec,
};

#[derive(Clone)]
#[contracttype]
pub(crate) enum DataKey {
    Admin,
    Config(Address),
    Invoice(BytesN<32>),
    Invoices,
    InitialInvoice(BytesN<32>),
}

#[cfg_attr(test, derive(Debug, Eq, PartialEq))]
#[derive(Clone)]
#[contracttype]
pub struct ReinvestmentConfig {
    pub token: Address,
    pub min_yield_bps: u32,
    pub max_amount: i128,
    pub enabled: bool,
}

#[cfg_attr(test, derive(Debug))]
#[derive(Clone, Eq, PartialEq)]
#[contracttype]
pub struct TargetInvoice {
    pub token: Address,
    pub yield_bps: u32,
    pub capacity: i128,
}

#[contractevent(topics = ["ReinvestmentExecuted"])]
#[cfg_attr(test, derive(Debug, Eq, PartialEq))]
#[derive(Clone)]
pub struct ReinvestmentExecuted {
    #[topic]
    pub wallet: Address,
    #[topic]
    pub invoice_id: BytesN<32>,
    pub payout_amount: i128,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidAmount = 3,
    NoMatchingInvoice = 4,
    Overflow = 5,
    InvoiceConflict = 6,
}

#[contract]
pub struct DividendReinvestment;

#[contractimpl]
impl DividendReinvestment {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    pub fn set_reinvestment_config(
        env: Env,
        wallet: Address,
        config: ReinvestmentConfig,
    ) -> Result<(), Error> {
        wallet.require_auth();
        if config.max_amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        env.storage()
            .persistent()
            .set(&DataKey::Config(wallet), &config);
        Ok(())
    }

    pub fn get_reinvestment_config(env: Env, wallet: Address) -> Option<ReinvestmentConfig> {
        env.storage().persistent().get(&DataKey::Config(wallet))
    }

    pub fn disable_reinvestment(env: Env, wallet: Address) -> Result<(), Error> {
        wallet.require_auth();
        if let Some(mut config) = Self::get_reinvestment_config(env.clone(), wallet.clone()) {
            config.enabled = false;
            env.storage()
                .persistent()
                .set(&DataKey::Config(wallet), &config);
        }
        Ok(())
    }

    pub fn register_invoice(
        env: Env,
        invoice_id: BytesN<32>,
        target: TargetInvoice,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        if target.capacity <= 0 {
            return Err(Error::InvalidAmount);
        }

        let initial_key = DataKey::InitialInvoice(invoice_id.clone());
        if let Some(initial) = env
            .storage()
            .persistent()
            .get::<_, TargetInvoice>(&initial_key)
        {
            if target != initial {
                return Err(Error::InvoiceConflict);
            }
            return Ok(());
        }

        let key = DataKey::Invoices;
        let mut ids: Vec<BytesN<32>> = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));
        ids.push_back(invoice_id.clone());
        env.storage().instance().set(&key, &ids);
        env.storage()
            .persistent()
            .set(&DataKey::Invoice(invoice_id), &target);
        env.storage().persistent().set(&initial_key, &target);
        Ok(())
    }

    pub fn execute_reinvestment(
        env: Env,
        wallet: Address,
        payout_amount: i128,
    ) -> Result<Option<BytesN<32>>, Error> {
        wallet.require_auth();
        if payout_amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let config = Self::get_reinvestment_config(env.clone(), wallet.clone());
        let Some(config) = config else {
            return Ok(None);
        };
        if !config.enabled || payout_amount > config.max_amount {
            return Ok(None);
        }
        let ids: Vec<BytesN<32>> = env
            .storage()
            .instance()
            .get(&DataKey::Invoices)
            .unwrap_or_else(|| Vec::new(&env));
        for id in ids.iter() {
            let key = DataKey::Invoice(id.clone());
            let Some(mut target) = env.storage().persistent().get::<_, TargetInvoice>(&key) else {
                continue;
            };
            if target.token != config.token
                || target.yield_bps < config.min_yield_bps
                || target.capacity < payout_amount
            {
                continue;
            }
            token::Client::new(&env, &config.token).transfer(
                &wallet,
                env.current_contract_address(),
                &payout_amount,
            );
            target.capacity -= payout_amount;
            env.storage().persistent().set(&key, &target);
            ReinvestmentExecuted {
                wallet: wallet.clone(),
                invoice_id: id.clone(),
                payout_amount,
            }
            .publish(&env);
            return Ok(Some(id));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod test;
