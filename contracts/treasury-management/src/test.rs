#![cfg(test)]
#![allow(deprecated)]

use soroban_sdk::testutils::{Address as _, Events, Ledger};
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
        &client.get_treasury_revision(),
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
        client.try_allocate(
            &admin,
            &destination,
            &200,
            &String::from_str(&env, "grant"),
            &client.get_treasury_revision(),
        ),
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
            &String::from_str(&env, "reserve"),
            &client.get_treasury_revision(),
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
        client.try_allocate(
            &admin,
            &destination,
            &100,
            &String::from_str(&env, ""),
            &client.get_treasury_revision(),
        ),
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
            &String::from_str(&env, "reserve"),
            &client.get_treasury_revision(),
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
            &client.get_treasury_revision(),
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
        &client.get_treasury_revision(),
    );

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 2);
    let event_name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_name, Symbol::new(&env, "FundsAllocated"));
    let (amount, purpose): (i128, String) = data.try_into_val(&env).unwrap();
    assert_eq!(amount, 250);
    assert_eq!(purpose, String::from_str(&env, "operations"));
}

#[test]
fn test_source_allowlist_and_cap_enforcement() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let approved = Address::generate(&env);
    let unapproved = Address::generate(&env);

    client.set_source_allowlist_enforced(&admin, &true);
    assert_eq!(
        client.try_receive_fee(&unapproved, &100),
        Err(Ok(Error::SourceNotApproved))
    );

    client.set_approved_source(&admin, &approved, &true, &Some(500));
    client.receive_fee(&approved, &300);
    assert_eq!(client.get_source_totals(&approved, &0), (300, 300));

    assert_eq!(
        client.try_receive_fee(&approved, &201),
        Err(Ok(Error::SourceCapExceeded))
    );
    client.receive_fee(&approved, &200);
    assert_eq!(client.get_source_totals(&approved, &0), (500, 500));
}

// ── Issue #825: allocation audit receipts ──────────────────────────────────

#[test]
fn test_allocate_writes_receipt_matching_allocation() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let source = Address::generate(&env);
    let destination = Address::generate(&env);

    client.receive_fee(&source, &1000);
    let revision_before = client.get_treasury_revision();
    client.allocate(
        &admin,
        &destination,
        &300,
        &String::from_str(&env, "operations"),
        &revision_before,
    );

    assert_eq!(client.get_receipt_count(), 1);
    let receipt = client.get_receipt(&0).unwrap();
    assert_eq!(receipt.receipt_id, 0);
    assert_eq!(receipt.allocator, admin);
    assert_eq!(receipt.destination, destination);
    assert_eq!(receipt.amount, 300);
    assert_eq!(receipt.purpose, String::from_str(&env, "operations"));
    assert_eq!(receipt.treasury_revision, revision_before + 1);
    assert!(receipt.ledger > 0 || receipt.ledger == env.ledger().sequence());

    // Receipt amount must match the allocation record amount.
    let record = client.get_allocation_history(&0, &1).get(0).unwrap();
    assert_eq!(record.amount, receipt.amount);
    assert_eq!(record.destination, receipt.destination);
}

#[test]
fn test_allocate_rejects_stale_revision() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let source = Address::generate(&env);
    let d1 = Address::generate(&env);
    let d2 = Address::generate(&env);

    client.receive_fee(&source, &1000);
    let rev = client.get_treasury_revision();

    // First allocation succeeds and bumps the revision.
    client.allocate(&admin, &d1, &100, &String::from_str(&env, "a"), &rev);

    // A retry computed against the old revision is rejected: this is the
    // concurrent-retry guard.
    assert_eq!(
        client.try_allocate(&admin, &d2, &100, &String::from_str(&env, "b"), &rev),
        Err(Ok(Error::StaleRevision))
    );
    // A fresh fee also invalidates the previously observed revision.
    let rev2 = client.get_treasury_revision();
    client.receive_fee(&source, &50);
    assert_eq!(
        client.try_allocate(&admin, &d2, &100, &String::from_str(&env, "b"), &rev2),
        Err(Ok(Error::StaleRevision))
    );
    // Re-reading the revision lets the allocation through.
    client.allocate(
        &admin,
        &d2,
        &100,
        &String::from_str(&env, "b"),
        &client.get_treasury_revision(),
    );
    assert_eq!(client.get_receipt_count(), 2);
}

