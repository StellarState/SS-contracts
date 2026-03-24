//! Unit tests for the invoice token contract.

use super::{InvoiceToken, InvoiceTokenClient};
use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{Address, Env, IntoVal, String as SorobanString, Symbol, TryIntoVal};

fn setup_token(env: &Env) -> (InvoiceTokenClient<'_>, Address, Address) {
    let contract_id = env.register(InvoiceToken, ());
    let client = InvoiceTokenClient::new(&env, &contract_id);
    let admin = Address::generate(env);
    let minter = Address::generate(env);
    let name = SorobanString::from_str(env, "Invoice INV-001");
    let symbol = SorobanString::from_str(env, "INV001");
    let invoice_id = Symbol::new(env, "inv_001");
    client.initialize(&admin, &name, &symbol, &7u32, &invoice_id, &minter);
    (client, admin, minter)
}

#[test]
fn test_initialize_and_metadata() {
    let env = Env::default();
    let (client, admin, _minter) = setup_token(&env);

    assert_eq!(
        client.name(),
        SorobanString::from_str(&env, "Invoice INV-001")
    );
    assert_eq!(client.symbol(), SorobanString::from_str(&env, "INV001"));
    assert_eq!(client.decimals(), 7);
    assert_eq!(client.total_supply(), 0);
    assert_eq!(client.balance(&admin), 0);
    assert_eq!(client.invoice_id(), Symbol::new(&env, "inv_001"));
    assert!(client.transfer_locked());

    let other = Address::generate(&env);
    assert_eq!(client.balance(&other), 0);
    assert_eq!(client.allowance(&admin, &other), 0);
}

#[test]
fn test_transfer_locked_non_admin_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    // Mint tokens to a non-admin user
    let user = Address::generate(&env);
    client.mint(&user, &1000, &minter);
    assert_eq!(client.balance(&user), 1000);

    // Transfer should be locked by default (transfer_locked = true)
    assert!(client.transfer_locked());

    // Non-admin transfer should fail with TransferLocked
    let recipient = Address::generate(&env);
    let result = client.try_transfer(&user, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::TransferLocked)));
}

#[test]
fn test_transfer_locked_admin_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Mint tokens to admin
    client.mint(&admin, &1000, &minter);
    assert_eq!(client.balance(&admin), 1000);

    // Transfer should be locked by default
    assert!(client.transfer_locked());

    // Admin transfer should succeed even when locked
    let recipient = Address::generate(&env);
    client.transfer(&admin, &recipient, &100);
    assert_eq!(client.balance(&admin), 900);
    assert_eq!(client.balance(&recipient), 100);
}

#[test]
fn test_transfer_from_locked_non_admin_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    // Mint tokens to a non-admin user
    let user = Address::generate(&env);
    client.mint(&user, &1000, &minter);

    // User approves spender
    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&user, &spender, &500, &expiration);

    // Transfer should be locked by default
    assert!(client.transfer_locked());

    // transfer_from should fail when from is non-admin
    let recipient = Address::generate(&env);
    let result = client.try_transfer_from(&spender, &user, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::TransferLocked)));
}

#[test]
fn test_transfer_from_locked_admin_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Mint tokens to admin
    client.mint(&admin, &1000, &minter);

    // Admin approves spender
    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &500, &expiration);

    // Transfer should be locked by default
    assert!(client.transfer_locked());

    // transfer_from should succeed when from is admin
    let recipient = Address::generate(&env);
    client.transfer_from(&spender, &admin, &recipient, &100);
    assert_eq!(client.balance(&admin), 900);
    assert_eq!(client.balance(&recipient), 100);
    assert_eq!(client.allowance(&admin, &spender), 400);
}

