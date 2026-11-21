#![cfg(test)]
#![allow(deprecated)]

use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{Address, BytesN, Env, String, Symbol, TryFromVal, TryIntoVal, Val, Vec};

use crate::types::{InvoiceMetadataInput, InvoiceStatus};
use crate::{Error, InvoiceMetadataRegistry, InvoiceMetadataRegistryClient};

fn zero_address(env: &Env) -> Address {
    Address::from_string(&String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ))
}

fn invoice_id(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

fn sample_input(_env: &Env, issuer: &Address) -> InvoiceMetadataInput {
    InvoiceMetadataInput {
        face_value: 100_000_0000000,
        yield_amount: 5_000_0000000,
        maturity_date: 1_900_000_000,
        issuer: issuer.clone(),
    }
}

fn setup<'a>(env: &Env) -> (Address, InvoiceMetadataRegistryClient<'a>) {
    env.mock_all_auths();
    let contract_id = env.register(InvoiceMetadataRegistry, ());
    let client = InvoiceMetadataRegistryClient::new(env, &contract_id);
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
    let contract_id = env.register(InvoiceMetadataRegistry, ());
    let client = InvoiceMetadataRegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    assert_eq!(client.try_initialize(&admin), Err(Ok(Error::AlreadyInit)));
}

#[test]
fn test_register_invoice_stores_all_metadata_fields() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let issuer = Address::generate(&env);
    let id = invoice_id(&env, 1);
    let input = sample_input(&env, &issuer);

    client.register_invoice(&admin, &id, &input);

    let stored = client.get_invoice(&id);
    assert_eq!(stored.invoice_id, id);
    assert_eq!(stored.face_value, input.face_value);
    assert_eq!(stored.yield_amount, input.yield_amount);
    assert_eq!(stored.maturity_date, input.maturity_date);
    assert_eq!(stored.issuer, issuer);
    assert_eq!(stored.status, InvoiceStatus::Pending);
}

#[test]
fn test_register_invoice_restricted_to_admin() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let impostor = Address::generate(&env);
    let issuer = Address::generate(&env);
    let id = invoice_id(&env, 2);

    assert_eq!(
        client.try_register_invoice(&impostor, &id, &sample_input(&env, &issuer)),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(client.try_get_invoice(&id), Err(Ok(Error::NotRegistered)));
}

#[test]
fn test_register_invoice_rejects_duplicate_id() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let issuer = Address::generate(&env);
    let id = invoice_id(&env, 3);
    client.register_invoice(&admin, &id, &sample_input(&env, &issuer));

    assert_eq!(
        client.try_register_invoice(&admin, &id, &sample_input(&env, &issuer)),
        Err(Ok(Error::AlreadyRegistered))
    );
}

#[test]
fn test_register_invoice_rejects_non_positive_amounts() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let issuer = Address::generate(&env);

    let mut zero_face = sample_input(&env, &issuer);
    zero_face.face_value = 0;
    assert_eq!(
        client.try_register_invoice(&admin, &invoice_id(&env, 4), &zero_face),
        Err(Ok(Error::InvalidAmount))
    );

    let mut negative_yield = sample_input(&env, &issuer);
    negative_yield.yield_amount = -1;
    assert_eq!(
        client.try_register_invoice(&admin, &invoice_id(&env, 5), &negative_yield),
        Err(Ok(Error::InvalidAmount))
    );
}

#[test]
fn test_register_invoice_rejects_zero_address_issuer() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let mut input = sample_input(&env, &zero_address(&env));
    input.issuer = zero_address(&env);
    assert_eq!(
        client.try_register_invoice(&admin, &invoice_id(&env, 6), &input),
        Err(Ok(Error::InvalidAddress))
    );
}

#[test]
fn test_get_invoice_unregistered_rejected() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    assert_eq!(
        client.try_get_invoice(&invoice_id(&env, 99)),
        Err(Ok(Error::NotRegistered))
    );
}

#[test]
fn test_valid_transitions_pending_to_live_to_funded_to_settled() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let issuer = Address::generate(&env);
    let id = invoice_id(&env, 10);
    client.register_invoice(&admin, &id, &sample_input(&env, &issuer));

    client.update_status(&admin, &id, &InvoiceStatus::Live);
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Live);

    client.update_status(&admin, &id, &InvoiceStatus::Funded);
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Funded);

    client.update_status(&admin, &id, &InvoiceStatus::Settled);
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Settled);
}

#[test]
fn test_valid_transition_pending_to_expired() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let issuer = Address::generate(&env);
    let id = invoice_id(&env, 11);
    client.register_invoice(&admin, &id, &sample_input(&env, &issuer));

    client.update_status(&admin, &id, &InvoiceStatus::Expired);
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Expired);
}

#[test]
fn test_valid_transition_live_to_expired() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let issuer = Address::generate(&env);
    let id = invoice_id(&env, 12);
    client.register_invoice(&admin, &id, &sample_input(&env, &issuer));
    client.update_status(&admin, &id, &InvoiceStatus::Live);

    client.update_status(&admin, &id, &InvoiceStatus::Expired);
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Expired);
}

#[test]
fn test_invalid_transition_pending_to_funded_rejected() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let issuer = Address::generate(&env);
    let id = invoice_id(&env, 13);
    client.register_invoice(&admin, &id, &sample_input(&env, &issuer));

    assert_eq!(
        client.try_update_status(&admin, &id, &InvoiceStatus::Funded),
        Err(Ok(Error::InvalidTransition))
    );
    // State unchanged after the rejected transition.
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Pending);
}

