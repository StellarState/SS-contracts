#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::xdr::{ContractEventBody, ScVal};
use soroban_sdk::{Address, BytesN, Env, Symbol, TryFromVal, TryIntoVal, Val, Vec};

use crate::{Error, PortfolioNavSnapshotContract, PortfolioNavSnapshotContractClient};

const DAY: u64 = 86_400;

fn setup<'a>(env: &Env) -> (Address, Address, PortfolioNavSnapshotContractClient<'a>) {
    env.mock_all_auths();
    let contract_id = env.register(PortfolioNavSnapshotContract, ());
    let client = PortfolioNavSnapshotContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let operator = Address::generate(env);
    client.initialize(&admin, &operator);
    (admin, operator, client)
}

fn invoice_id(env: &Env, byte: u8) -> BytesN<32> {
    BytesN::from_array(env, &[byte; 32])
}

/// Record a snapshot for `wallet` with the given NAV by holding `nav`
/// fractions of an invoice priced at 1.
fn snapshot_with_nav(
    env: &Env,
    client: &PortfolioNavSnapshotContractClient,
    operator: &Address,
    wallet: &Address,
    nav: i128,
) -> u64 {
    let inv = invoice_id(env, 7);
    client.set_invoice_price(operator, &inv, &1);
    client.set_investor_balance(operator, wallet, &inv, &nav);
    let date = (env.ledger().timestamp() / DAY) * DAY;
    client.take_portfolio_snapshot(wallet);
    date
}

fn event_names(env: &Env) -> Vec<Symbol> {
    let mut names = Vec::new(env);
    for e in env.events().all().events().iter() {
        let ContractEventBody::V0(v0) = &e.body;
        let topics = Vec::<Val>::try_from_val(
            env,
            &ScVal::Vec(Some(v0.topics.clone().into())),
        )
        .unwrap();
        names.push_back(topics.get(0).unwrap().try_into_val(env).unwrap());
    }
    names
}

#[test]
fn test_verify_nav_snapshot_success() {
    let env = Env::default();
    let (admin, operator, client) = setup(&env);
    env.ledger().set_timestamp(1_000_000);

    let wallet = Address::generate(&env);
    let date = snapshot_with_nav(&env, &client, &operator, &wallet, 1000);

    client.set_nav_oracle_inputs(&operator, &10, &1000);
    let result = client.verify_nav_snapshot(&operator, &wallet, &date);
    assert!(result.verified);
    assert_eq!(result.reason, None);
    assert_eq!(result.nav_amount, 1000);
    assert_eq!(result.outstanding_supply, 10);
    assert_eq!(result.reserve, 1000);

    // `events().all()` reflects only the most recent invocation, so inspect
    // them before issuing further calls.
    let names = event_names(&env);
    assert!(names.contains(&Symbol::new(&env, "nav_snapshot_verified")));

    assert_eq!(client.get_current_nav(&wallet), Some(1000));
    let stored = client.get_last_nav_verification(&wallet).unwrap();
    assert!(stored.verified);

    // Admin can also run verification.
    let result2 = client.verify_nav_snapshot(&admin, &wallet, &date);
    assert!(result2.verified);
}

#[test]
fn test_verify_nav_snapshot_rounding_tolerance() {
    let env = Env::default();
    let (_admin, operator, client) = setup(&env);
    env.ledger().set_timestamp(1_000_000);

    let wallet = Address::generate(&env);
    // NAV differs from reserve by 5, within the supply (10) rounding bound.
    let date = snapshot_with_nav(&env, &client, &operator, &wallet, 1005);
    client.set_nav_oracle_inputs(&operator, &10, &1000);

    let result = client.verify_nav_snapshot(&operator, &wallet, &date);
    assert!(result.verified);
}

#[test]
fn test_verify_nav_snapshot_rejects_mismatch() {
    let env = Env::default();
    let (_admin, operator, client) = setup(&env);
    env.ledger().set_timestamp(1_000_000);

    let wallet = Address::generate(&env);
    let date = snapshot_with_nav(&env, &client, &operator, &wallet, 2000);
    client.set_nav_oracle_inputs(&operator, &10, &1000);

    let result = client.verify_nav_snapshot(&operator, &wallet, &date);
    assert!(!result.verified);
    assert_eq!(result.reason, Some(Symbol::new(&env, "nav_mismatch")));

    let names = event_names(&env);
    assert!(names.contains(&Symbol::new(&env, "nav_snapshot_rejected")));

    assert_eq!(client.get_current_nav(&wallet), None);
}

