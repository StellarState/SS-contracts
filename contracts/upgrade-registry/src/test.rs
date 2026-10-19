#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, Symbol,
};

#[test]
fn test_initialize_and_admin_auth() {
    let env = Env::default();
    let contract_id = env.register(UpgradeRegistry, ());
    let client = UpgradeRegistryClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);

    // Initializing again should fail
    let err = client.try_initialize(&admin);
    assert_eq!(err, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
fn test_register_and_get_contract() {
    let env = Env::default();
    let contract_id = env.register(UpgradeRegistry, ());
    let client = UpgradeRegistryClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);

    let contract_name = Symbol::new(&env, "escrow");
    let addr_v1 = Address::generate(&env);

    // Initial registration
    client.register_contract(&contract_name, &addr_v1, &1);

    let entry = client.get_contract(&contract_name);
    assert_eq!(entry.address, addr_v1);
    assert_eq!(entry.version, 1);

    // Invalid version 0 should fail
    let err_zero = client.try_register_contract(&contract_name, &addr_v1, &0);
    assert_eq!(err_zero, Err(Ok(Error::InvalidVersion)));
}

#[test]
fn test_contract_upgrade_and_history() {
    let env = Env::default();
    let contract_id = env.register(UpgradeRegistry, ());
    let client = UpgradeRegistryClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);

    let name = Symbol::new(&env, "order_book");
    let addr_v1 = Address::generate(&env);
    let addr_v2 = Address::generate(&env);
    let addr_v3 = Address::generate(&env);

    env.ledger().set_timestamp(1000);
    client.register_contract(&name, &addr_v1, &1);

    // Upgrade to version 2 at timestamp 2000
    env.ledger().set_timestamp(2000);
    client.register_contract(&name, &addr_v2, &2);

    let current = client.get_contract(&name);
    assert_eq!(current.address, addr_v2);
    assert_eq!(current.version, 2);

    // Reject same version or lower version
    let err_same = client.try_register_contract(&name, &addr_v2, &2);
    assert_eq!(err_same, Err(Ok(Error::VersionNotHigher)));

    let err_lower = client.try_register_contract(&name, &addr_v1, &1);
    assert_eq!(err_lower, Err(Ok(Error::VersionNotHigher)));

    // Upgrade to version 3 at timestamp 3000
    env.ledger().set_timestamp(3000);
    client.register_contract(&name, &addr_v3, &3);

    // Check full history
    let history = client.get_upgrade_history(&name);
    assert_eq!(history.len(), 3);

    let h1 = history.get(0).unwrap();
    assert_eq!(h1.address, addr_v1);
    assert_eq!(h1.version, 1);
    assert_eq!(h1.timestamp, 1000);

    let h2 = history.get(1).unwrap();
    assert_eq!(h2.address, addr_v2);
    assert_eq!(h2.version, 2);
    assert_eq!(h2.timestamp, 2000);

    let h3 = history.get(2).unwrap();
    assert_eq!(h3.address, addr_v3);
    assert_eq!(h3.version, 3);
    assert_eq!(h3.timestamp, 3000);
}

#[test]
fn test_check_compatibility() {
    let env = Env::default();
    let contract_id = env.register(UpgradeRegistry, ());
    let client = UpgradeRegistryClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);

    let name = Symbol::new(&env, "yield_engine");
    let addr_v2 = Address::generate(&env);
    client.register_contract(&name, &addr_v2, &2);

    // Version 2 is compatible with min_version 1
    assert!(client.check_compatibility(&name, &1));
    // Version 2 is compatible with min_version 2
    assert!(client.check_compatibility(&name, &2));
    // Version 2 is NOT compatible with min_version 3
    assert!(!client.check_compatibility(&name, &3));

    // Non-existent contract returns error
    let unknown = Symbol::new(&env, "non_existent");
    let err = client.try_check_compatibility(&unknown, &1);
    assert_eq!(err, Err(Ok(Error::ContractNotFound)));
}