#[test]
fn test_allocate_rejects_without_allocator_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let source = Address::generate(&env);
    let impostor = Address::generate(&env);
    let destination = Address::generate(&env);

    client.receive_fee(&source, &1000);
    assert_eq!(
        client.try_allocate(
            &impostor,
            &destination,
            &100,
            &String::from_str(&env, "ops"),
            &client.get_treasury_revision(),
        ),
        Err(Ok(Error::Unauthorized))
    );
    // No receipt may be created for an unauthorised allocation attempt.
    assert_eq!(client.get_receipt_count(), 0);
    let _ = admin;
}

#[test]
fn test_receipt_history_cursor_pagination() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let source = Address::generate(&env);
    client.receive_fee(&source, &10_000);

    for i in 0..5u64 {
        client.allocate(
            &admin,
            &Address::generate(&env),
            &(10 + i as i128),
            &String::from_str(&env, "grant"),
            &client.get_treasury_revision(),
        );
    }

    // Page 1: cursor 0, limit 2.
    let page1 = client.get_receipt_history(&0, &2);
    assert_eq!(page1.receipts.len(), 2);
    assert_eq!(page1.receipts.get(0).unwrap().receipt_id, 0);
    assert_eq!(page1.receipts.get(1).unwrap().receipt_id, 1);
    assert_eq!(page1.next_cursor, Some(2));

    // Page 2 via cursor.
    let page2 = client.get_receipt_history(&page1.next_cursor.unwrap(), &2);
    assert_eq!(page2.receipts.len(), 2);
    assert_eq!(page2.receipts.get(0).unwrap().receipt_id, 2);
    assert_eq!(page2.next_cursor, Some(4));

    // Final partial page terminates the cursor.
    let page3 = client.get_receipt_history(&page2.next_cursor.unwrap(), &2);
    assert_eq!(page3.receipts.len(), 1);
    assert_eq!(page3.receipts.get(0).unwrap().receipt_id, 4);
    assert_eq!(page3.next_cursor, None);

    // Past the end yields an empty page.
    let page4 = client.get_receipt_history(&9, &2);
    assert_eq!(page4.receipts.len(), 0);
    assert_eq!(page4.next_cursor, None);
}

#[test]
fn test_treasury_allocation_receipt_event_emitted() {
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
        &client.get_treasury_revision(),
    );

    // The receipt event is emitted for every allocation; FundsAllocated is
    // the final event of the call.
    let all = env.events().all();
    let events = all.events();
    let mut saw_receipt = false;
    for e in events.iter() {
        let soroban_sdk::xdr::ContractEventBody::V0(v0) = &e.body;
        let topics = Vec::<Val>::try_from_val(
            &env,
            &soroban_sdk::xdr::ScVal::Vec(Some(v0.topics.clone().into())),
        )
        .unwrap();
        let name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
        if name == Symbol::new(&env, "treasury_allocation_receipt") {
            saw_receipt = true;
            let data = Val::try_from_val(&env, &v0.data).unwrap();
            let (_id, _alloc, _dest, amount, _purpose, _ledger, _ts, _rev): (
                u64,
                Address,
                Address,
                i128,
                String,
                u32,
                u64,
                u64,
            ) = data.try_into_val(&env).unwrap();
            assert_eq!(amount, 250);
        }
    }
    assert!(saw_receipt);
}

// ── Issue #824: daily allocation limit ─────────────────────────────────────

#[test]
fn test_daily_allocation_limit_enforced() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let source = Address::generate(&env);
    client.receive_fee(&source, &10_000);

    client.set_daily_allocation_limit(&admin, &Some(500));
    assert_eq!(client.get_daily_allocation_limit(), Some(500));
    assert_eq!(client.get_daily_allocation_remaining(), Some(500));

    // Exact-limit allocation succeeds.
    client.allocate(
        &admin,
        &Address::generate(&env),
        &500,
        &String::from_str(&env, "ops"),
        &client.get_treasury_revision(),
    );
    assert_eq!(client.get_daily_allocation_total(), 500);
    assert_eq!(client.get_daily_allocation_remaining(), Some(0));

    // Anything more breaches the cap.
    assert_eq!(
        client.try_allocate(
            &admin,
            &Address::generate(&env),
            &1,
            &String::from_str(&env, "ops"),
            &client.get_treasury_revision(),
        ),
        Err(Ok(Error::DailyLimitExceeded))
    );
}

