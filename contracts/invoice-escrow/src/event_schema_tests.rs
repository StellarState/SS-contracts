//! Event Schema Snapshot Tests
//!
//! These tests capture the exact topic and data shape for each public mutation event.
//! They fail if a topic is renamed, a field is removed, or field order changes.
//!
//! Run with: cargo test event_schema_snapshot -- --nocapture
//!
//! To update snapshots after intentional schema changes:
//! 1. Run the test to see the diff
//! 2. Update the expected JSON in test_snapshots/events/
//! 3. Commit the updated snapshots

#![cfg(test)]
#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use crate::events::*;
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{
    contract, contractimpl, Address, BytesN, Env, IntoVal, Symbol, TryFromVal, Val, Vec,
};

/// Mock invoice token for testing
#[contract]
struct MockInvoiceToken;

#[contractimpl]
impl MockInvoiceToken {
    pub fn mint(env: Env, to: Address, amount: i128, _by: Address) {
        env.storage().instance().set(&to, &amount);
    }

    pub fn set_transfer_locked(_env: Env, _caller: Address, _locked: bool) {}

    pub fn decimals(_env: Env) -> u32 {
        7
    }
}

/// Helper to create a test commitment hash
fn test_commitment(env: &Env, data: &str) -> BytesN<32> {
    let mut array = [0u8; 32];
    let bytes = data.as_bytes();
    let len = bytes.len().min(32);
    array[..len].copy_from_slice(&bytes[..len]);
    BytesN::from_array(env, &array)
}

/// Parse event into structured format for comparison
fn parse_event(env: &Env, event: &soroban_sdk::xdr::ContractEvent) -> (Vec<Val>, Val) {
    let soroban_sdk::xdr::ContractEventBody::V0(v0) = &event.body;
    let topics = Vec::<Val>::try_from_val(
        env,
        &soroban_sdk::xdr::ScVal::Vec(Some(v0.topics.clone().into())),
    )
    .unwrap();
    let data = Val::try_from_val(env, &v0.data).unwrap();
    (topics, data)
}

/// Extract topic names (excluding contract address) from event
fn extract_topic_names(env: &Env, event: &soroban_sdk::xdr::ContractEvent) -> Vec<String> {
    let (topics, _) = parse_event(env, event);
    let mut names = Vec::new(env);
    // Skip first topic (contract address), second is event name
    for i in 1..topics.len() {
        if let Ok(sym) = Symbol::try_from_val(env, &topics.get(i).unwrap()) {
            names.push_back(sym.to_string());
        }
    }
    names
}

/// Extract data field names and types from event
fn extract_data_schema(env: &Env, event: &soroban_sdk::xdr::ContractEvent) -> Vec<(String, String)> {
    let (_, data) = parse_event(env, event);
    let mut fields = Vec::new(env);
    
    // Try to decode as tuple/struct
    if let Ok(vec) = Vec::<Val>::try_from_val(env, &data) {
        for (i, val) in vec.iter().enumerate() {
            let type_name = match val.type_of() {
                soroban_sdk::xdr::ScValType::Bool => "bool",
                soroban_sdk::xdr::ScValType::U32 => "u32",
                soroban_sdk::xdr::ScValType::I64 => "i64",
                soroban_sdk::xdr::ScValType::U64 => "u64",
                soroban_sdk::xdr::ScValType::I128 => "i128",
                soroban_sdk::xdr::ScValType::U128 => "u128",
                soroban_sdk::xdr::ScValType::Address => "Address",
                soroban_sdk::xdr::ScValType::Symbol => "Symbol",
                soroban_sdk::xdr::ScValType::String => "String",
                soroban_sdk::xdr::ScValType::Bytes => "Bytes",
                soroban_sdk::xdr::ScValType::BytesN => "BytesN",
                soroban_sdk::xdr::ScValType::Vec => "Vec",
                soroban_sdk::xdr::ScValType::Map => "Map",
                soroban_sdk::xdr::ScValType::Option => "Option",
                _ => "Unknown",
            };
            fields.push_back((format!("field_{}", i), type_name.to_string()));
        }
    }
    fields
}

