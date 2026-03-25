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

// ========== Authorization Tests ==========

#[test]
fn test_initialize_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);

    // First initialization should succeed
    escrow_client.initialize(&admin, &300);

    // Second initialization should fail
    let result = escrow_client.try_initialize(&admin, &500);
    assert_eq!(result, Err(Ok(Error::AlreadyInit)));
}

#[test]
fn test_create_escrow_requires_seller_auth() {
    let env = Env::default();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);

    escrow_client.initialize(&admin, &300);

    // Without auth, should fail
    let result = escrow_client.try_create_escrow(
        &Symbol::new(&env, "INV001"),
        &seller,
        &1000,
        &1000000,
        &payment_token,
        &inv_token,
    );
    assert!(result.is_err());
}

#[test]
fn test_update_platform_fee_requires_admin_auth() {
    let env = Env::default();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    escrow_client.initialize(&admin, &300);

    // Without auth, should fail
    let result = escrow_client.try_update_platform_fee_bps(&500);
    assert!(result.is_err());
}

// ========== Invalid Input Tests ==========

#[test]
fn test_initialize_invalid_fee_bps() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);

    // Fee > 10000 bps (100%) should fail
    let result = escrow_client.try_initialize(&admin, &10001);
    assert_eq!(result, Err(Ok(Error::InvalidFeeBps)));
}

#[test]
fn test_create_escrow_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);

    escrow_client.initialize(&admin, &300);

    // Zero amount should fail
    let result = escrow_client.try_create_escrow(
        &Symbol::new(&env, "INV001"),
        &seller,
        &0,
        &1000000,
        &payment_token,
        &inv_token,
    );
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_create_escrow_negative_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);

    escrow_client.initialize(&admin, &300);

    // Negative amount should fail
    let result = escrow_client.try_create_escrow(
        &Symbol::new(&env, "INV001"),
        &seller,
        &-100,
        &1000000,
        &payment_token,
        &inv_token,
    );
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_create_escrow_duplicate_invoice_id() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");

    escrow_client.initialize(&admin, &300);

    // First create should succeed
    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &1000,
        &1000000,
        &payment_token,
        &inv_token,
    );

    // Second create with same invoice_id should fail
    let result = escrow_client.try_create_escrow(
        &invoice_id,
        &seller,
        &2000,
        &2000000,
        &payment_token,
        &inv_token,
    );
    assert_eq!(result, Err(Ok(Error::EscrowExists)));
}

#[test]
fn test_record_payment_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");

    escrow_client.initialize(&admin, &300);

    // Zero amount should fail
    let result = escrow_client.try_record_payment(&invoice_id, &payer, &0);
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_update_platform_fee_invalid_bps() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    escrow_client.initialize(&admin, &300);

    // Fee > 10000 bps should fail
    let result = escrow_client.try_update_platform_fee_bps(&10001);
    assert_eq!(result, Err(Ok(Error::InvalidFeeBps)));
}

// ========== State Transition Tests ==========

#[test]
fn test_fund_escrow_not_found() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);

    escrow_client.initialize(&admin, &300);

    // Try to fund non-existent escrow
    let result = escrow_client.try_fund_escrow(&Symbol::new(&env, "NONEXISTENT"), &buyer);
    assert_eq!(result, Err(Ok(Error::EscrowNotFound)));
}

#[test]
fn test_fund_escrow_already_funded() {
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
    let buyer1 = Address::generate(&env);
    let buyer2 = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");

    payment_token_asset.mint(&buyer1, &1000);
    payment_token_asset.mint(&buyer2, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &1000,
        &1000000,
        &payment_token_id.address(),
        &inv_token_id,
    );

    // First funding should succeed
    escrow_client.fund_escrow(&invoice_id, &buyer1);

    // Second funding should fail
    let result = escrow_client.try_fund_escrow(&invoice_id, &buyer2);
    assert_eq!(result, Err(Ok(Error::EscrowFunded)));
}

#[test]
fn test_record_payment_not_funded() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payer = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");

    escrow_client.initialize(&admin, &300);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &1000,
        &1000000,
        &payment_token,
        &inv_token,
    );

    // Try to record payment without funding first
    let result = escrow_client.try_record_payment(&invoice_id, &payer, &1000);
    assert_eq!(result, Err(Ok(Error::AlreadySettled)));
}