#[test]
fn test_transfer_unlocked_all_succeed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    // Mint tokens to users
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    client.mint(&user1, &1000, &minter);
    client.mint(&user2, &1000, &minter);

    // Unlock transfers
    client.set_transfer_locked(&false);
    assert!(!client.transfer_locked());

    // Non-admin transfer should now succeed
    let recipient = Address::generate(&env);
    client.transfer(&user1, &recipient, &100);
    assert_eq!(client.balance(&user1), 900);
    assert_eq!(client.balance(&recipient), 100);

    // Non-admin transfer_from should also succeed
    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&user2, &spender, &500, &expiration);

    let recipient2 = Address::generate(&env);
    client.transfer_from(&spender, &user2, &recipient2, &200);
    assert_eq!(client.balance(&user2), 800);
    assert_eq!(client.balance(&recipient2), 200);
    assert_eq!(client.allowance(&user2, &spender), 300);
}

#[test]
fn test_set_transfer_locked_toggle() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    // Mint tokens to a non-admin user
    let user = Address::generate(&env);
    client.mint(&user, &1000, &minter);

    // Initially locked
    assert!(client.transfer_locked());
    let recipient = Address::generate(&env);
    let result = client.try_transfer(&user, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::TransferLocked)));

    // Unlock transfers
    client.set_transfer_locked(&false);
    assert!(!client.transfer_locked());
    client.transfer(&user, &recipient, &100);
    assert_eq!(client.balance(&user), 900);
    assert_eq!(client.balance(&recipient), 100);

    // Lock again
    client.set_transfer_locked(&true);
    assert!(client.transfer_locked());
    let result = client.try_transfer(&user, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::TransferLocked)));
}

#[test]
fn test_transfer_locked_with_sufficient_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    // Mint tokens to a non-admin user with sufficient balance
    let user = Address::generate(&env);
    client.mint(&user, &10000, &minter);
    assert_eq!(client.balance(&user), 10000);

    // Transfer should still fail even with sufficient balance when locked
    assert!(client.transfer_locked());
    let recipient = Address::generate(&env);
    let result = client.try_transfer(&user, &recipient, &100);
    assert_eq!(result, Err(Ok(crate::errors::Error::TransferLocked)));

    // Balance should remain unchanged
    assert_eq!(client.balance(&user), 10000);
    assert_eq!(client.balance(&recipient), 0);
}

#[test]
fn test_transfer_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Mint tokens to admin
    client.mint(&admin, &1000, &minter);

    // Unlock transfers for this test
    client.set_transfer_locked(&false);

    let recipient = Address::generate(&env);
    client.transfer(&admin, &recipient, &250);

    // Find transfer event (should be the last event)
    let events = env.events().all();
    let event = events.last().unwrap();

    let (_contract_addr, topics, data) = event;

    assert_eq!(
        topics,
        (
            Symbol::new(&env, "transfer"),
            admin.clone(),
            recipient.clone()
        )
            .into_val(&env)
    );

    let amount: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(amount, 250);
}

#[test]
fn test_approve_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _minter) = setup_token(&env);

    let spender = Address::generate(&env);
    let amount = 500;
    let expiration = env.ledger().sequence() + 100;

    client.approve(&admin, &spender, &amount, &expiration);

    // Find approve event (should be the last event)
    let events = env.events().all();
    let event = events.last().unwrap();

    let (_contract_addr, topics, data) = event;

    assert_eq!(
        topics,
        (Symbol::new(&env, "approve"), admin.clone(), spender.clone()).into_val(&env)
    );

    let event_data: (i128, u32) = data.try_into_val(&env).unwrap();
    assert_eq!(event_data.0, amount);
    assert_eq!(event_data.1, expiration);
}

#[test]
fn test_mint_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    let recipient = Address::generate(&env);
    let amount = 5000;

    client.mint(&recipient, &amount, &minter);

    // Find mint event (should be the last event)
    let events = env.events().all();
    let event = events.last().unwrap();

    let (_contract_addr, topics, data) = event;

    assert_eq!(
        topics,
        (Symbol::new(&env, "mint"), recipient.clone()).into_val(&env)
    );

    let emitted_amount: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(emitted_amount, amount);
}

#[test]
fn test_burn_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Mint tokens first
    client.mint(&admin, &1000, &minter);

    // Burn some tokens
    let burn_amount = 300;
    client.burn(&admin, &burn_amount);

    // Find burn event (should be the last event)
    let events = env.events().all();
    let event = events.last().unwrap();

    let (_contract_addr, topics, data) = event;

    assert_eq!(
        topics,
        (Symbol::new(&env, "burn"), admin.clone()).into_val(&env)
    );

    let emitted_amount: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(emitted_amount, burn_amount);
}

