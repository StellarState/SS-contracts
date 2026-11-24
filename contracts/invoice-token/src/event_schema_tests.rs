//! Event Schema Snapshot Tests for Invoice Token
//!
//! These tests capture the exact topic and data shape for each public mutation event.
//! They fail if a topic is renamed, a field is removed, or field order changes.

#![cfg(test)]
#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{Address, Env, Symbol, TryFromVal, Val, Vec};

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

fn assert_val_type<T>(env: &Env, value: &Val)
where
    T: TryFromVal<Env, Val>,
{
    assert!(T::try_from_val(env, value).is_ok());
}

#[test]
fn event_schema_snapshot_transfer() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    let from = Address::generate(&env);
    let to = Address::generate(&env);

    let token_id = env.register_contract(None, InvoiceToken);
    let token = InvoiceTokenClient::new(&env, &token_id);

    token.initialize(
        &admin,
        &soroban_sdk::String::from_str(&env, "Test Token"),
        &soroban_sdk::String::from_str(&env, "TTK"),
        &7,
        &soroban_sdk::Symbol::new(&env, "INV001"),
        &minter,
        &i128::MAX,
        &1,
    );

    // Mint some tokens to from
    token.mint(&from, &1000, &admin);
    token.set_transfer_locked(&admin, &false);

    let _ = env.events().all();

    token.transfer(&from, &to, &100);

    let events = env.events().all();
    let mut found = false;
    for event in events.events().iter() {
        let (topics, _) = parse_event(&env, &event);
        if topics.len() >= 2 {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(1).unwrap()) {
                if sym == Symbol::new(&env, "transfer") {
                    found = true;

                    // Verify topic structure: [contract, "transfer", from, to]
                    assert!(topics.len() >= 4, "transfer should have at least 4 topics");

                    // Verify data: amount (i128)
                    let (_, data) = parse_event(&env, &event);
                    assert_val_type::<i128>(&env, &data);

                    break;
                }
            }
        }
    }
    assert!(found, "transfer event not found");
}

#[test]
fn event_schema_snapshot_approve() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    let from = Address::generate(&env);
    let spender = Address::generate(&env);

    let token_id = env.register_contract(None, InvoiceToken);
    let token = InvoiceTokenClient::new(&env, &token_id);

    token.initialize(
        &admin,
        &soroban_sdk::String::from_str(&env, "Test Token"),
        &soroban_sdk::String::from_str(&env, "TTK"),
        &7,
        &soroban_sdk::Symbol::new(&env, "INV002"),
        &minter,
        &i128::MAX,
        &1,
    );

    let _ = env.events().all();

    token.approve(&from, &spender, &500, &1000);

    let events = env.events().all();
    let mut found = false;
    for event in events.events().iter() {
        let (topics, _) = parse_event(&env, &event);
        if topics.len() >= 2 {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(1).unwrap()) {
                if sym == Symbol::new(&env, "approve") {
                    found = true;

                    // Verify topic structure: [contract, "approve", from, spender]
                    assert!(topics.len() >= 4, "approve should have at least 4 topics");

                    // Verify data: (amount, expiration_ledger)
                    let (_, data) = parse_event(&env, &event);
                    let data_vec = Vec::<Val>::try_from_val(&env, &data).unwrap();
                    assert_eq!(data_vec.len(), 2, "approve data should have 2 fields");
                    assert_val_type::<i128>(&env, &data_vec.get(0).unwrap());
                    assert_val_type::<u32>(&env, &data_vec.get(1).unwrap());

                    break;
                }
            }
        }
    }
    assert!(found, "approve event not found");
}

#[test]
fn event_schema_snapshot_mint() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    let to = Address::generate(&env);

    let token_id = env.register_contract(None, InvoiceToken);
    let token = InvoiceTokenClient::new(&env, &token_id);

    token.initialize(
        &admin,
        &soroban_sdk::String::from_str(&env, "Test Token"),
        &soroban_sdk::String::from_str(&env, "TTK"),
        &7,
        &soroban_sdk::Symbol::new(&env, "INV003"),
        &minter,
        &i128::MAX,
        &1,
    );

    let _ = env.events().all();

    token.mint(&to, &100, &admin);

    let events = env.events().all();
    let mut found = false;
    for event in events.events().iter() {
        let (topics, _) = parse_event(&env, &event);
        if topics.len() >= 2 {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(1).unwrap()) {
                if sym == Symbol::new(&env, "mint") {
                    found = true;

                    // Verify topic structure: [contract, "mint", to]
                    assert!(topics.len() >= 3, "mint should have at least 3 topics");

                    // Verify data: amount (i128)
                    let (_, data) = parse_event(&env, &event);
                    assert_val_type::<i128>(&env, &data);

                    break;
                }
            }
        }
    }
    assert!(found, "mint event not found");
}

