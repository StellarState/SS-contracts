#![cfg(test)]
#![allow(deprecated)]

use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{Address, BytesN, Env, String, Symbol, TryFromVal, TryIntoVal, Val, Vec};

use crate::errors::Error;
use crate::{InvoiceRegistration, InvoiceRegistrationClient};

fn invoice_id(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

fn external(env: &Env, value: &str) -> String {
    String::from_str(env, value)
}

fn setup<'a>(env: &Env) -> InvoiceRegistrationClient<'a> {
    env.mock_all_auths();
    let contract_id = env.register(InvoiceRegistration, ());
    InvoiceRegistrationClient::new(env, &contract_id)
}

fn event_count(env: &Env) -> u32 {
    env.events().all().events().len() as u32
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
fn test_register_invoice_returns_explicit_result_and_stores_it() {
    let env = Env::default();
    env.ledger().set_timestamp(1_700_000_000);
    let client = setup(&env);
    let issuer = Address::generate(&env);
    let id = invoice_id(&env, 1);
    let key = external(&env, "INV-2026-001");

    let result = client.register_invoice(&issuer, &key, &id);

    assert_eq!(result.invoice_id, id);
    assert_eq!(result.external_invoice_id, key);
    assert_eq!(result.issuer, issuer);
    assert_eq!(result.registered_at, 1_700_000_000);

    // Resolvable both ways.
    assert_eq!(client.resolve_registration(&issuer, &key), result);
    assert_eq!(client.get_registration_by_invoice(&id), result);
}

#[test]
fn test_duplicate_registration_same_key_and_issuer_rejected() {
    let env = Env::default();
    let client = setup(&env);
    let issuer = Address::generate(&env);
    let first_id = invoice_id(&env, 2);
    let second_id = invoice_id(&env, 3);
    let key = external(&env, "INV-2026-002");

    let first = client.register_invoice(&issuer, &key, &first_id);
    assert_eq!(
        client.try_register_invoice(&issuer, &key, &second_id),
        Err(Ok(Error::AlreadyRegistered))
    );

    // The failed retry created no second invoice and left the first intact.
    assert_eq!(client.resolve_registration(&issuer, &key), first);
    assert_eq!(client.get_registration_by_invoice(&first_id), first);
    assert_eq!(
        client.try_get_registration_by_invoice(&second_id),
        Err(Ok(Error::NotRegistered))
    );
    assert_eq!(event_count(&env), 1);
}

#[test]
fn test_concurrent_style_retries_register_exactly_once() {
    let env = Env::default();
    let client = setup(&env);
    let issuer = Address::generate(&env);
    let id = invoice_id(&env, 4);
    let key = external(&env, "INV-2026-004");

    let first = client.register_invoice(&issuer, &key, &id);
    for _ in 0..5 {
        assert_eq!(
            client.try_register_invoice(&issuer, &key, &id),
            Err(Ok(Error::AlreadyRegistered))
        );
    }

    assert_eq!(client.resolve_registration(&issuer, &key), first);
    assert_eq!(event_count(&env), 1, "invoice_registered must fire once");
}

#[test]
fn test_same_key_is_namespaced_per_issuer() {
    let env = Env::default();
    let client = setup(&env);
    let issuer_a = Address::generate(&env);
    let issuer_b = Address::generate(&env);
    let key = external(&env, "INV-2026-005");
    let id_a = invoice_id(&env, 5);
    let id_b = invoice_id(&env, 6);

    let reg_a = client.register_invoice(&issuer_a, &key, &id_a);
    let reg_b = client.register_invoice(&issuer_b, &key, &id_b);

    assert_eq!(reg_a.invoice_id, id_a);
    assert_eq!(reg_b.invoice_id, id_b);
    assert_eq!(client.resolve_registration(&issuer_a, &key), reg_a);
    assert_eq!(client.resolve_registration(&issuer_b, &key), reg_b);
}

#[test]
fn test_wrong_issuer_cannot_resolve_or_verify_registration() {
    let env = Env::default();
    let client = setup(&env);
    let issuer = Address::generate(&env);
    let other = Address::generate(&env);
    let id = invoice_id(&env, 7);
    let key = external(&env, "INV-2026-007");

    client.register_invoice(&issuer, &key, &id);

    assert_eq!(
        client.try_resolve_registration(&other, &key),
        Err(Ok(Error::NotRegistered))
    );
    assert!(!client.verify_registration(&other, &key, &id));
    assert!(client.verify_registration(&issuer, &key, &id));
}

#[test]
fn test_verify_registration_mismatched_invoice_id_is_false() {
    let env = Env::default();
    let client = setup(&env);
    let issuer = Address::generate(&env);
    let key = external(&env, "INV-2026-008");

    client.register_invoice(&issuer, &key, &invoice_id(&env, 8));

    assert!(!client.verify_registration(&issuer, &key, &invoice_id(&env, 9)));
}

#[test]
fn test_same_invoice_id_cannot_be_registered_twice() {
    let env = Env::default();
    let client = setup(&env);
    let issuer = Address::generate(&env);
    let id = invoice_id(&env, 10);

    client.register_invoice(&issuer, &external(&env, "KEY-A"), &id);

    // Different key, same invoice id.
    assert_eq!(
        client.try_register_invoice(&issuer, &external(&env, "KEY-B"), &id),
        Err(Ok(Error::InvoiceAlreadyRegistered))
    );
    // Different issuer, same invoice id.
    let other_issuer = Address::generate(&env);
    assert_eq!(
        client.try_register_invoice(&other_issuer, &external(&env, "KEY-C"), &id),
        Err(Ok(Error::InvoiceAlreadyRegistered))
    );
    assert_eq!(
        client.try_resolve_registration(&issuer, &external(&env, "KEY-B")),
        Err(Ok(Error::NotRegistered))
    );
}

#[test]
fn test_invalid_external_id_rejected_without_state() {
    let env = Env::default();
    let client = setup(&env);
    let issuer = Address::generate(&env);
    let id = invoice_id(&env, 11);

    assert_eq!(
        client.try_register_invoice(&issuer, &external(&env, ""), &id),
        Err(Ok(Error::InvalidExternalId))
    );

    let long = "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
    assert_eq!(
        client.try_register_invoice(&issuer, &external(&env, long), &id),
        Err(Ok(Error::InvalidExternalId))
    );

    // Nothing was written and no event was emitted.
    assert_eq!(
        client.try_resolve_registration(&issuer, &external(&env, "")),
        Err(Ok(Error::NotRegistered))
    );
    assert_eq!(
        client.try_get_registration_by_invoice(&id),
        Err(Ok(Error::NotRegistered))
    );
    assert_eq!(event_count(&env), 0);
}

#[test]
fn test_register_requires_issuer_authorization() {
    let env = Env::default();
    let client = setup(&env);
    let issuer = Address::generate(&env);

    client.register_invoice(
        &issuer,
        &external(&env, "INV-2026-012"),
        &invoice_id(&env, 12),
    );

    let auths = env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths.first().unwrap().0, issuer);
}

#[test]
fn test_unknown_registration_lookup_rejected() {
    let env = Env::default();
    let client = setup(&env);
    let issuer = Address::generate(&env);

    assert_eq!(
        client.try_resolve_registration(&issuer, &external(&env, "MISSING")),
        Err(Ok(Error::NotRegistered))
    );
    assert_eq!(
        client.try_get_registration_by_invoice(&invoice_id(&env, 13)),
        Err(Ok(Error::NotRegistered))
    );
}

#[test]
fn test_invoice_registered_event_emitted_once_with_issuer_and_ids() {
    let env = Env::default();
    let client = setup(&env);
    let issuer = Address::generate(&env);
    let id = invoice_id(&env, 14);
    let key = external(&env, "INV-2026-014");

    client.register_invoice(&issuer, &key, &id);

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 2);
    let name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(name, Symbol::new(&env, "invoice_registered"));
    let event_issuer: Address = topics.get(1).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_issuer, issuer);

    let (event_key, event_id): (String, BytesN<32>) = data.try_into_val(&env).unwrap();
    assert_eq!(event_key, key);
    assert_eq!(event_id, id);

    // A rejected duplicate must not emit a second event.
    let _ = client.try_register_invoice(&issuer, &key, &invoice_id(&env, 15));
    assert_eq!(event_count(&env), 1);
}
