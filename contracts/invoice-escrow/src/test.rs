#![cfg(test)]
#![allow(deprecated)]

use super::*;
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as AssetClient;
use soroban_sdk::{contract, contractimpl, Address, Env, IntoVal, Symbol, TryIntoVal};

#[contract]
struct MockInvoiceToken;

#[contractimpl]
impl MockInvoiceToken {
    pub fn mint(env: Env, to: Address, amount: i128, _by: Address) {
        // Just mock the mint call
        env.storage().instance().set(&to, &amount);
    }
}

#[test]
fn test_create_and_fund() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);

    // Register the payment token
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = TokenClient::new(&env, &payment_token_id.address());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());

    // Register our mock invoice token
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    // Initialize escrow contract
    escrow_client.initialize(&admin, &300); // 3% fee

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV123");
    let amount = 1000;

    // Buyer gets payment tokens
    payment_token_asset.mint(&buyer, &2000);

    // Create escrow
    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &amount,
        &1000000,
        &payment_token.address,
        &inv_token_id,
    );

    // Fund escrow
    escrow_client.fund_escrow(&invoice_id, &buyer);

    // Check status
    let status = escrow_client.get_escrow_status(&invoice_id);
    assert_eq!(status, EscrowStatus::Funded);

    // Check tokens transferred to escrow
    assert_eq!(payment_token.balance(&escrow_id), 1000);
    assert_eq!(payment_token.balance(&buyer), 1000);
}

#[test]
fn test_record_payment() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);

    // Register the payment token
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = TokenClient::new(&env, &payment_token_id.address());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());

    // Register our mock invoice token
    let inv_token_id = env.register(MockInvoiceToken, ());

    // Initialize escrow contract (300 bps = 3% fee)
    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV456");
    let amount = 1000;

    // Buyer gets payment tokens for funding
    payment_token_asset.mint(&buyer, &1000);
    // Payer gets payment tokens for settling
    payment_token_asset.mint(&payer, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &amount,
        &1000000,
        &payment_token.address,
        &inv_token_id,
    );

    escrow_client.fund_escrow(&invoice_id, &buyer);
    assert_eq!(payment_token.balance(&buyer), 0);

    // The contract holds the buyer's 1000
    assert_eq!(payment_token.balance(&escrow_id), 1000);

    // Now testing record_payment
    escrow_client.record_payment(&invoice_id, &payer, &amount);

    // Status must be Settled
    let status = escrow_client.get_escrow_status(&invoice_id);
    assert_eq!(status, EscrowStatus::Settled);

    // Payer should have spent 1000
    assert_eq!(payment_token.balance(&payer), 0);

    // contract receives 1000 from payer. Then distributes to funder (970) and admin (30).
    // Initial contract balance (from fund_escrow): 1000.
    // + 1000 from record_payment = 2000.
    // - 1000 distributed = 1000.
    assert_eq!(payment_token.balance(&escrow_id), 1000); // 1000 remains (the investor's initial funding)

    assert_eq!(payment_token.balance(&buyer), 970);
    assert_eq!(payment_token.balance(&admin), 30);
}

#[test]
fn test_escrow_created_event() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV789");
    let amount = 5000;
    let due_date = 2000000;

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &amount,
        &due_date,
        &payment_token_id.address(),
        &inv_token_id,
    );

    // Assert escrow_created event was emitted
    let events = env.events().all();
    let event = events.last().unwrap();

    // Event tuple is (contract_address, topics, data)
    let (_contract_addr, topics, data) = event;

    assert_eq!(
        topics,
        (Symbol::new(&env, "escrow_created"),).into_val(&env)
    );

    let event_data: (Symbol, Address, i128, u64, Address, Address) =
        data.try_into_val(&env).unwrap();
    assert_eq!(event_data.0, invoice_id);
    assert_eq!(event_data.1, seller);
    assert_eq!(event_data.2, amount);
    assert_eq!(event_data.3, due_date);
    assert_eq!(event_data.4, payment_token_id.address());
    assert_eq!(event_data.5, inv_token_id);
}

#[test]
fn test_escrow_funded_event() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV999");
    let amount = 3000;

    payment_token_asset.mint(&buyer, &3000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &amount,
        &1000000,
        &payment_token_id.address(),
        &inv_token_id,
    );

    escrow_client.fund_escrow(&invoice_id, &buyer);

    // Find escrow_funded event (should be the last event)
    let events = env.events().all();
    let event = events.last().unwrap();

    let (_contract_addr, topics, data) = event;

    assert_eq!(topics, (Symbol::new(&env, "escrow_funded"),).into_val(&env));

    let event_data: (Symbol, Address, i128) = data.try_into_val(&env).unwrap();
    assert_eq!(event_data.0, invoice_id);
    assert_eq!(event_data.1, buyer);
    assert_eq!(event_data.2, amount);
}