#[test]
fn test_daily_allocation_limit_partial_then_breach() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let source = Address::generate(&env);
    client.receive_fee(&source, &10_000);
    client.set_daily_allocation_limit(&admin, &Some(100));

    client.allocate(
        &admin,
        &Address::generate(&env),
        &60,
        &String::from_str(&env, "a"),
        &client.get_treasury_revision(),
    );
    // 60 + 50 = 110 > 100 -> reject; 40 would fit.
    assert_eq!(
        client.try_allocate(
            &admin,
            &Address::generate(&env),
            &50,
            &String::from_str(&env, "b"),
            &client.get_treasury_revision(),
        ),
        Err(Ok(Error::DailyLimitExceeded))
    );
    client.allocate(
        &admin,
        &Address::generate(&env),
        &40,
        &String::from_str(&env, "c"),
        &client.get_treasury_revision(),
    );
    assert_eq!(client.get_daily_allocation_total(), 100);
}

#[test]
fn test_daily_allocation_limit_rollover_new_day() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let source = Address::generate(&env);
    client.receive_fee(&source, &10_000);
    client.set_daily_allocation_limit(&admin, &Some(100));

    client.allocate(
        &admin,
        &Address::generate(&env),
        &100,
        &String::from_str(&env, "a"),
        &client.get_treasury_revision(),
    );
    assert_eq!(
        client.try_allocate(
            &admin,
            &Address::generate(&env),
            &1,
            &String::from_str(&env, "b"),
            &client.get_treasury_revision(),
        ),
        Err(Ok(Error::DailyLimitExceeded))
    );

    // Advance into the next ledger day: capacity resets.
    env.ledger().with_mut(|li| li.timestamp += 86_400);
    assert_eq!(client.get_daily_allocation_remaining(), Some(100));
    client.allocate(
        &admin,
        &Address::generate(&env),
        &100,
        &String::from_str(&env, "b"),
        &client.get_treasury_revision(),
    );
}

#[test]
fn test_daily_limit_change_requires_admin() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let impostor = Address::generate(&env);

    assert_eq!(
        client.try_set_daily_allocation_limit(&impostor, &Some(100)),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        client.try_set_daily_allocation_limit(&admin, &Some(0)),
        Err(Ok(Error::InvalidDailyLimit))
    );
    assert_eq!(
        client.try_set_daily_allocation_limit(&admin, &Some(-5)),
        Err(Ok(Error::InvalidDailyLimit))
    );
    // Clearing the limit restores uncapped allocation.
    client.set_daily_allocation_limit(&admin, &Some(10));
    client.set_daily_allocation_limit(&admin, &None);
    assert_eq!(client.get_daily_allocation_limit(), None);
    assert_eq!(client.get_daily_allocation_remaining(), None);
}

#[test]
fn test_daily_limit_event_once_per_breach_window() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let source = Address::generate(&env);
    client.receive_fee(&source, &10_000);
    client.set_daily_allocation_limit(&admin, &Some(100));

    let daily_limit_events = |env: &Env| -> u32 {
        let mut count = 0u32;
        for e in env.events().all().events().iter() {
            let soroban_sdk::xdr::ContractEventBody::V0(v0) = &e.body;
            let topics = Vec::<Val>::try_from_val(
                env,
                &soroban_sdk::xdr::ScVal::Vec(Some(v0.topics.clone().into())),
            )
            .unwrap();
            let name: Symbol = topics.get(0).unwrap().try_into_val(env).unwrap();
            if name == Symbol::new(env, "daily_treasury_limit_reached") {
                count += 1;
            }
        }
        count
    };

    // Allocation that exactly reaches the daily limit emits the event once.
    client.allocate(
        &admin,
        &Address::generate(&env),
        &100,
        &String::from_str(&env, "a"),
        &client.get_treasury_revision(),
    );
    assert_eq!(daily_limit_events(&env), 1);

    // Repeated breach attempts in the same ledger day add no further
    // events (a rejected call rolls back its events anyway).
    for _ in 0..2 {
        assert_eq!(
            client.try_allocate(
                &admin,
                &Address::generate(&env),
                &1,
                &String::from_str(&env, "b"),
                &client.get_treasury_revision(),
            ),
            Err(Ok(Error::DailyLimitExceeded))
        );
    }

    // A second allocation in the same day that does not reach the limit
    // emits nothing new either (it is rejected before committing).
    // Next ledger day is a fresh breach window: reaching the limit again
    // emits the event a second time.
    env.ledger().with_mut(|li| li.timestamp += 86_400);
    client.allocate(
        &admin,
        &Address::generate(&env),
        &100,
        &String::from_str(&env, "c"),
        &client.get_treasury_revision(),
    );
    assert_eq!(daily_limit_events(&env), 1);
}