#[test]
fn event_schema_snapshot_escrow_created() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let debtor = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);

    let invoice_id = Symbol::new(&env, "INV_EVT");
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = payment_token_id.address();
    let payment_asset = soroban_sdk::token::StellarAssetClient::new(&env, &payment_token_id);
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    payment_asset.mint(&buyer, &1000);
    payment_asset.mint(&payer, &1000);

    client.initialize(&admin, &0);
    client.create_escrow_legacy(
        &invoice_id,
        &seller,
        &debtor,
        &1000,
        &1000,
        &1_000_000,
        &payment_token,
        &inv_token_id,
        &test_commitment(&env, "test"),
        &None,
        &None,
    );

    // Get the emitted events
    let events = env.events().all();
    
    // Find escrow_created event
    let mut found = false;
    for event in events.iter() {
        let (topics, _) = parse_event(&env, &event);
        if topics.len() >= 2 {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(1).unwrap()) {
                if sym == Symbol::new(&env, "escrow_created") {
                    found = true;
                    
                    // Verify topic structure: [contract, "escrow_created", invoice_id]
                    assert!(topics.len() >= 3, "escrow_created should have at least 3 topics");
                    
                    // Verify data structure: (invoice_id, seller, debtor, face_value, purchase_price, due_dt, token, inv_token, commitment, funding_milestone)
                    let (_, data) = parse_event(&env, &event);
                    let data_vec = Vec::<Val>::try_from_val(&env, &data).unwrap();
                    assert_eq!(data_vec.len(), 10, "escrow_created data should have 10 fields");
                    
                    // Verify field types in order
                    assert_eq!(data_vec.get(0).unwrap().type_of(), soroban_sdk::xdr::ScValType::Symbol);
                    assert_eq!(data_vec.get(1).unwrap().type_of(), soroban_sdk::xdr::ScValType::Address);
                    assert_eq!(data_vec.get(2).unwrap().type_of(), soroban_sdk::xdr::ScValType::Address);
                    assert_eq!(data_vec.get(3).unwrap().type_of(), soroban_sdk::xdr::ScValType::I128);
                    assert_eq!(data_vec.get(4).unwrap().type_of(), soroban_sdk::xdr::ScValType::I128);
                    assert_eq!(data_vec.get(5).unwrap().type_of(), soroban_sdk::xdr::ScValType::U64);
                    assert_eq!(data_vec.get(6).unwrap().type_of(), soroban_sdk::xdr::ScValType::Address);
                    assert_eq!(data_vec.get(7).unwrap().type_of(), soroban_sdk::xdr::ScValType::Address);
                    assert_eq!(data_vec.get(8).unwrap().type_of(), soroban_sdk::xdr::ScValType::BytesN);
                    // funding_milestone is Option<i128>
                    
                    break;
                }
            }
        }
    }
    assert!(found, "escrow_created event not found");
}

#[test]
fn event_schema_snapshot_escrow_funded() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let debtor = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);

    let invoice_id = Symbol::new(&env, "INV_EVT2");
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = payment_token_id.address();
    let payment_asset = soroban_sdk::token::StellarAssetClient::new(&env, &payment_token_id);
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    payment_asset.mint(&buyer, &1000);
    payment_asset.mint(&payer, &1000);

    client.initialize(&admin, &0);
    client.create_escrow_legacy(
        &invoice_id,
        &seller,
        &debtor,
        &1000,
        &1000,
        &1_000_000,
        &payment_token,
        &inv_token_id,
        &test_commitment(&env, "test"),
        &None,
        &None,
    );

    // Clear events from create_escrow
    let _ = env.events().all();

    client.fund_escrow_legacy(&invoice_id, &buyer, &1000);

    let events = env.events().all();
    let mut found = false;
    for event in events.iter() {
        let (topics, _) = parse_event(&env, &event);
        if topics.len() >= 2 {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(1).unwrap()) {
                if sym == Symbol::new(&env, "escrow_funded") {
                    found = true;
                    
                    // Verify topic structure: [contract, "escrow_funded", invoice_id]
                    assert!(topics.len() >= 3, "escrow_funded should have at least 3 topics");
                    
                    // Verify data: (invoice_id, funder, amount, funded_amt, purchase_price)
                    let (_, data) = parse_event(&env, &event);
                    let data_vec = Vec::<Val>::try_from_val(&env, &data).unwrap();
                    assert_eq!(data_vec.len(), 5, "escrow_funded data should have 5 fields");
                    
                    assert_eq!(data_vec.get(0).unwrap().type_of(), soroban_sdk::xdr::ScValType::Symbol);
                    assert_eq!(data_vec.get(1).unwrap().type_of(), soroban_sdk::xdr::ScValType::Address);
                    assert_eq!(data_vec.get(2).unwrap().type_of(), soroban_sdk::xdr::ScValType::I128);
                    assert_eq!(data_vec.get(3).unwrap().type_of(), soroban_sdk::xdr::ScValType::I128);
                    assert_eq!(data_vec.get(4).unwrap().type_of(), soroban_sdk::xdr::ScValType::I128);
                    
                    break;
                }
            }
        }
    }
    assert!(found, "escrow_funded event not found");
}