#[test]
fn test_payment_settled_event() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register(MockInvoiceToken, ());

    escrow_client.initialize(&admin, &300); // 3% fee

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV111");
    let amount = 1000;

    payment_token_asset.mint(&buyer, &1000);
    payment_token_asset.mint(&payer, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &amount,
        &1000000,
        &payment_token_id.address(),
        &inv_token_id,
    );

    escrow_client.fund_escrow(&invoice_id, &buyer);
    escrow_client.record_payment(&invoice_id, &payer, &amount);

    // Find payment_settled event (should be the last event)
    let events = env.events().all();
    let event = events.last().unwrap();

    let (_contract_addr, topics, data) = event;

    assert_eq!(
        topics,
        (Symbol::new(&env, "payment_settled"),).into_val(&env)
    );

    let event_data: (Symbol, i128, i128, i128) = data.try_into_val(&env).unwrap();
    assert_eq!(event_data.0, invoice_id);
    assert_eq!(event_data.1, amount); // total amount
    assert_eq!(event_data.2, 30); // platform_fee (3% of 1000)
    assert_eq!(event_data.3, 970); // investor_amount (1000 - 30)
}

#[test]
fn test_escrow_refunded_event() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV222");
    let amount = 2000;
    let due_date = 1000;

    payment_token_asset.mint(&buyer, &2000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &amount,
        &due_date,
        &payment_token_id.address(),
        &inv_token_id,
    );

    escrow_client.fund_escrow(&invoice_id, &buyer);

    // Set ledger timestamp past due date to allow refund
    env.ledger().with_mut(|li| li.timestamp = due_date + 1);

    escrow_client.refund(&invoice_id);

    // Find escrow_refunded event (should be the last event)
    let events = env.events().all();
    let event = events.last().unwrap();

    let (_contract_addr, topics, data) = event;

    assert_eq!(
        topics,
        (Symbol::new(&env, "escrow_refunded"),).into_val(&env)
    );

    let event_data: (Symbol, Address, i128) = data.try_into_val(&env).unwrap();
    assert_eq!(event_data.0, invoice_id);
    assert_eq!(event_data.1, buyer);
    assert_eq!(event_data.2, amount);
}

#[test]
fn test_no_settlement_event_on_invalid_state() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register(MockInvoiceToken, ());

    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV333");
    let amount = 1000;

    payment_token_asset.mint(&payer, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &amount,
        &1000000,
        &payment_token_id.address(),
        &inv_token_id,
    );

    // Try to record payment without funding first (should fail)
    let result = escrow_client.try_record_payment(&invoice_id, &payer, &amount);

    // Should fail with AlreadySettled error (status is Created, not Funded)
    assert!(result.is_err());

    // Verify no payment_settled event was emitted by checking all events
    let all_events = env.events().all();
    for event in all_events.iter() {
        let (_addr, topics, _data) = event;
        // Check if this is a payment_settled event
        let topic_vec: soroban_sdk::Vec<soroban_sdk::Val> = topics.clone();
        if topic_vec.len() > 0 {
            if let Ok(symbol) = topic_vec.get(0).unwrap().try_into_val(&env) {
                let sym: Symbol = symbol;
                // Assert that no payment_settled event exists
                assert_ne!(sym, Symbol::new(&env, "payment_settled"));
            }
        }
    }
}

#[test]
fn test_no_refund_event_on_invalid_state() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV444");
    let amount = 1000;
    let due_date = 1000;

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &amount,
        &due_date,
        &payment_token_id.address(),
        &inv_token_id,
    );

    // Set ledger timestamp past due date
    env.ledger().with_mut(|li| li.timestamp = due_date + 1);

    // Try to refund without funding first (should fail)
    let result = escrow_client.try_refund(&invoice_id);

    // Should fail with RefundNotAllowed error (status is Created, not Funded)
    assert!(result.is_err());

    // Verify no escrow_refunded event was emitted by checking all events
    let all_events = env.events().all();
    for event in all_events.iter() {
        let (_addr, topics, _data) = event;
        // Check if this is an escrow_refunded event
        let topic_vec: soroban_sdk::Vec<soroban_sdk::Val> = topics.clone();
        if topic_vec.len() > 0 {
            if let Ok(symbol) = topic_vec.get(0).unwrap().try_into_val(&env) {
                let sym: Symbol = symbol;
                // Assert that no escrow_refunded event exists
                assert_ne!(sym, Symbol::new(&env, "escrow_refunded"));
            }
        }
    }
}