#[test]
fn event_schema_snapshot_burn() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    let from = Address::generate(&env);

    let token_id = env.register_contract(None, InvoiceToken);
    let token = InvoiceTokenClient::new(&env, &token_id);

    token.initialize(
        &admin,
        &soroban_sdk::String::from_str(&env, "Test Token"),
        &soroban_sdk::String::from_str(&env, "TTK"),
        &7,
        &soroban_sdk::Symbol::new(&env, "INV004"),
        &minter,
        &i128::MAX,
        &1,
    );

    token.mint(&from, &1000, &admin);

    let _ = env.events().all();

    token.burn(&from, &100);

    let events = env.events().all();
    let mut found = false;
    for event in events.events().iter() {
        let (topics, _) = parse_event(&env, &event);
        if topics.len() >= 2 {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(1).unwrap()) {
                if sym == Symbol::new(&env, "burn") {
                    found = true;

                    // Verify topic structure: [contract, "burn", from]
                    assert!(topics.len() >= 3, "burn should have at least 3 topics");

                    // Verify data: amount (i128)
                    let (_, data) = parse_event(&env, &event);
                    assert_val_type::<i128>(&env, &data);

                    break;
                }
            }
        }
    }
    assert!(found, "burn event not found");
}

#[test]
fn event_schema_snapshot_transfer_locked_updated() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let minter = Address::generate(&env);

    let token_id = env.register_contract(None, InvoiceToken);
    let token = InvoiceTokenClient::new(&env, &token_id);

    token.initialize(
        &admin,
        &soroban_sdk::String::from_str(&env, "Test Token"),
        &soroban_sdk::String::from_str(&env, "TTK"),
        &7,
        &soroban_sdk::Symbol::new(&env, "INV005"),
        &minter,
        &i128::MAX,
        &1,
    );

    let _ = env.events().all();

    token.set_transfer_locked(&admin, &false);

    let events = env.events().all();
    let mut found = false;
    for event in events.events().iter() {
        let (topics, _) = parse_event(&env, &event);
        if topics.len() >= 2 {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(1).unwrap()) {
                if sym == Symbol::new(&env, "transfer_locked_updated") {
                    found = true;

                    // Verify topic structure: [contract, "transfer_locked_updated"]
                    assert!(
                        topics.len() >= 2,
                        "transfer_locked_updated should have at least 2 topics"
                    );

                    // Verify data: (old_value, new_value)
                    let (_, data) = parse_event(&env, &event);
                    let data_vec = Vec::<Val>::try_from_val(&env, &data).unwrap();
                    assert_eq!(
                        data_vec.len(),
                        2,
                        "transfer_locked_updated data should have 2 fields"
                    );
                    assert_val_type::<bool>(&env, &data_vec.get(0).unwrap());
                    assert_val_type::<bool>(&env, &data_vec.get(1).unwrap());

                    break;
                }
            }
        }
    }
    assert!(found, "transfer_locked_updated event not found");
}

#[test]
fn event_schema_snapshot_minter_updated() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    let new_minter = Address::generate(&env);

    let token_id = env.register_contract(None, InvoiceToken);
    let token = InvoiceTokenClient::new(&env, &token_id);

    token.initialize(
        &admin,
        &soroban_sdk::String::from_str(&env, "Test Token"),
        &soroban_sdk::String::from_str(&env, "TTK"),
        &7,
        &soroban_sdk::Symbol::new(&env, "INV006"),
        &minter,
        &i128::MAX,
        &1,
    );

    let _ = env.events().all();

    token.set_minter(&new_minter);

    let events = env.events().all();
    let mut found = false;
    for event in events.events().iter() {
        let (topics, _) = parse_event(&env, &event);
        if topics.len() >= 2 {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(1).unwrap()) {
                if sym == Symbol::new(&env, "minter_updated") {
                    found = true;

                    // Verify topic structure: [contract, "minter_updated"]
                    assert!(
                        topics.len() >= 2,
                        "minter_updated should have at least 2 topics"
                    );

                    // Verify data: (old_minter, new_minter)
                    let (_, data) = parse_event(&env, &event);
                    let data_vec = Vec::<Val>::try_from_val(&env, &data).unwrap();
                    assert_eq!(
                        data_vec.len(),
                        2,
                        "minter_updated data should have 2 fields"
                    );
                    assert_val_type::<Address>(&env, &data_vec.get(0).unwrap());
                    assert_val_type::<Address>(&env, &data_vec.get(1).unwrap());

                    break;
                }
            }
        }
    }
    assert!(found, "minter_updated event not found");
}

