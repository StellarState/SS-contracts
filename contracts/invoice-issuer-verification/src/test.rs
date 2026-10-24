#![cfg(test)]
#![allow(deprecated)]

use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{Address, Env, String, Symbol, TryFromVal, TryIntoVal, Val, Vec};

use crate::types::VerificationTier;
use crate::{Error, InvoiceIssuerVerification, InvoiceIssuerVerificationClient};

fn zero_address(env: &Env) -> Address {
    Address::from_string(&String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ))
}

fn setup<'a>(env: &Env) -> (Address, InvoiceIssuerVerificationClient<'a>) {
    env.mock_all_auths();
    let contract_id = env.register(InvoiceIssuerVerification, ());
    let client = InvoiceIssuerVerificationClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (admin, client)
}

/// Returns the topics and data of the most recently emitted event on `env`,
/// decoded from its raw XDR form.
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
    let contract_id = env.register(InvoiceIssuerVerification, ());
    let client = InvoiceIssuerVerificationClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);
    assert_eq!(client.try_initialize(&admin), Err(Ok(Error::AlreadyInit)));
}

#[test]
fn test_initialize_rejects_zero_address_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(InvoiceIssuerVerification, ());
    let client = InvoiceIssuerVerificationClient::new(&env, &contract_id);

    assert_eq!(
        client.try_initialize(&zero_address(&env)),
        Err(Ok(Error::InvalidAddress))
    );
}

#[test]
fn test_grant_and_is_verified() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let issuer = Address::generate(&env);

    let now = env.ledger().timestamp();
    client.grant_verification(
        &_admin,
        &issuer,
        &VerificationTier::EnhancedVerified,
        &(now + 1000),
    );

    let status = client.is_verified(&issuer);
    assert!(status.verified);
    assert_eq!(status.tier, VerificationTier::EnhancedVerified);
    assert_eq!(status.expiry_timestamp, now + 1000);
}

#[test]
fn test_is_verified_false_for_never_granted_issuer() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let issuer = Address::generate(&env);

    let status = client.is_verified(&issuer);
    assert!(!status.verified);
    assert_eq!(status.tier, VerificationTier::BasicVerified);
    assert_eq!(status.expiry_timestamp, 0);
}

#[test]
fn test_expired_verification_returns_not_verified() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let issuer = Address::generate(&env);

    let now = env.ledger().timestamp();
    client.grant_verification(
        &_admin,
        &issuer,
        &VerificationTier::BasicVerified,
        &(now + 100),
    );

    // Advance ledger time past expiry.
    env.ledger().set_timestamp(now + 200);

    let status = client.is_verified(&issuer);
    assert!(!status.verified);
    assert_eq!(status.tier, VerificationTier::BasicVerified);
    assert_eq!(status.expiry_timestamp, 0);
}

#[test]
fn test_grant_rejects_expiry_in_the_past() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let issuer = Address::generate(&env);

    let now = env.ledger().timestamp();
    // Advance so that `now` itself is in the past relative to the ledger
    // when we try to grant with that stale timestamp as expiry.
    env.ledger().set_timestamp(now + 100);

    assert_eq!(
        client.try_grant_verification(&_admin, &issuer, &VerificationTier::BasicVerified, &now),
        Err(Ok(Error::InvalidExpiry))
    );
}

#[test]
fn test_grant_rejects_zero_address_issuer() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let now = env.ledger().timestamp();

    assert_eq!(
        client.try_grant_verification(
            &admin,
            &zero_address(&env),
            &VerificationTier::BasicVerified,
            &(now + 1000)
        ),
        Err(Ok(Error::InvalidAddress))
    );
}

#[test]
fn test_non_admin_cannot_grant_verification() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let not_admin = Address::generate(&env);
    let issuer = Address::generate(&env);
    let now = env.ledger().timestamp();

    assert_eq!(
        client.try_grant_verification(
            &not_admin,
            &issuer,
            &VerificationTier::BasicVerified,
            &(now + 1000)
        ),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn test_revoke_immediately_invalidates_issuer() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let issuer = Address::generate(&env);
    let now = env.ledger().timestamp();

    client.grant_verification(
        &admin,
        &issuer,
        &VerificationTier::Institutional,
        &(now + 10_000),
    );
    assert!(client.is_verified(&issuer).verified);

    client.revoke_verification(&admin, &issuer);

    let status = client.is_verified(&issuer);
    assert!(!status.verified);
    assert_eq!(status.tier, VerificationTier::BasicVerified);
}

#[test]
fn test_non_admin_cannot_revoke_verification() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let not_admin = Address::generate(&env);
    let issuer = Address::generate(&env);
    let now = env.ledger().timestamp();

    client.grant_verification(
        &admin,
        &issuer,
        &VerificationTier::BasicVerified,
        &(now + 1000),
    );

    assert_eq!(
        client.try_revoke_verification(&not_admin, &issuer),
        Err(Ok(Error::Unauthorized))
    );
    // Still verified, since the unauthorized revoke must not have taken effect.
    assert!(client.is_verified(&issuer).verified);
}

#[test]
fn test_revoke_fails_when_never_granted() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let issuer = Address::generate(&env);

    assert_eq!(
        client.try_revoke_verification(&admin, &issuer),
        Err(Ok(Error::NotVerified))
    );
}

#[test]
fn test_grant_emits_verification_granted_event() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let issuer = Address::generate(&env);
    let now = env.ledger().timestamp();

    client.grant_verification(
        &admin,
        &issuer,
        &VerificationTier::Institutional,
        &(now + 500),
    );

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 2);
    let event_name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_name, Symbol::new(&env, "VerificationGranted"));
    let event_issuer: Address = topics.get(1).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_issuer, issuer);
    let (tier, expiry): (u32, u64) = data.try_into_val(&env).unwrap();
    assert_eq!(tier, VerificationTier::Institutional as u32);
    assert_eq!(expiry, now + 500);
}

#[test]
fn test_revoke_emits_verification_revoked_event() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let issuer = Address::generate(&env);
    let now = env.ledger().timestamp();

    client.grant_verification(
        &admin,
        &issuer,
        &VerificationTier::BasicVerified,
        &(now + 500),
    );
    client.revoke_verification(&admin, &issuer);

    let (topics, _data) = last_event(&env);
    assert_eq!(topics.len(), 2);
    let event_name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_name, Symbol::new(&env, "VerificationRevoked"));
    let event_issuer: Address = topics.get(1).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_issuer, issuer);
}

#[test]
fn test_re_granting_overwrites_prior_tier_and_expiry() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let issuer = Address::generate(&env);
    let now = env.ledger().timestamp();

    client.grant_verification(
        &admin,
        &issuer,
        &VerificationTier::BasicVerified,
        &(now + 100),
    );
    client.grant_verification(
        &admin,
        &issuer,
        &VerificationTier::Institutional,
        &(now + 9999),
    );

    let status = client.is_verified(&issuer);
    assert!(status.verified);
    assert_eq!(status.tier, VerificationTier::Institutional);
    assert_eq!(status.expiry_timestamp, now + 9999);
}