#[test]
fn event_schema_snapshot_payment_settled() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let debtor = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);

    let invoice_id = Symbol::new(&env, "INV_EVT3");
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = payment_token_id.address();
    let payment_asset = soroban_sdk::token::StellarAssetClient::new(&env, &payment_token_id);
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    payment_asset.mint(&buyer, &1000);
    payment_asset.mint(&payer, &1000);

    client.initialize(&admin, &0);
    client.create_escrow_legacy(
        &invoice_id,
        &seller,
        &debtor,
        &1000,
        &1000,
        &1_000_000,
        &payment_token,
        &inv_token_id,
        &test_commitment(&env, "test"),
        &None,
        &None,
    );
    client.fund_escrow_legacy(&invoice_id, &buyer, &1000);

    let _ = env.events().all();

    client.record_payment(&invoice_id, &payer, &1000);

    let events = env.events().all();
    let mut found = false;
    for event in events.iter() {
        let (topics, _) = parse_event(&env, &event);
        if topics.len() >= 2 {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(1).unwrap()) {
                if sym == Symbol::new(&env, "payment_settled") {
                    found = true;
                    
                    // Verify topic structure: [contract, "payment_settled", invoice_id]
                    assert!(topics.len() >= 3, "payment_settled should have at least 3 topics");
                    
                    // Verify data: (invoice_id, amount, platform_fee, investor_amount)
                    let (_, data) = parse_event(&env, &event);
                    let data_vec = Vec::<Val>::try_from_val(&env, &data).unwrap();
                    assert_eq!(data_vec.len(), 4, "payment_settled data should have 4 fields");
                    
                    assert_eq!(data_vec.get(0).unwrap().type_of(), soroban_sdk::xdr::ScValType::Symbol);
                    assert_eq!(data_vec.get(1).unwrap().type_of(), soroban_sdk::xdr::ScValType::I128);
                    assert_eq!(data_vec.get(2).unwrap().type_of(), soroban_sdk::xdr::ScValType::I128);
                    assert_eq!(data_vec.get(3).unwrap().type_of(), soroban_sdk::xdr::ScValType::I128);
                    
                    break;
                }
            }
        }
    }
    assert!(found, "payment_settled event not found");
}

#[test]
fn event_schema_snapshot_escrow_refunded() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let debtor = Address::generate(&env);
    let buyer = Address::generate(&env);

    let invoice_id = Symbol::new(&env, "INV_EVT4");
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = payment_token_id.address();
    let payment_asset = soroban_sdk::token::StellarAssetClient::new(&env, &payment_token_id);
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    payment_asset.mint(&buyer, &1000);

    client.initialize(&admin, &0);
    client.create_escrow_legacy(
        &invoice_id,
        &seller,
        &debtor,
        &1000,
        &1000,
        &1_000_000,
        &payment_token,
        &inv_token_id,
        &test_commitment(&env, "test"),
        &None,
        &None,
    );
    client.fund_escrow_legacy(&invoice_id, &buyer, &1000);

    let _ = env.events().all();

    // Fast-forward time past due date + grace period
    env.ledger().with_mut(|li| {
        li.timestamp = 2_000_000;
    });

    client.refund_escrow(&invoice_id);

    let events = env.events().all();
    let mut found = false;
    for event in events.iter() {
        let (topics, _) = parse_event(&env, &event);
        if topics.len() >= 2 {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(1).unwrap()) {
                if sym == Symbol::new(&env, "escrow_refunded") {
                    found = true;
                    
                    // Verify data: (invoice_id, amount)
                    let (_, data) = parse_event(&env, &event);
                    let data_vec = Vec::<Val>::try_from_val(&env, &data).unwrap();
                    assert_eq!(data_vec.len(), 2, "escrow_refunded data should have 2 fields");
                    
                    assert_eq!(data_vec.get(0).unwrap().type_of(), soroban_sdk::xdr::ScValType::Symbol);
                    assert_eq!(data_vec.get(1).unwrap().type_of(), soroban_sdk::xdr::ScValType::I128);
                    
                    break;
                }
            }
        }
    }
    assert!(found, "escrow_refunded event not found");
}