#[test]
fn test_record_payment_already_settled() {
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
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");

    payment_token_asset.mint(&buyer, &1000);
    payment_token_asset.mint(&payer, &2000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &1000,
        &1000000,
        &payment_token_id.address(),
        &inv_token_id,
    );

    escrow_client.fund_escrow(&invoice_id, &buyer);
    escrow_client.record_payment(&invoice_id, &payer, &1000);

    // Try to record payment again
    let result = escrow_client.try_record_payment(&invoice_id, &payer, &1000);
    assert_eq!(result, Err(Ok(Error::AlreadySettled)));
}

#[test]
fn test_record_payment_amount_exceeds_escrow() {
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
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");

    payment_token_asset.mint(&buyer, &1000);
    payment_token_asset.mint(&payer, &2000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &1000,
        &1000000,
        &payment_token_id.address(),
        &inv_token_id,
    );

    escrow_client.fund_escrow(&invoice_id, &buyer);

    // Try to record payment with amount > escrow amount
    let result = escrow_client.try_record_payment(&invoice_id, &payer, &1500);
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_refund_not_funded() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");

    escrow_client.initialize(&admin, &300);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &1000,
        &1000,
        &payment_token,
        &inv_token,
    );

    // Set time past due date
    env.ledger().with_mut(|li| li.timestamp = 2000);

    // Try to refund without funding first
    let result = escrow_client.try_refund(&invoice_id);
    assert_eq!(result, Err(Ok(Error::RefundNotAllowed)));
}

// ========== Refund Timing Tests ==========

#[test]
fn test_refund_before_due_date() {
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
    let invoice_id = Symbol::new(&env, "INV001");
    let due_date = 10000;

    payment_token_asset.mint(&buyer, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &1000,
        &due_date,
        &payment_token_id.address(),
        &inv_token_id,
    );

    escrow_client.fund_escrow(&invoice_id, &buyer);

    // Set time before due date
    env.ledger().with_mut(|li| li.timestamp = due_date - 1);

    // Refund should fail
    let result = escrow_client.try_refund(&invoice_id);
    assert_eq!(result, Err(Ok(Error::RefundNotAllowed)));
}

#[test]
fn test_refund_at_due_date() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = TokenClient::new(&env, &payment_token_id.address());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");
    let due_date = 10000;

    payment_token_asset.mint(&buyer, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &1000,
        &due_date,
        &payment_token_id.address(),
        &inv_token_id,
    );

    escrow_client.fund_escrow(&invoice_id, &buyer);

    // Set time exactly at due date
    env.ledger().with_mut(|li| li.timestamp = due_date);

    // Refund should succeed
    escrow_client.refund(&invoice_id);

    // Verify buyer got refund
    assert_eq!(payment_token.balance(&buyer), 1000);
    assert_eq!(
        escrow_client.get_escrow_status(&invoice_id),
        EscrowStatus::Refunded
    );
}

#[test]
fn test_refund_after_due_date() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = TokenClient::new(&env, &payment_token_id.address());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    escrow_client.initialize(&admin, &300);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");
    let due_date = 10000;

    payment_token_asset.mint(&buyer, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &1000,
        &due_date,
        &payment_token_id.address(),
        &inv_token_id,
    );

    escrow_client.fund_escrow(&invoice_id, &buyer);

    // Set time after due date
    env.ledger().with_mut(|li| li.timestamp = due_date + 5000);

    // Refund should succeed
    escrow_client.refund(&invoice_id);

    // Verify buyer got refund
    assert_eq!(payment_token.balance(&buyer), 1000);
    assert_eq!(
        escrow_client.get_escrow_status(&invoice_id),
        EscrowStatus::Refunded
    );
}

#[test]
fn test_refund_already_settled() {
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
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");
    let due_date = 10000;

    payment_token_asset.mint(&buyer, &1000);
    payment_token_asset.mint(&payer, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &1000,
        &due_date,
        &payment_token_id.address(),
        &inv_token_id,
    );

    escrow_client.fund_escrow(&invoice_id, &buyer);
    escrow_client.record_payment(&invoice_id, &payer, &1000);

    // Set time after due date
    env.ledger().with_mut(|li| li.timestamp = due_date + 1);

    // Try to refund after settlement
    let result = escrow_client.try_refund(&invoice_id);
    assert_eq!(result, Err(Ok(Error::RefundNotAllowed)));
}

