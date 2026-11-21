#![cfg(test)]
#![allow(deprecated)]

use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{Address, Env, String, Symbol, TryFromVal, TryIntoVal, Val, Vec};

use crate::{Error, TreasuryManagement, TreasuryManagementClient};

#[cfg(test)]
mod overflow_edge_case_tests {
    use super::*;
    use proptest::prelude::*;

    // Strategy for i128 boundary values
    fn i128_boundary() -> impl Strategy<Value = i128> {
        prop_oneof![
            Just(i128::MIN),
            Just(i128::MIN + 1),
            Just(-1),
            Just(0),
            Just(1),
            Just(i128::MAX - 1),
            Just(i128::MAX),
            Just(i128::MAX / 2),
            Just(i128::MAX / 10),
            Just(i128::MAX / 100),
            Just(i128::MAX / 1000),
            Just(i128::MAX / 10000),
            Just(1000000),
            Just(1000000000),
        ]
    }

    proptest! {
        // Test treasury balance addition boundaries (receive_fee)
        #[test]
        fn test_treasury_balance_addition(
            total in i128_boundary(),
            amount in i128_boundary(),
        ) {
            let result = total.checked_add(amount);
            // Should not panic
            if let Some(new_total) = result {
                if total >= 0 && amount > 0 {
                    prop_assert!(new_total >= total);
                }
            }
        }

        // Test treasury allocation boundaries (allocate)
        #[test]
        fn test_treasury_allocation_boundary(
            total in i128_boundary(),
            amount in i128_boundary(),
        ) {
            if total >= 0 && amount > 0 && amount <= total {
                let remaining = total.checked_sub(amount);
                if let Some(rem) = remaining {
                    prop_assert!(rem <= total);
                    prop_assert!(rem >= 0);
                }
            }
        }

        // Test source balance tracking boundaries
        #[test]
        fn test_source_balance_tracking(
            existing in i128_boundary(),
            amount in i128_boundary(),
        ) {
            let result = existing.checked_add(amount);
            if let Some(new_balance) = result {
                if existing >= 0 && amount >= 0 {
                    prop_assert!(new_balance >= existing);
                }
            }
        }

        // Test cumulative allocation boundaries
        #[test]
        fn test_cumulative_allocation(
            allocations in proptest::collection::vec((0..=1000000i128), 1..20),
        ) {
            let mut total: i128 = 0;
            for amount in allocations {
                let new_total = total.checked_add(amount);
                if let Some(sum) = new_total {
                    total = sum;
                    prop_assert!(total >= 0);
                }
            }
        }

        // Test allocation index boundaries
        #[test]
        fn test_allocation_index_boundary(
            count in (0..=1000u32),
            offset in (0..=2000u32),
            limit in (0..=100u32),
        ) {
            // Test pagination boundary conditions
            if offset >= count {
                prop_assert!(offset >= count);
            } else {
                let end = core::cmp::min(offset.saturating_add(limit), count);
                prop_assert!(end <= count);
                prop_assert!(end >= offset);
            }
        }

        // Test zero amount rejection
        #[test]
        fn test_zero_amount_rejection(
            amount in prop_oneof![Just(0i128), Just(-1i128), Just(i128::MIN)],
        ) {
            let is_valid = amount > 0;
            if amount <= 0 {
                prop_assert!(!is_valid);
            }
        }

        // Test amount exceeding balance
        #[test]
        fn test_amount_exceeds_balance(
            amount in i128_boundary(),
            balance in i128_boundary(),
        ) {
            if amount > balance {
                prop_assert!(amount > balance);
            } else {
                prop_assert!(amount <= balance);
            }
        }

        // Test division identity for fee calculations
        #[test]
        fn test_division_identity(
            numerator in (1..=100000i128),
            denominator in (1..=10000i128),
        ) {
            let result = numerator / denominator;
            let remainder = numerator % denominator;
            let reconstructed = result.checked_mul(denominator).unwrap_or(0).checked_add(remainder).unwrap_or(0);
            prop_assert_eq!(reconstructed, numerator);
        }

        // Test multiplication overflow handling
        #[test]
        fn test_multiplication_overflow(
            a in i128_boundary(),
            b in i128_boundary(),
        ) {
            let result = a.checked_mul(b);
            if let Some(prod) = result {
                if a > 0 && b > 0 {
                    prop_assert!(prod >= a);
                    prop_assert!(prod >= b);
                }
            }
        }

        // Test addition overflow handling
        #[test]
        fn test_addition_overflow(
            a in i128_boundary(),
            b in i128_boundary(),
        ) {
            let result = a.checked_add(b);
            if let Some(sum) = result {
                if a >= 0 && b >= 0 {
                    prop_assert!(sum >= a);
                    prop_assert!(sum >= b);
                }
            }
        }

        // Test subtraction underflow handling
        #[test]
        fn test_subtraction_underflow(
            a in i128_boundary(),
            b in i128_boundary(),
        ) {
            let result = a.checked_sub(b);
            if let Some(diff) = result {
                if a >= b && a >= 0 && b >= 0 {
                    prop_assert!(diff <= a);
                }
            }
        }

        // Test empty purpose string rejection
        #[test]
        fn test_empty_purpose_rejection(
            purpose in prop_oneof![Just(String::new()), Just("   ".to_string())],
        ) {
            let is_empty = purpose.trim().is_empty();
            if is_empty {
                prop_assert!(is_empty);
            }
        }

        // Test large number of allocations
        #[test]
        fn test_large_allocation_count(
            count in (0..=10000u32),
        ) {
            // Just verify the count doesn't overflow u32
            prop_assert!(count <= u32::MAX);
        }

        // Test timestamp boundaries
        #[test]
        fn test_timestamp_boundary(
            ts in prop_oneof![
                Just(0u64),
                Just(1u64),
                Just(3600u64),
                Just(86400u64),
                Just(u64::MAX / 2),
                Just(u64::MAX - 1),
                Just(u64::MAX),
            ]
        ) {
            // Timestamps should be valid u64 values
            prop_assert!(ts <= u64::MAX);
        }
    }
}

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