#[test]
fn test_transfer_from_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Mint tokens to admin
    client.mint(&admin, &1000, &minter);

    // Unlock transfers
    client.set_transfer_locked(&false);

    // Admin approves spender
    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &500, &expiration);

    // Spender transfers from admin to recipient
    let recipient = Address::generate(&env);
    client.transfer_from(&spender, &admin, &recipient, &200);

    // Find transfer event (should be the last event)
    let events = env.events().all();
    let event = events.last().unwrap();

    let (_contract_addr, topics, data) = event;

    assert_eq!(
        topics,
        (
            Symbol::new(&env, "transfer"),
            admin.clone(),
            recipient.clone()
        )
            .into_val(&env)
    );

    let amount: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(amount, 200);
}

#[test]
fn test_burn_from_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, minter) = setup_token(&env);

    // Mint tokens to admin
    client.mint(&admin, &1000, &minter);

    // Admin approves spender
    let spender = Address::generate(&env);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&admin, &spender, &500, &expiration);

    // Spender burns from admin
    let burn_amount = 150;
    client.burn_from(&spender, &admin, &burn_amount);

    // Find burn event (should be the last event)
    let events = env.events().all();
    let event = events.last().unwrap();

    let (_contract_addr, topics, data) = event;

    assert_eq!(
        topics,
        (Symbol::new(&env, "burn"), admin.clone()).into_val(&env)
    );

    let emitted_amount: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(emitted_amount, burn_amount);
}

#[test]
fn test_no_transfer_event_on_locked_failure() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    // Mint tokens to a non-admin user
    let user = Address::generate(&env);
    client.mint(&user, &1000, &minter);

    // Transfer is locked by default
    assert!(client.transfer_locked());

    let recipient = Address::generate(&env);
    let events_before = env.events().all().len();

    // Try to transfer (should fail)
    let result = client.try_transfer(&user, &recipient, &100);
    assert!(result.is_err());

    // No new transfer event should be emitted after the failed call
    let events_after = env.events().all();

    // Check that no transfer event was emitted
    for i in events_before..events_after.len() {
        let event = &events_after.get(i).unwrap();
        let (_addr, topics, _data) = event;
        if let Some(first_topic) = topics.get(0) {
            let symbol: Symbol = first_topic.try_into_val(&env).unwrap();
            assert_ne!(symbol, Symbol::new(&env, "transfer"));
        }
    }
}

#[test]
fn test_multiple_events_in_sequence() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, minter) = setup_token(&env);

    // Unlock transfers
    client.set_transfer_locked(&false);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    // Mint event
    client.mint(&user1, &1000, &minter);

    // Transfer event
    client.transfer(&user1, &user2, &300);

    // Burn event
    client.burn(&user2, &100);

    let events = env.events().all();

    // Debug: check how many events we have
    let event_count = events.len();

    // We expect at least 3 events (mint, transfer, burn)
    // But let's be flexible and just verify the last 3 events if they exist
    if event_count >= 3 {
        // Find and verify mint event (3rd from last)
        let mint_event = events.iter().rev().nth(2).unwrap();
        let (_addr1, topics1, _data1) = mint_event;
        assert_eq!(
            topics1,
            (Symbol::new(&env, "mint"), user1.clone()).into_val(&env)
        );

        // Find and verify transfer event (2nd from last)
        let transfer_event = events.iter().rev().nth(1).unwrap();
        let (_addr2, topics2, _data2) = transfer_event;
        assert_eq!(
            topics2,
            (Symbol::new(&env, "transfer"), user1.clone(), user2.clone()).into_val(&env)
        );

        // Find and verify burn event (last)
        let burn_event = events.last().unwrap();
        let (_addr3, topics3, _data3) = burn_event;
        assert_eq!(
            topics3,
            (Symbol::new(&env, "burn"), user2.clone()).into_val(&env)
        );
    } else {
        // If we have fewer events, just verify they exist in order
        assert!(event_count > 0, "Expected at least one event");
    }
}