#[test]
fn event_schema_snapshot_installment_events() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let debtor = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);

    let invoice_id = Symbol::new(&env, "INV_INST");
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = payment_token_id.address();
    let payment_asset = soroban_sdk::token::StellarAssetClient::new(&env, &payment_token_id);
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    payment_asset.mint(&buyer, &2000);
    payment_asset.mint(&payer, &2000);

    client.initialize(&admin, &0);
    client.create_escrow_legacy(
        &invoice_id,
        &seller,
        &debtor,
        &2000,
        &2000,
        &1_000_000,
        &payment_token,
        &inv_token_id,
        &test_commitment(&env, "test"),
        &None,
        &None,
    );
    client.fund_escrow_legacy(&invoice_id, &buyer, &2000);

    // Set installment schedule
    let schedule = soroban_sdk::vec![
        &env,
        crate::types::InstallmentInput { due_ts: 500_000, amount: 1000 },
        crate::types::InstallmentInput { due_ts: 800_000, amount: 1000 },
    ];
    client.set_installment_schedule(&invoice_id, &seller, &schedule);

    let _ = env.events().all();

    // First payment - should trigger installment_settled for first milestone
    client.record_payment(&invoice_id, &payer, &1000);

    let events = env.events().all();
    let mut found_installment_set = false;
    let mut found_installment_settled = false;
    for event in events.iter() {
        let (topics, _) = parse_event(&env, &event);
        if topics.len() >= 2 {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(1).unwrap()) {
                if sym == Symbol::new(&env, "installment_schedule_set") {
                    found_installment_set = true;
                    let (_, data) = parse_event(&env, &event);
                    let data_vec = Vec::<Val>::try_from_val(&env, &data).unwrap();
                    assert_eq!(data_vec.len(), 4, "installment_schedule_set data should have 4 fields");
                } else if sym == Symbol::new(&env, "installment_settled") {
                    found_installment_settled = true;
                    let (_, data) = parse_event(&env, &event);
                    let data_vec = Vec::<Val>::try_from_val(&env, &data).unwrap();
                    assert_eq!(data_vec.len(), 4, "installment_settled data should have 4 fields");
                    assert_eq!(data_vec.get(0).unwrap().type_of(), soroban_sdk::xdr::ScValType::Symbol);
                    assert_eq!(data_vec.get(1).unwrap().type_of(), soroban_sdk::xdr::ScValType::U32);
                    assert_eq!(data_vec.get(2).unwrap().type_of(), soroban_sdk::xdr::ScValType::I128);
                    assert_eq!(data_vec.get(3).unwrap().type_of(), soroban_sdk::xdr::ScValType::I128);
                }
            }
        }
    }
    assert!(found_installment_set, "installment_schedule_set event not found");
    assert!(found_installment_settled, "installment_settled event not found");
}

#[test]
fn event_schema_snapshot_dispute_events() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let debtor = Address::generate(&env);
    let buyer = Address::generate(&env);

    let invoice_id = Symbol::new(&env, "INV_DISP");
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = payment_token_id.address();
    let payment_asset = soroban_sdk::token::StellarAssetClient::new(&env, &payment_token_id);
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    payment_asset.mint(&buyer, &1000);

    client.initialize(&admin, &0);
    client.create_escrow_legacy(
        &invoice_id,
        &seller,
        &debtor,
        &1000,
        &1000,
        &1_000_000,
        &payment_token,
        &inv_token_id,
        &test_commitment(&env, "test"),
        &None,
        &None,
    );
    client.fund_escrow_legacy(&invoice_id, &buyer, &1000);

    let _ = env.events().all();

    // Raise dispute
    client.raise_dispute(&invoice_id, &buyer, &BytesN::from_array(&env, &[1; 32]));

    let events = env.events().all();
    let mut found_dispute_raised = false;
    for event in events.iter() {
        let (topics, _) = parse_event(&env, &event);
        if topics.len() >= 2 {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(1).unwrap()) {
                if sym == Symbol::new(&env, "DisputeRaised") {
                    found_dispute_raised = true;
                    let (_, data) = parse_event(&env, &event);
                    let data_vec = Vec::<Val>::try_from_val(&env, &data).unwrap();
                    assert_eq!(data_vec.len(), 3, "DisputeRaised data should have 3 fields");
                    assert_eq!(data_vec.get(0).unwrap().type_of(), soroban_sdk::xdr::ScValType::Symbol);
                    assert_eq!(data_vec.get(1).unwrap().type_of(), soroban_sdk::xdr::ScValType::Address);
                    assert_eq!(data_vec.get(2).unwrap().type_of(), soroban_sdk::xdr::ScValType::U64);
                }
            }
        }
    }
    assert!(found_dispute_raised, "DisputeRaised event not found");
}