#![cfg(test)]
#![allow(deprecated)]

use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{Address, BytesN, Env, String, Symbol, TryFromVal, TryIntoVal, Val, Vec};

use crate::{Error, SecondaryTradeHistory, SecondaryTradeHistoryClient};

fn zero_address(env: &Env) -> Address {
    Address::from_string(&String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ))
}

fn setup<'a>(env: &Env) -> (Address, Address, SecondaryTradeHistoryClient<'a>) {
    env.mock_all_auths();
    let contract_id = env.register(SecondaryTradeHistory, ());
    let client = SecondaryTradeHistoryClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let recorder = Address::generate(env);
    client.initialize(&admin, &recorder);
    (admin, recorder, client)
}

fn invoice_id(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

/// Returns the topics and data of the most recently emitted event on `env`.
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
fn test_initialize_sets_admin_and_recorder() {
    let env = Env::default();
    let (admin, recorder, client) = setup(&env);
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_recorder(), recorder);
}

#[test]
fn test_cannot_initialize_twice() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(SecondaryTradeHistory, ());
    let client = SecondaryTradeHistoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let recorder = Address::generate(&env);
    client.initialize(&admin, &recorder);
    assert_eq!(
        client.try_initialize(&admin, &recorder),
        Err(Ok(Error::AlreadyInit))
    );
}

#[test]
fn test_initialize_rejects_zero_address() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(SecondaryTradeHistory, ());
    let client = SecondaryTradeHistoryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let recorder = Address::generate(&env);

    assert_eq!(
        client.try_initialize(&zero_address(&env), &recorder),
        Err(Ok(Error::InvalidAddress))
    );
    assert_eq!(
        client.try_initialize(&admin, &zero_address(&env)),
        Err(Ok(Error::InvalidAddress))
    );
}

#[test]
fn test_record_trade_and_get_last_price() {
    let env = Env::default();
    let (_admin, recorder, client) = setup(&env);
    let inv = invoice_id(&env, 1);

    client.record_trade(&inv, &100, &5, &1000);
    assert_eq!(client.get_last_price(&inv), 100);

    client.record_trade(&inv, &150, &3, &2000);
    assert_eq!(client.get_last_price(&inv), 150);
    let _ = recorder;
}

#[test]
fn test_get_last_price_fails_with_no_trades() {
    let env = Env::default();
    let (_admin, _recorder, client) = setup(&env);
    let inv = invoice_id(&env, 1);

    assert_eq!(client.try_get_last_price(&inv), Err(Ok(Error::NoTrades)));
}

#[test]
fn test_get_price_history_newest_first() {
    let env = Env::default();
    let (_admin, _recorder, client) = setup(&env);
    let inv = invoice_id(&env, 1);

    client.record_trade(&inv, &100, &1, &1000);
    client.record_trade(&inv, &110, &1, &2000);
    client.record_trade(&inv, &120, &1, &3000);

    let history = client.get_price_history(&inv, &10);
    assert_eq!(history.len(), 3);
    assert_eq!(history.get(0).unwrap().price, 120);
    assert_eq!(history.get(1).unwrap().price, 110);
    assert_eq!(history.get(2).unwrap().price, 100);
}

#[test]
fn test_get_price_history_respects_limit() {
    let env = Env::default();
    let (_admin, _recorder, client) = setup(&env);
    let inv = invoice_id(&env, 1);

    for i in 0..5 {
        client.record_trade(&inv, &(100 + i as i128), &1, &(1000 + i as u64));
    }

    let history = client.get_price_history(&inv, &2);
    assert_eq!(history.len(), 2);
    // Newest first: last recorded (104) then second-to-last (103).
    assert_eq!(history.get(0).unwrap().price, 104);
    assert_eq!(history.get(1).unwrap().price, 103);
}

#[test]
fn test_history_pruned_beyond_100_trades() {
    let env = Env::default();
    let (_admin, _recorder, client) = setup(&env);
    let inv = invoice_id(&env, 1);

    for i in 0..105 {
        client.record_trade(&inv, &(1000 + i as i128), &1, &(i as u64));
    }

    let history = client.get_price_history(&inv, &200);
    assert_eq!(history.len(), 100);
    // The 5 oldest trades (prices 1000..1004) should have been pruned;
    // the oldest remaining entry is price 1005, and it sits last since
    // history is stored newest-first.
    assert_eq!(history.get(99).unwrap().price, 1005);
    // The most recent trade (price 1104) is still first.
    assert_eq!(history.get(0).unwrap().price, 1104);
    assert_eq!(client.get_last_price(&inv), 1104);
}

#[test]
fn test_record_trade_rejects_non_positive_price_or_amount() {
    let env = Env::default();
    let (_admin, _recorder, client) = setup(&env);
    let inv = invoice_id(&env, 1);

    assert_eq!(
        client.try_record_trade(&inv, &0, &1, &1000),
        Err(Ok(Error::InvalidTrade))
    );
    assert_eq!(
        client.try_record_trade(&inv, &100, &0, &1000),
        Err(Ok(Error::InvalidTrade))
    );
    assert_eq!(
        client.try_record_trade(&inv, &-10, &1, &1000),
        Err(Ok(Error::InvalidTrade))
    );
}

#[test]
fn test_record_trade_emits_trade_recorded_event() {
    let env = Env::default();
    let (_admin, _recorder, client) = setup(&env);
    let inv = invoice_id(&env, 1);

    client.record_trade(&inv, &100, &5, &1000);

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 2);
    let event_name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_name, Symbol::new(&env, "TradeRecorded"));
    let event_invoice: BytesN<32> = topics.get(1).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_invoice, inv);
    let (price, amount, timestamp): (i128, i128, u64) = data.try_into_val(&env).unwrap();
    assert_eq!(price, 100);
    assert_eq!(amount, 5);
    assert_eq!(timestamp, 1000);
}

#[test]
fn test_set_recorder_admin_only() {
    let env = Env::default();
    let (admin, _old_recorder, client) = setup(&env);
    let new_recorder = Address::generate(&env);
    let not_admin = Address::generate(&env);

    assert_eq!(
        client.try_set_recorder(&not_admin, &new_recorder),
        Err(Ok(Error::Unauthorized))
    );

    client.set_recorder(&admin, &new_recorder);
    assert_eq!(client.get_recorder(), new_recorder);
}

#[test]
fn test_history_isolated_per_invoice() {
    let env = Env::default();
    let (_admin, _recorder, client) = setup(&env);
    let inv_a = invoice_id(&env, 1);
    let inv_b = invoice_id(&env, 2);

    client.record_trade(&inv_a, &100, &1, &1000);
    client.record_trade(&inv_b, &999, &1, &1000);

    assert_eq!(client.get_last_price(&inv_a), 100);
    assert_eq!(client.get_last_price(&inv_b), 999);
}
