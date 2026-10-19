use soroban_sdk::{Address, Env, Symbol};

pub fn contract_registered(env: &Env, name: &Symbol, address: &Address, version: u32) {
    let topics = (Symbol::new(env, "ContractRegistered"), name.clone());
    env.events().publish(topics, (address.clone(), version));
}

pub fn contract_upgraded(
    env: &Env,
    name: &Symbol,
    address: &Address,
    new_version: u32,
    previous_version: u32,
) {
    let topics = (Symbol::new(env, "ContractUpgraded"), name.clone());
    env.events()
        .publish(topics, (address.clone(), new_version, previous_version));
}