#[test]
fn test_verify_nav_snapshot_rejects_stale_oracle() {
    let env = Env::default();
    let (_admin, operator, client) = setup(&env);
    env.ledger().set_timestamp(1_000_000);

    let wallet = Address::generate(&env);
    let date = snapshot_with_nav(&env, &client, &operator, &wallet, 1000);
    client.set_nav_oracle_inputs(&operator, &10, &1000);

    // Age the oracle inputs past the default 1h max age.
    env.ledger().set_timestamp(1_000_000 + 3601);
    let result = client.verify_nav_snapshot(&operator, &wallet, &date);
    assert!(!result.verified);
    assert_eq!(result.reason, Some(Symbol::new(&env, "stale_oracle")));
    assert_eq!(client.get_current_nav(&wallet), None);
}

#[test]
fn test_verify_nav_snapshot_rejects_uninitialized_oracle() {
    let env = Env::default();
    let (_admin, operator, client) = setup(&env);
    env.ledger().set_timestamp(1_000_000);

    let wallet = Address::generate(&env);
    let date = snapshot_with_nav(&env, &client, &operator, &wallet, 1000);

    let result = client.verify_nav_snapshot(&operator, &wallet, &date);
    assert!(!result.verified);
    assert_eq!(result.reason, Some(Symbol::new(&env, "oracle_not_set")));
}

#[test]
fn test_failed_snapshot_does_not_replace_current_nav() {
    let env = Env::default();
    let (_admin, operator, client) = setup(&env);
    env.ledger().set_timestamp(1_000_000);

    let wallet = Address::generate(&env);
    let date1 = snapshot_with_nav(&env, &client, &operator, &wallet, 1000);
    client.set_nav_oracle_inputs(&operator, &10, &1000);
    assert!(client.verify_nav_snapshot(&operator, &wallet, &date1).verified);
    assert_eq!(client.get_current_nav(&wallet), Some(1000));

    // Next day: a mismatched snapshot is rejected and must not replace NAV.
    env.ledger().set_timestamp(1_000_000 + DAY);
    let date2 = snapshot_with_nav(&env, &client, &operator, &wallet, 5000);
    client.set_nav_oracle_inputs(&operator, &10, &1000);
    let result = client.verify_nav_snapshot(&operator, &wallet, &date2);
    assert!(!result.verified);
    assert_eq!(client.get_current_nav(&wallet), Some(1000));
}

#[test]
fn test_verify_nav_snapshot_unauthorized_and_missing() {
    let env = Env::default();
    let (_admin, operator, client) = setup(&env);
    env.ledger().set_timestamp(1_000_000);

    let wallet = Address::generate(&env);
    let date = snapshot_with_nav(&env, &client, &operator, &wallet, 1000);
    let stranger = Address::generate(&env);
    assert_eq!(
        client.try_verify_nav_snapshot(&stranger, &wallet, &date),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        client.try_verify_nav_snapshot(&operator, &wallet, &(date + DAY)),
        Err(Ok(Error::SnapshotNotFound))
    );
}

#[test]
fn test_oracle_input_validation_and_auth() {
    let env = Env::default();
    let (admin, operator, client) = setup(&env);
    let stranger = Address::generate(&env);

    assert_eq!(
        client.try_set_nav_oracle_inputs(&stranger, &10, &1000),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        client.try_set_nav_oracle_inputs(&operator, &0, &1000),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        client.try_set_nav_oracle_inputs(&operator, &10, &-1),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        client.try_set_oracle_max_age(&operator, &3600),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        client.try_set_oracle_max_age(&admin, &0),
        Err(Ok(Error::InvalidAmount))
    );

    client.set_oracle_max_age(&admin, &7200);
    assert_eq!(client.get_oracle_max_age(), 7200);
}
