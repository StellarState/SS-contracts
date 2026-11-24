#![cfg(test)]

use super::{InvoiceToken, InvoiceTokenClient};
use crate::errors::Error;
use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{Address, Env, Symbol, TryFromVal, Val, Vec};

fn setup(env: &Env) -> (InvoiceTokenClient<'_>, Address, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(InvoiceToken, ());
    let client = InvoiceTokenClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let settlement = Address::generate(env);
    let name = soroban_sdk::String::from_str(env, "Invoice INV-001");
    let symbol = soroban_sdk::String::from_str(env, "INV001");
    client.initialize(
        &admin,
        &name,
        &symbol,
        &7,
        &Symbol::new(env, "inv_001"),
        &Address::generate(env),
        &i128::MAX,
        &1,
    );
    (client, contract_id, admin, settlement)
}

fn event_topics(env: &Env, event: &soroban_sdk::xdr::ContractEvent) -> Vec<Val> {
    let soroban_sdk::xdr::ContractEventBody::V0(v0) = &event.body;
    Vec::<Val>::try_from_val(
        env,
        &soroban_sdk::xdr::ScVal::Vec(Some(v0.topics.clone().into())),
    )
    .unwrap()
}

#[test]
fn secondary_transfer_moves_fractions_records_earnings_and_emits_breakdown() {
    let env = Env::default();
    let (client, _, admin, settlement) = setup(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let treasury = Address::generate(&env);
    let issuer = Address::generate(&env);

    client.set_transfer_locked(&admin, &false);
    client.set_secondary_config(&admin, &settlement, &treasury, &issuer, &250, &500);
    client.mint(&seller, &100, &admin);
    client.secondary_transfer(&Symbol::new(&env, "inv_001"), &seller, &buyer, &40, &10_000);

    let mut found_event = false;
    for event in env.events().all().events().iter() {
        let topics = event_topics(&env, &event);
        let mut is_secondary_transfer = false;
        for i in 0..topics.len() {
            if let Ok(symbol) = Symbol::try_from_val(&env, &topics.get(i).unwrap()) {
                if symbol == Symbol::new(&env, "SecondaryTransfer") {
                    is_secondary_transfer = true;
                    break;
                }
            }
        }
        if !is_secondary_transfer {
            continue;
        }
        let soroban_sdk::xdr::ContractEventBody::V0(v0) = &event.body;
        let data = Val::try_from_val(&env, &v0.data).unwrap();
        let breakdown: (
            Address,
            Address,
            Address,
            Address,
            i128,
            i128,
            i128,
            i128,
            i128,
        ) = TryFromVal::try_from_val(&env, &data).unwrap();
        assert_eq!(
            breakdown,
            (
                seller.clone(),
                buyer.clone(),
                treasury.clone(),
                issuer.clone(),
                40,
                10_000,
                250,
                500,
                9_250
            )
        );
        found_event = true;
    }
    assert!(found_event, "events: {:?}", env.events().all().events());

    assert_eq!(client.balance(&seller), 60);
    assert_eq!(client.balance(&buyer), 40);
    assert_eq!(client.get_earnings(&seller), 9_250);
    assert_eq!(client.get_earnings(&treasury), 250);
    assert_eq!(client.get_earnings(&issuer), 500);
}

#[test]
fn secondary_transfer_rejects_mismatched_invoice_and_preserves_balances() {
    let env = Env::default();
    let (client, _, admin, settlement) = setup(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let treasury = Address::generate(&env);
    let issuer = Address::generate(&env);

    client.set_transfer_locked(&admin, &false);
    client.set_secondary_config(&admin, &settlement, &treasury, &issuer, &250, &500);
    client.mint(&seller, &100, &admin);

    assert_eq!(
        client.try_secondary_transfer(
            &Symbol::new(&env, "other_invoice"),
            &seller,
            &buyer,
            &40,
            &10_000,
        ),
        Err(Ok(Error::InvoiceIdMismatch))
    );
    assert_eq!(client.balance(&seller), 100);
    assert_eq!(client.balance(&buyer), 0);
    assert_eq!(client.get_earnings(&seller), 0);
}

#[test]
fn secondary_transfer_reverts_fraction_move_if_an_earnings_credit_overflows() {
    let env = Env::default();
    let (client, contract_id, admin, settlement) = setup(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let treasury = Address::generate(&env);
    let issuer = Address::generate(&env);

    client.set_transfer_locked(&admin, &false);
    client.set_secondary_config(&admin, &settlement, &treasury, &issuer, &250, &500);
    client.mint(&seller, &100, &admin);
    env.as_contract(&contract_id, || {
        crate::storage::set_earnings(&env, &treasury, i128::MAX)
    });

    assert_eq!(
        client
            .try_secondary_transfer(&Symbol::new(&env, "inv_001"), &seller, &buyer, &40, &10_000,),
        Err(Ok(Error::Overflow))
    );
    assert_eq!(client.balance(&seller), 100);
    assert_eq!(client.balance(&buyer), 0);
    assert_eq!(client.get_earnings(&seller), 0);
    assert_eq!(client.get_earnings(&treasury), i128::MAX);
}
