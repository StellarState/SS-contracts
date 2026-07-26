//! Invoice Token (SEP-41) contract for StellarSettle.
//!
//! Implements a fungible token representing fractional ownership of an invoice,
//! with mint (admin/escrow), burn, allowances, and optional transfer lock.

mod errors;
mod events;
mod storage;
mod types;

use soroban_sdk::{contract, contractimpl, Address, Env, String as SorobanString, Symbol};

use crate::errors::Error;
use crate::types::TokenMetadata;

#[contract]
pub struct InvoiceToken;

#[contractimpl]
impl InvoiceToken {
    /// Validate that an address is not zero (null).
    fn validate_address(addr: &Address) -> Result<(), Error> {
        // In Soroban, we can check if address is zero by comparing with Address::default()
        // A zero address would be equivalent to Address::default()
        if *addr == Address::default() {
            return Err(Error::InvalidAddress);
        }
        Ok(())
    }

    /// Initialize the token with admin, metadata, and minter (escrow) address.
    pub fn initialize(
        env: Env,
        admin: Address,
        name: SorobanString,
        symbol: SorobanString,
        decimals: u32,
        invoice_id: Symbol,
        minter: Address,
    ) -> Result<(), Error> {
        // Validate address parameters
        Self::validate_address(&admin)?;
        Self::validate_address(&minter)?;

        if storage::get_metadata(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        let meta = TokenMetadata {
            admin: admin.clone(),
            minter,
            name,
            symbol,
            decimals,
            invoice_id,
            transfer_locked: true, // default locked until settlement
            paused: false,
        };
        storage::set_metadata(&env, &meta);
        storage::set_total_supply(&env, 0);
        Ok(())
    }

    // ---------- SEP-41 standard view functions ----------

    pub fn name(env: Env) -> Result<SorobanString, Error> {
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        Ok(meta.name)
    }

    pub fn symbol(env: Env) -> Result<SorobanString, Error> {
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        Ok(meta.symbol)
    }

    pub fn decimals(env: Env) -> Result<u32, Error> {
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        Ok(meta.decimals)
    }

    pub fn total_supply(env: Env) -> Result<i128, Error> {
        storage::get_metadata(&env).ok_or(Error::NotInit)?;
        Ok(storage::get_total_supply(&env))
    }

    pub fn balance(env: Env, id: Address) -> Result<i128, Error> {
        // Validate address parameters
        Self::validate_address(&id)?;
        
        storage::get_metadata(&env).ok_or(Error::NotInit)?;
        Ok(storage::get_balance(&env, &id))
    }

    pub fn allowance(env: Env, from: Address, spender: Address) -> Result<i128, Error> {
        // Validate address parameters
        Self::validate_address(&from)?;
        Self::validate_address(&spender)?;
        
        storage::get_metadata(&env).ok_or(Error::NotInit)?;
        let ledger = env.ledger().sequence();
        Ok(storage::get_allowance(&env, &from, &spender, ledger))
    }

    // ---------- SEP-41 transfer ----------

    /// Transfer amount from `from` to `to`. Requires `from` auth.
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) -> Result<(), Error> {
        // Validate address parameters
        Self::validate_address(&from)?;
        Self::validate_address(&to)?;
        
        from.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        if meta.paused {
            return Err(Error::Paused);
        }
        if meta.transfer_locked && from != meta.admin {
            return Err(Error::TransferLocked);
        }
        let from_balance = storage::get_balance(&env, &from);
        if from_balance < amount {
            return Err(Error::InsufficientBalance);
        }
        storage::set_balance(&env, &from, from_balance - amount);
        let to_balance = storage::get_balance(&env, &to)
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        storage::set_balance(&env, &to, to_balance);
        events::transfer_event(&env, &from, &to, amount);
        Ok(())
    }

    // ---------- SEP-41 allowance ----------

    /// Set allowance for spender. Requires `from` auth.
    pub fn approve(
        env: Env,
        from: Address,
        spender: Address,
        amount: i128,
        expiration_ledger: u32,
    ) -> Result<(), Error> {
        // Validate address parameters
        Self::validate_address(&from)?;
        Self::validate_address(&spender)?;
        
        from.require_auth();
        if amount < 0 {
            return Err(Error::InvalidAmount);
        }
        let ledger = env.ledger().sequence();
        if amount != 0 && expiration_ledger < ledger {
            return Err(Error::InvalidExpiration);
        }
        storage::set_allowance(&env, &from, &spender, amount, expiration_ledger);
        events::approve_event(&env, &from, &spender, amount, expiration_ledger);
        Ok(())
    }

    /// Transfer from `from` to `to` using allowance. Requires `spender` auth.
    pub fn transfer_from(
        env: Env,
        spender: Address,
        from: Address,
        to: Address,
        amount: i128,
    ) -> Result<(), Error> {
        // Validate address parameters
        Self::validate_address(&spender)?;
        Self::validate_address(&from)?;
        Self::validate_address(&to)?;
        
        spender.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        if meta.paused {
            return Err(Error::Paused);
        }
        if meta.transfer_locked && from != meta.admin {
            return Err(Error::TransferLocked);
        }
        let ledger = env.ledger().sequence();
        let allow = storage::get_allowance_data(&env, &from, &spender)
            .ok_or(Error::InsufficientAllowance)?;
        if allow.expiration_ledger < ledger {
            return Err(Error::AllowanceExpired);
        }
        if allow.amount < amount {
            return Err(Error::InsufficientAllowance);
        }
        let from_balance = storage::get_balance(&env, &from);
        if from_balance < amount {
            return Err(Error::InsufficientBalance);
        }
        storage::set_allowance(
            &env,
            &from,
            &spender,
            allow.amount - amount,
            allow.expiration_ledger,
        );
        storage::set_balance(&env, &from, from_balance - amount);
        let to_balance = storage::get_balance(&env, &to)
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        storage::set_balance(&env, &to, to_balance);
        events::transfer_event(&env, &from, &to, amount);
        Ok(())
    }