#[test]
fn event_schema_snapshot_paused_updated() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let minter = Address::generate(&env);

    let token_id = env.register_contract(None, InvoiceToken);
    let token = InvoiceTokenClient::new(&env, &token_id);

    token.initialize(
        &admin,
        &soroban_sdk::String::from_str(&env, "Test Token"),
        &soroban_sdk::String::from_str(&env, "TTK"),
        &7,
        &soroban_sdk::Symbol::new(&env, "INV007"),
        &minter,
        &i128::MAX,
        &1,
    );

    let _ = env.events().all();

    token.set_paused(&true);

    let events = env.events().all();
    let mut found = false;
    for event in events.events().iter() {
        let (topics, _) = parse_event(&env, &event);
        if topics.len() >= 2 {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(1).unwrap()) {
                if sym == Symbol::new(&env, "paused_updated") {
                    found = true;

                    // Verify topic structure: [contract, "paused_updated"]
                    assert!(
                        topics.len() >= 2,
                        "paused_updated should have at least 2 topics"
                    );

                    // Verify data: (old_value, new_value)
                    let (_, data) = parse_event(&env, &event);
                    let data_vec = Vec::<Val>::try_from_val(&env, &data).unwrap();
                    assert_eq!(
                        data_vec.len(),
                        2,
                        "paused_updated data should have 2 fields"
                    );
                    assert_val_type::<bool>(&env, &data_vec.get(0).unwrap());
                    assert_val_type::<bool>(&env, &data_vec.get(1).unwrap());

                    break;
                }
            }
        }
    }
    assert!(found, "paused_updated event not found");
}

#[test]
fn event_schema_snapshot_account_frozen() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    let account = Address::generate(&env);

    let token_id = env.register_contract(None, InvoiceToken);
    let token = InvoiceTokenClient::new(&env, &token_id);

    token.initialize(
        &admin,
        &soroban_sdk::String::from_str(&env, "Test Token"),
        &soroban_sdk::String::from_str(&env, "TTK"),
        &7,
        &soroban_sdk::Symbol::new(&env, "INV008"),
        &minter,
        &i128::MAX,
        &1,
    );

    let _ = env.events().all();

    token.freeze_account(&account);

    let events = env.events().all();
    let mut found = false;
    for event in events.events().iter() {
        let (topics, _) = parse_event(&env, &event);
        if topics.len() >= 2 {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(1).unwrap()) {
                if sym == Symbol::new(&env, "account_frozen") {
                    found = true;

                    // Verify topic structure: [contract, "account_frozen"]
                    assert!(
                        topics.len() >= 2,
                        "account_frozen should have at least 2 topics"
                    );

                    // Verify data: account
                    let (_, data) = parse_event(&env, &event);
                    assert_val_type::<Address>(&env, &data);

                    break;
                }
            }
        }
    }
    assert!(found, "account_frozen event not found");
}

#[test]
fn event_schema_snapshot_clawback() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    let from = Address::generate(&env);

    let token_id = env.register_contract(None, InvoiceToken);
    let token = InvoiceTokenClient::new(&env, &token_id);

    token.initialize(
        &admin,
        &soroban_sdk::String::from_str(&env, "Test Token"),
        &soroban_sdk::String::from_str(&env, "TTK"),
        &7,
        &soroban_sdk::Symbol::new(&env, "INV009"),
        &minter,
        &i128::MAX,
        &1,
    );

    token.mint(&from, &1000, &admin);
    token.freeze_account(&from);

    let _ = env.events().all();

    token.clawback(&from, &100);

    let events = env.events().all();
    let mut found = false;
    for event in events.events().iter() {
        let (topics, _) = parse_event(&env, &event);
        if topics.len() >= 2 {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(1).unwrap()) {
                if sym == Symbol::new(&env, "clawback") {
                    found = true;

                    // Verify topic structure: [contract, "clawback", from]
                    assert!(topics.len() >= 3, "clawback should have at least 3 topics");

                    // Verify data: amount
                    let (_, data) = parse_event(&env, &event);
                    assert_val_type::<i128>(&env, &data);

                    break;
                }
            }
        }
    }
    assert!(found, "clawback event not found");
}

