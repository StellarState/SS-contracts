#![cfg(test)]
#![allow(deprecated)]

use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{Address, Env, String, Symbol, TryFromVal, TryIntoVal, Val, Vec};

use crate::{Error, TreasuryManagement, TreasuryManagementClient};

fn zero_address(env: &Env) -> Address {
    Address::from_string(&String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ))
}

fn setup<'a>(env: &Env) -> (Address, TreasuryManagementClient<'a>) {
    env.mock_all_auths();
    let contract_id = env.register(TreasuryManagement, ());
    let client = TreasuryManagementClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (admin, client)
}

fn last_event(env: &Env) -> (Vec<Val>, Val) {
    let all = env.events().all();
    let events = all.events();
    let last = events.last().expect("no events emitted");
    let soroban_sdk::xdr::ContractEventBody::V0(v0) = &last.body;
    let topics = Vec::<Val>::try_from_val(
        env,
        &soroban_sdk::xdr::ScVal::Vec(Some(v0.topics.clone().into())),
    )
    .unwrap();
    let data = Val::try_from_val(env, &v0.data).unwrap();
    (topics, data)
}

#[test]
fn test_initialize_sets_admin() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    assert_eq!(client.get_admin(), admin);
}

#[test]
fn test_cannot_initialize_twice() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(TreasuryManagement, ());
    let client = TreasuryManagementClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    assert_eq!(client.try_initialize(&admin), Err(Ok(Error::AlreadyInit)));
}

#[test]
fn test_initialize_rejects_zero_address_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(TreasuryManagement, ());
    let client = TreasuryManagementClient::new(&env, &contract_id);
    assert_eq!(
        client.try_initialize(&zero_address(&env)),
        Err(Ok(Error::InvalidAddress))
    );
}

#[test]
fn test_receive_fee_attributed_to_source() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let source_a = Address::generate(&env);
    let source_b = Address::generate(&env);

    client.receive_fee(&source_a, &1000);
    client.receive_fee(&source_b, &500);
    client.receive_fee(&source_a, &200);

    let balance = client.get_treasury_balance();
    assert_eq!(balance.total, 1700);
    assert_eq!(balance.sources.len(), 2);

    // Find each source's amount regardless of insertion order in the pair
    // of parallel vectors, then confirm each is attributed correctly.
    let mut found_a = None;
    let mut found_b = None;
    for i in 0..balance.sources.len() {
        let src = balance.sources.get(i).unwrap();
        let amt = balance.amounts.get(i).unwrap();
        if src == source_a {
            found_a = Some(amt);
        } else if src == source_b {
            found_b = Some(amt);
        }
    }
    assert_eq!(found_a, Some(1200));
    assert_eq!(found_b, Some(500));
}

#[test]
fn test_receive_fee_rejects_non_positive_amount() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let source = Address::generate(&env);

    assert_eq!(
        client.try_receive_fee(&source, &0),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        client.try_receive_fee(&source, &-5),
        Err(Ok(Error::InvalidAmount))
    );
}

#[test]
fn test_allocate_routes_funds_and_reduces_balance() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let source = Address::generate(&env);
    let destination = Address::generate(&env);

    client.receive_fee(&source, &1000);
    let index = client.allocate(
        &admin,
        &destination,
        &300,
        &String::from_str(&env, "operations"),
    );
    assert_eq!(index, 0);

    assert_eq!(client.get_treasury_balance().total, 700);
}

#[test]
fn test_allocate_rejects_insufficient_balance() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let source = Address::generate(&env);
    let destination = Address::generate(&env);

    client.receive_fee(&source, &100);
    assert_eq!(
        client.try_allocate(&admin, &destination, &200, &String::from_str(&env, "grant")),
        Err(Ok(Error::InsufficientBalance))
    );
}

#[test]
fn test_allocate_rejects_non_admin() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let not_admin = Address::generate(&env);
    let source = Address::generate(&env);
    let destination = Address::generate(&env);

    client.receive_fee(&source, &1000);
    assert_eq!(
        client.try_allocate(
            &not_admin,
            &destination,
            &100,
            &String::from_str(&env, "reserve")
        ),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn test_allocate_rejects_empty_purpose() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let source = Address::generate(&env);
    let destination = Address::generate(&env);

    client.receive_fee(&source, &1000);
    assert_eq!(
        client.try_allocate(&admin, &destination, &100, &String::from_str(&env, "")),
        Err(Ok(Error::InvalidPurpose))
    );
}

#[test]
fn test_allocate_rejects_zero_address_destination() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let source = Address::generate(&env);

    client.receive_fee(&source, &1000);
    assert_eq!(
        client.try_allocate(
            &admin,
            &zero_address(&env),
            &100,
            &String::from_str(&env, "reserve")
        ),
        Err(Ok(Error::InvalidAddress))
    );
}

#[test]
fn test_get_allocation_history_paginated() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let source = Address::generate(&env);
    client.receive_fee(&source, &10_000);

    for i in 0..5u32 {
        let destination = Address::generate(&env);
        client.allocate(
            &admin,
            &destination,
            &(100 + i as i128),
            &String::from_str(&env, "grant"),
        );
    }

    assert_eq!(client.get_allocation_count(), 5);

    let page1 = client.get_allocation_history(&0, &2);
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0).unwrap().amount, 100);
    assert_eq!(page1.get(1).unwrap().amount, 101);

    let page2 = client.get_allocation_history(&2, &2);
    assert_eq!(page2.len(), 2);
    assert_eq!(page2.get(0).unwrap().amount, 102);
    assert_eq!(page2.get(1).unwrap().amount, 103);

    // Last page has fewer than `limit` remaining records.
    let page3 = client.get_allocation_history(&4, &2);
    assert_eq!(page3.len(), 1);
    assert_eq!(page3.get(0).unwrap().amount, 104);

    // Past the end returns empty, not an error.
    let page4 = client.get_allocation_history(&10, &2);
    assert_eq!(page4.len(), 0);
}

#[test]
fn test_fee_received_event_emitted() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let source = Address::generate(&env);

    client.receive_fee(&source, &1000);

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 2);
    let event_name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_name, Symbol::new(&env, "FeeReceived"));
    let amount: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(amount, 1000);
}

#[test]
fn test_funds_allocated_event_emitted() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let source = Address::generate(&env);
    let destination = Address::generate(&env);

    client.receive_fee(&source, &1000);
    client.allocate(
        &admin,
        &destination,
        &250,
        &String::from_str(&env, "operations"),
    );

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 2);
    let event_name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_name, Symbol::new(&env, "FundsAllocated"));
    let (amount, purpose): (i128, String) = data.try_into_val(&env).unwrap();
    assert_eq!(amount, 250);
    assert_eq!(purpose, String::from_str(&env, "operations"));
}
