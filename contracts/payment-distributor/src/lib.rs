#![no_std]

mod errors;
mod events;
mod storage;

use soroban_sdk::{contract, contractimpl, token, Address, Env};

use errors::Error;

#[contract]
pub struct PaymentDistributor;

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

    /// Distribute payment from the contract to a recipient. Admin only.
    pub fn distribute(
        env: Env,
        token: Address,
        recipient: Address,
        amount: i128,
    ) -> Result<(), Error> {
        let admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        admin.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let token_client = token::Client::new(&env, &token);
        let contract_addr = env.current_contract_address();

        // Transfer funds from contract to recipient
        token_client.transfer(&contract_addr, &recipient, &amount);

        events::distributed(&env, &token, &recipient, amount);

        Ok(())
    }

    /// View: return the current admin.
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)
    }
}

#[cfg(test)]
mod test;