// ========== Fee Calculation Tests ==========

#[test]
fn test_fee_calculation_zero_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = TokenClient::new(&env, &payment_token_id.address());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register(MockInvoiceToken, ());

    // Initialize with 0% fee
    escrow_client.initialize(&admin, &0);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");

    payment_token_asset.mint(&buyer, &1000);
    payment_token_asset.mint(&payer, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &1000,
        &1000000,
        &payment_token_id.address(),
        &inv_token_id,
    );

    escrow_client.fund_escrow(&invoice_id, &buyer);
    escrow_client.record_payment(&invoice_id, &payer, &1000);

    // With 0% fee, buyer should get full amount
    assert_eq!(payment_token.balance(&buyer), 1000);
    assert_eq!(payment_token.balance(&admin), 0);
}

#[test]
fn test_fee_calculation_max_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = TokenClient::new(&env, &payment_token_id.address());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());
    let inv_token_id = env.register(MockInvoiceToken, ());

    // Initialize with 100% fee (10000 bps)
    escrow_client.initialize(&admin, &10000);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");

    payment_token_asset.mint(&buyer, &1000);
    payment_token_asset.mint(&payer, &1000);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &1000,
        &1000000,
        &payment_token_id.address(),
        &inv_token_id,
    );

    escrow_client.fund_escrow(&invoice_id, &buyer);
    escrow_client.record_payment(&invoice_id, &payer, &1000);

    // With 100% fee, admin gets all, buyer gets nothing
    assert_eq!(payment_token.balance(&buyer), 0);
    assert_eq!(payment_token.balance(&admin), 1000);
}

#[test]
fn test_update_platform_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);

    escrow_client.initialize(&admin, &300);

    // Verify initial fee
    let config = escrow_client.get_config();
    assert_eq!(config.fee_bps, 300);

    // Update fee
    escrow_client.update_platform_fee_bps(&500);

    // Verify updated fee
    let config = escrow_client.get_config();
    assert_eq!(config.fee_bps, 500);
}

// ========== View Function Tests ==========

#[test]
fn test_get_escrow_not_found() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    escrow_client.initialize(&admin, &300);

    // Try to get non-existent escrow
    let result = escrow_client.try_get_escrow(&Symbol::new(&env, "NONEXISTENT"));
    assert_eq!(result, Err(Ok(Error::EscrowNotFound)));
}

#[test]
fn test_get_config_not_initialized() {
    let env = Env::default();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    // Try to get config before initialization
    let result = escrow_client.try_get_config();
    assert_eq!(result, Err(Ok(Error::NotInit)));
}

#[test]
fn test_get_escrow_status_not_found() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    escrow_client.initialize(&admin, &300);

    // Try to get status of non-existent escrow
    let result = escrow_client.try_get_escrow_status(&Symbol::new(&env, "NONEXISTENT"));
    assert_eq!(result, Err(Ok(Error::EscrowNotFound)));
}

#[test]
fn test_get_escrow_data() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV001");
    let amount = 1000;
    let due_date = 1000000;

    escrow_client.initialize(&admin, &300);

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &amount,
        &due_date,
        &payment_token,
        &inv_token,
    );

    // Get escrow data and verify
    let data = escrow_client.get_escrow(&invoice_id);
    assert_eq!(data.inv_id, invoice_id);
    assert_eq!(data.seller, seller);
    assert_eq!(data.amount, amount);
    assert_eq!(data.due_dt, due_date);
    assert_eq!(data.token, payment_token);
    assert_eq!(data.inv_token, inv_token);
    assert_eq!(data.status, EscrowStatus::Created);
    assert_eq!(data.funder, None);
}

// ========== Operations Before Initialization Tests ==========

#[test]
fn test_create_escrow_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let seller = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let inv_token = Address::generate(&env);

    // Try to create escrow without initialization
    let result = escrow_client.try_create_escrow(
        &Symbol::new(&env, "INV001"),
        &seller,
        &1000,
        &1000000,
        &payment_token,
        &inv_token,
    );
    assert_eq!(result, Err(Ok(Error::NotInit)));
}

#[test]
fn test_update_fee_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    // Try to update fee without initialization
    let result = escrow_client.try_update_platform_fee_bps(&500);
    assert_eq!(result, Err(Ok(Error::NotInit)));
}