#[test]
fn event_schema_snapshot_fee_updated() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let minter = Address::generate(&env);

    let token_id = env.register_contract(None, InvoiceToken);
    let token = InvoiceTokenClient::new(&env, &token_id);

    token.initialize(
        &admin,
        &soroban_sdk::String::from_str(&env, "Test Token"),
        &soroban_sdk::String::from_str(&env, "TTK"),
        &7,
        &soroban_sdk::Symbol::new(&env, "INV010"),
        &minter,
        &i128::MAX,
        &1,
    );

    let _ = env.events().all();

    token.set_fee_bps(&admin, &500);

    let events = env.events().all();
    let mut found = false;
    for event in events.events().iter() {
        let (topics, _) = parse_event(&env, &event);
        if topics.len() >= 2 {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(1).unwrap()) {
                if sym == Symbol::new(&env, "fee_updated") {
                    found = true;

                    // Verify topic structure: [contract, "fee_updated"]
                    assert!(
                        topics.len() >= 2,
                        "fee_updated should have at least 2 topics"
                    );

                    // Verify data: (old_bps, new_bps)
                    let (_, data) = parse_event(&env, &event);
                    let data_vec = Vec::<Val>::try_from_val(&env, &data).unwrap();
                    assert_eq!(data_vec.len(), 2, "fee_updated data should have 2 fields");
                    assert_val_type::<i128>(&env, &data_vec.get(0).unwrap());
                    assert_val_type::<i128>(&env, &data_vec.get(1).unwrap());

                    break;
                }
            }
        }
    }
    assert!(found, "fee_updated event not found");
}

#[test]
fn event_schema_snapshot_role_admin_updated() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    let new_admin = Address::generate(&env);

    let token_id = env.register_contract(None, InvoiceToken);
    let token = InvoiceTokenClient::new(&env, &token_id);

    token.initialize(
        &admin,
        &soroban_sdk::String::from_str(&env, "Test Token"),
        &soroban_sdk::String::from_str(&env, "TTK"),
        &7,
        &soroban_sdk::Symbol::new(&env, "INV011"),
        &minter,
        &i128::MAX,
        &1,
    );

    let role = soroban_sdk::Symbol::new(&env, "minter");

    let _ = env.events().all();

    token.set_role_admin(&admin, &role, &new_admin);

    let events = env.events().all();
    let mut found = false;
    for event in events.events().iter() {
        let (topics, _) = parse_event(&env, &event);
        if topics.len() >= 2 {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(1).unwrap()) {
                if sym == Symbol::new(&env, "role_admin_updated") {
                    found = true;

                    // Verify topic structure: [contract, "role_admin_updated", role]
                    assert!(
                        topics.len() >= 3,
                        "role_admin_updated should have at least 3 topics"
                    );

                    // Verify data: (old_admin, new_admin)
                    let (_, data) = parse_event(&env, &event);
                    let data_vec = Vec::<Val>::try_from_val(&env, &data).unwrap();
                    assert_eq!(
                        data_vec.len(),
                        2,
                        "role_admin_updated data should have 2 fields"
                    );
                    assert_val_type::<Address>(&env, &data_vec.get(0).unwrap());
                    assert_val_type::<Address>(&env, &data_vec.get(1).unwrap());

                    break;
                }
            }
        }
    }
    assert!(found, "role_admin_updated event not found");
}

#[test]
fn event_schema_snapshot_role_granted() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    let account = Address::generate(&env);

    let token_id = env.register_contract(None, InvoiceToken);
    let token = InvoiceTokenClient::new(&env, &token_id);

    token.initialize(
        &admin,
        &soroban_sdk::String::from_str(&env, "Test Token"),
        &soroban_sdk::String::from_str(&env, "TTK"),
        &7,
        &soroban_sdk::Symbol::new(&env, "INV012"),
        &minter,
        &i128::MAX,
        &1,
    );

    let role = soroban_sdk::Symbol::new(&env, "minter");

    let _ = env.events().all();

    token.grant_role(&admin, &role, &account);

    let events = env.events().all();
    let mut found = false;
    for event in events.events().iter() {
        let (topics, _) = parse_event(&env, &event);
        if topics.len() >= 2 {
            if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(1).unwrap()) {
                if sym == Symbol::new(&env, "role_granted") {
                    found = true;

                    // Verify topic structure: [contract, "role_granted", role, account]
                    assert!(
                        topics.len() >= 4,
                        "role_granted should have at least 4 topics"
                    );

                    // Verify data: granted (bool)
                    let (_, data) = parse_event(&env, &event);
                    assert_val_type::<bool>(&env, &data);

                    break;
                }
            }
        }
    }
    assert!(found, "role_granted event not found");
}