#[test]
fn test_invalid_transition_settled_is_terminal() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let issuer = Address::generate(&env);
    let id = invoice_id(&env, 14);
    client.register_invoice(&admin, &id, &sample_input(&env, &issuer));
    client.update_status(&admin, &id, &InvoiceStatus::Live);
    client.update_status(&admin, &id, &InvoiceStatus::Funded);
    client.update_status(&admin, &id, &InvoiceStatus::Settled);

    assert_eq!(
        client.try_update_status(&admin, &id, &InvoiceStatus::Live),
        Err(Ok(Error::InvalidTransition))
    );
    assert_eq!(
        client.try_update_status(&admin, &id, &InvoiceStatus::Expired),
        Err(Ok(Error::InvalidTransition))
    );
}

#[test]
fn test_invalid_transition_expired_is_terminal() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let issuer = Address::generate(&env);
    let id = invoice_id(&env, 15);
    client.register_invoice(&admin, &id, &sample_input(&env, &issuer));
    client.update_status(&admin, &id, &InvoiceStatus::Expired);

    assert_eq!(
        client.try_update_status(&admin, &id, &InvoiceStatus::Live),
        Err(Ok(Error::InvalidTransition))
    );
}

#[test]
fn test_invalid_self_transition_rejected() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let issuer = Address::generate(&env);
    let id = invoice_id(&env, 16);
    client.register_invoice(&admin, &id, &sample_input(&env, &issuer));

    assert_eq!(
        client.try_update_status(&admin, &id, &InvoiceStatus::Pending),
        Err(Ok(Error::InvalidTransition))
    );
}

#[test]
fn test_update_status_restricted_to_admin() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let impostor = Address::generate(&env);
    let issuer = Address::generate(&env);
    let id = invoice_id(&env, 17);
    client.register_invoice(&admin, &id, &sample_input(&env, &issuer));

    assert_eq!(
        client.try_update_status(&impostor, &id, &InvoiceStatus::Live),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Pending);
}

#[test]
fn test_update_status_unregistered_invoice_rejected() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    assert_eq!(
        client.try_update_status(&admin, &invoice_id(&env, 18), &InvoiceStatus::Live),
        Err(Ok(Error::NotRegistered))
    );
}

#[test]
fn metadata_editor_updates_revisions_but_funding_admin_cannot() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let issuer = Address::generate(&env);
    let editor = Address::generate(&env);
    let id = invoice_id(&env, 30);
    let mut metadata = sample_input(&env, &issuer);
    client.register_invoice(&admin, &id, &metadata);
    client.set_metadata_editor(&issuer, &id, &editor);

    metadata.face_value += 1;
    assert_eq!(
        client.try_update_metadata(&admin, &id, &metadata),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(client.update_metadata(&editor, &id, &metadata), 2);
    assert_eq!(client.get_current_revision(&id), 2);
    let history = client.get_revision_history(&id);
    assert_eq!(history.len(), 2);
    assert_eq!(
        history.get(1).unwrap().metadata.face_value,
        metadata.face_value
    );

    metadata.issuer = Address::generate(&env);
    assert_eq!(
        client.try_update_metadata(&editor, &id, &metadata),
        Err(Ok(Error::Unauthorized))
    );
    metadata.issuer = issuer.clone();
    client.update_status(&admin, &id, &InvoiceStatus::Live);
    client.update_status(&admin, &id, &InvoiceStatus::Funded);
    metadata.face_value += 1;
    assert_eq!(
        client.try_update_metadata(&editor, &id, &metadata),
        Err(Ok(Error::MetadataLocked))
    );
    assert_eq!(client.get_current_revision(&id), 2);
}

#[test]
fn issuer_can_reassign_editor_but_non_issuer_cannot() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let issuer = Address::generate(&env);
    let outsider = Address::generate(&env);
    let editor = Address::generate(&env);
    let id = invoice_id(&env, 31);
    client.register_invoice(&_admin, &id, &sample_input(&env, &issuer));

    assert_eq!(
        client.try_set_metadata_editor(&outsider, &id, &editor),
        Err(Ok(Error::Unauthorized))
    );
    client.set_metadata_editor(&issuer, &id, &editor);
    assert_eq!(client.get_invoice(&id).metadata_editor, editor);
}

#[test]
fn test_invoice_registered_event_emitted() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let issuer = Address::generate(&env);
    let id = invoice_id(&env, 20);
    client.register_invoice(&admin, &id, &sample_input(&env, &issuer));

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 2);
    let name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(name, Symbol::new(&env, "InvoiceRegistered"));
    let event_id: BytesN<32> = topics.get(1).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_id, id);
    let event_issuer: Address = data.try_into_val(&env).unwrap();
    assert_eq!(event_issuer, issuer);
}

#[test]
fn test_invoice_status_updated_event_emitted_with_old_and_new_status() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let issuer = Address::generate(&env);
    let id = invoice_id(&env, 21);
    client.register_invoice(&admin, &id, &sample_input(&env, &issuer));

    client.update_status(&admin, &id, &InvoiceStatus::Live);
    let (topics, data) = last_event(&env);
    let name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(name, Symbol::new(&env, "InvoiceStatusUpdated"));
    let event_id: BytesN<32> = topics.get(1).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_id, id);

    let (old_status, new_status): (u32, u32) = data.try_into_val(&env).unwrap();
    assert_eq!(old_status, InvoiceStatus::Pending as u32);
    assert_eq!(new_status, InvoiceStatus::Live as u32);
}