    // ---------- SEP-41 burn ----------

    /// Burn amount from `from`. Requires `from` auth.
    pub fn burn(env: Env, from: Address, amount: i128) -> Result<(), Error> {
        // Validate address parameters
        Self::validate_address(&from)?;
        
        from.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        if meta.paused {
            return Err(Error::Paused);
        }
        let balance = storage::get_balance(&env, &from);
        if balance < amount {
            return Err(Error::InsufficientBalance);
        }
        storage::set_balance(&env, &from, balance - amount);
        let new_supply = storage::get_total_supply(&env)
            .checked_sub(amount)
            .ok_or(Error::Overflow)?;
        storage::set_total_supply(&env, new_supply);
        events::burn_event(&env, &from, amount);
        Ok(())
    }

    /// Burn from `from` using spender's allowance. Requires `spender` auth.
    pub fn burn_from(env: Env, spender: Address, from: Address, amount: i128) -> Result<(), Error> {
        // Validate address parameters
        Self::validate_address(&spender)?;
        Self::validate_address(&from)?;
        
        spender.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        if meta.paused {
            return Err(Error::Paused);
        }
        let ledger = env.ledger().sequence();
        let allow = storage::get_allowance_data(&env, &from, &spender)
            .ok_or(Error::InsufficientAllowance)?;
        if allow.expiration_ledger < ledger {
            return Err(Error::AllowanceExpired);
        }
        if allow.amount < amount {
            return Err(Error::InsufficientAllowance);
        }
        let balance = storage::get_balance(&env, &from);
        if balance < amount {
            return Err(Error::InsufficientBalance);
        }
        storage::set_allowance(
            &env,
            &from,
            &spender,
            allow.amount - amount,
            allow.expiration_ledger,
        );
        storage::set_balance(&env, &from, balance - amount);
        let new_supply = storage::get_total_supply(&env)
            .checked_sub(amount)
            .ok_or(Error::Overflow)?;
        storage::set_total_supply(&env, new_supply);
        events::burn_event(&env, &from, amount);
        Ok(())
    }

    // ---------- Admin / minter ----------

    /// Mint tokens to `to`. Callable only by admin or minter (escrow).
    /// `by` must be admin or minter and must authorize the call.
    pub fn mint(env: Env, to: Address, amount: i128, by: Address) -> Result<(), Error> {
        // Validate address parameters
        Self::validate_address(&to)?;
        Self::validate_address(&by)?;
        
        by.require_auth();
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        if meta.paused {
            return Err(Error::Paused);
        }
        if by != meta.admin && by != meta.minter {
            return Err(Error::Unauthorized);
        }
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let new_balance = storage::get_balance(&env, &to)
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        let new_supply = storage::get_total_supply(&env)
            .checked_add(amount)
            .ok_or(Error::Overflow)?;
        storage::set_balance(&env, &to, new_balance);
        storage::set_total_supply(&env, new_supply);
        events::mint_event(&env, &to, amount);
        Ok(())
    }

    /// Set transfer lock. Callable by admin or minter (escrow contract).
    /// When true, only admin can transfer; when false, all holders can transfer.
    pub fn set_transfer_locked(env: Env, caller: Address, locked: bool) -> Result<(), Error> {
        // Validate address parameters
        Self::validate_address(&caller)?;
        
        caller.require_auth();
        let mut meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        if caller != meta.admin && caller != meta.minter {
            return Err(Error::Unauthorized);
        }
        let old_locked = meta.transfer_locked;
        meta.transfer_locked = locked;
        storage::set_metadata(&env, &meta);
        events::transfer_locked_updated_event(&env, old_locked, locked);
        Ok(())
    }

    /// Set minter address (admin only).
    pub fn set_minter(env: Env, new_minter: Address) -> Result<(), Error> {
        // Validate address parameters
        Self::validate_address(&new_minter)?;
        
        let mut meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        let old_minter = meta.minter.clone();
        meta.admin.require_auth();
        meta.minter = new_minter;
        storage::set_metadata(&env, &meta);
        events::minter_updated_event(&env, &old_minter, &meta.minter);
        Ok(())
    }

    /// Set emergency pause state. Admin only.
    pub fn set_paused(env: Env, paused: bool) -> Result<(), Error> {
        let mut meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        meta.admin.require_auth();
        let old_paused = meta.paused;
        meta.paused = paused;
        storage::set_metadata(&env, &meta);
        events::paused_updated_event(&env, old_paused, paused);
        Ok(())
    }

    /// Get invoice_id for this token (metadata).
    pub fn invoice_id(env: Env) -> Result<Symbol, Error> {
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        Ok(meta.invoice_id)
    }

    /// Check if transfers are locked.
    pub fn transfer_locked(env: Env) -> Result<bool, Error> {
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        Ok(meta.transfer_locked)
    }

    /// Check if the contract is paused.
    pub fn paused(env: Env) -> Result<bool, Error> {
        let meta = storage::get_metadata(&env).ok_or(Error::NotInit)?;
        Ok(meta.paused)
    }
}

#[cfg(test)]
mod test;
