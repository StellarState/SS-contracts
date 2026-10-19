#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Vec};
pub use types::{ContractEntry, StorageKey, UpgradeRecord};

#[contract]
pub struct UpgradeRegistry;

fn require_admin(env: &Env) -> Result<Address, Error> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&StorageKey::Admin)
        .ok_or(Error::NotInitialized)?;
    admin.require_auth();
    Ok(admin)
}

#[contractimpl]
impl UpgradeRegistry {
    /// Initialize the registry with an administrator.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&StorageKey::Admin, &admin);
        Ok(())
    }

    /// Admin-only: records initial contract deployment or upgrades an existing contract to a higher version.
    pub fn register_contract(
        env: Env,
        name: Symbol,
        address: Address,
        version: u32,
    ) -> Result<(), Error> {
        require_admin(&env)?;

        if version == 0 {
            return Err(Error::InvalidVersion);
        }

        let current_time = env.ledger().timestamp();
        let existing: Option<ContractEntry> = env
            .storage()
            .persistent()
            .get(&StorageKey::Contract(name.clone()));

        match existing {
            None => {
                // Initial registration
                let entry = ContractEntry {
                    address: address.clone(),
                    version,
                };
                env.storage()
                    .persistent()
                    .set(&StorageKey::Contract(name.clone()), &entry);

                let mut history: Vec<UpgradeRecord> = Vec::new(&env);
                history.push_back(UpgradeRecord {
                    address: address.clone(),
                    version,
                    timestamp: current_time,
                });
                env.storage()
                    .persistent()
                    .set(&StorageKey::History(name.clone()), &history);

                events::contract_registered(&env, &name, &address, version);
            }
            Some(curr) => {
                // Upgrade must specify a strictly higher version
                if version <= curr.version {
                    return Err(Error::VersionNotHigher);
                }

                let entry = ContractEntry {
                    address: address.clone(),
                    version,
                };
                env.storage()
                    .persistent()
                    .set(&StorageKey::Contract(name.clone()), &entry);

                let mut history: Vec<UpgradeRecord> = env
                    .storage()
                    .persistent()
                    .get(&StorageKey::History(name.clone()))
                    .unwrap_or(Vec::new(&env));
                history.push_back(UpgradeRecord {
                    address: address.clone(),
                    version,
                    timestamp: current_time,
                });
                env.storage()
                    .persistent()
                    .set(&StorageKey::History(name.clone()), &history);

                events::contract_upgraded(&env, &name, &address, version, curr.version);
            }
        }

        Ok(())
    }

    /// Returns current address and version for any registered contract.
    pub fn get_contract(env: Env, name: Symbol) -> Result<ContractEntry, Error> {
        env.storage()
            .persistent()
            .get(&StorageKey::Contract(name))
            .ok_or(Error::ContractNotFound)
    }

    /// Returns full version history with addresses and timestamps.
    pub fn get_upgrade_history(env: Env, name: Symbol) -> Result<Vec<UpgradeRecord>, Error> {
        // Ensure contract exists
        if !env.storage().persistent().has(&StorageKey::Contract(name.clone())) {
            return Err(Error::ContractNotFound);
        }

        let history: Vec<UpgradeRecord> = env
            .storage()
            .persistent()
            .get(&StorageKey::History(name))
            .unwrap_or(Vec::new(&env));
        Ok(history)
    }

    /// Returns compatible flag for caller validation (current version >= min_version).
    pub fn check_compatibility(env: Env, name: Symbol, min_version: u32) -> Result<bool, Error> {
        let entry = Self::get_contract(env, name)?;
        Ok(entry.version >= min_version)
    }

    /// Admin-only: transfer admin role to a new address.
    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        require_admin(&env)?;
        env.storage().instance().set(&StorageKey::Admin, &new_admin);
        Ok(())
    }
}
