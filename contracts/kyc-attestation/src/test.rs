#![cfg(test)]
#![allow(deprecated)]

use soroban_sdk::testutils::{Address as _, Events, Ledger, LedgerInfo};
use soroban_sdk::{Address, BytesN, Env, String, Symbol, TryFromVal, TryIntoVal, Val, Vec};

use crate::{Error, KycAttestation, KycAttestationClient};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn zero_address(env: &Env) -> Address {
    Address::from_string(&String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ))
}

/// A deterministic 32-byte credential hash.
fn credential_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0xABu8; 32])
}

/// A second distinct hash for re-attestation tests.
fn credential_hash_2(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0xCDu8; 32])
}

/// Registers the contract and initialises it; returns `(oracle, client)`.
fn setup<'a>(env: &Env) -> (Address, KycAttestationClient<'a>) {
    env.mock_all_auths();
    let contract_id = env.register(KycAttestation, ());
    let client = KycAttestationClient::new(env, &contract_id);
    let oracle = Address::generate(env);
    client.initialize(&oracle);
    (oracle, client)
}

/// Returns the topics and data of the most recently emitted event.
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

/// Advance the ledger sequence by `n`.
fn advance_ledger(env: &Env, n: u32) {
    env.ledger().set(LedgerInfo {
        sequence_number: env.ledger().sequence() + n,
        ..env.ledger().get()
    });
}

// ── Initialisation ────────────────────────────────────────────────────────────

#[test]
fn test_initialize_stores_oracle() {
    let env = Env::default();
    let (oracle, client) = setup(&env);
    assert_eq!(client.get_oracle(), oracle);
}

#[test]
fn test_cannot_initialize_twice() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(KycAttestation, ());
    let client = KycAttestationClient::new(&env, &contract_id);

    let oracle = Address::generate(&env);
    client.initialize(&oracle);
    assert_eq!(client.try_initialize(&oracle), Err(Ok(Error::AlreadyInit)));
}

#[test]
fn test_initialize_rejects_zero_oracle() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(KycAttestation, ());
    let client = KycAttestationClient::new(&env, &contract_id);
    assert_eq!(
        client.try_initialize(&zero_address(&env)),
        Err(Ok(Error::InvalidAddress))
    );
}

#[test]
fn test_uninitialized_calls_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(KycAttestation, ());
    let client = KycAttestationClient::new(&env, &contract_id);

    let oracle = Address::generate(&env);
    let wallet = Address::generate(&env);
    let hash = credential_hash(&env);

    assert_eq!(
        client.try_attest(&oracle, &wallet, &hash, &1000),
        Err(Ok(Error::NotInit))
    );
    assert_eq!(client.try_revoke(&oracle, &wallet), Err(Ok(Error::NotInit)));
    assert_eq!(client.try_get_oracle(), Err(Ok(Error::NotInit)));
    assert_eq!(client.try_get_attestation(&wallet), Err(Ok(Error::NotInit)));
}

// ── attest ────────────────────────────────────────────────────────────────────

#[test]
fn test_attest_stores_attestation_correctly() {
    let env = Env::default();
    let (oracle, client) = setup(&env);
    let wallet = Address::generate(&env);
    let hash = credential_hash(&env);
    let current_seq = env.ledger().sequence();
    let expiry = current_seq + 500;

    client.attest(&oracle, &wallet, &hash, &expiry);

    let (returned_hash, attested_ledger, expiry_ledger) = client.get_attestation(&wallet);
    assert_eq!(returned_hash, hash);
    assert_eq!(attested_ledger, current_seq);
    assert_eq!(expiry_ledger, expiry);
}

#[test]
fn test_attest_restricted_to_oracle() {
    let env = Env::default();
    let (_oracle, client) = setup(&env);
    let impostor = Address::generate(&env);
    let wallet = Address::generate(&env);
    let hash = credential_hash(&env);

    assert_eq!(
        client.try_attest(&impostor, &wallet, &hash, &1000),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn test_attest_rejects_zero_wallet() {
    let env = Env::default();
    let (oracle, client) = setup(&env);
    let hash = credential_hash(&env);

    assert_eq!(
        client.try_attest(&oracle, &zero_address(&env), &hash, &1000),
        Err(Ok(Error::InvalidAddress))
    );
}

#[test]
fn test_attest_rejects_expiry_at_current_ledger() {
    let env = Env::default();
    let (oracle, client) = setup(&env);
    let wallet = Address::generate(&env);
    let hash = credential_hash(&env);
    let current = env.ledger().sequence();

    assert_eq!(
        client.try_attest(&oracle, &wallet, &hash, &current),
        Err(Ok(Error::InvalidExpiry))
    );
}

#[test]
fn test_attest_rejects_expiry_in_the_past() {
    let env = Env::default();
    let (oracle, client) = setup(&env);
    let wallet = Address::generate(&env);
    let hash = credential_hash(&env);

    // Advance ledger so a fixed low expiry falls in the past.
    advance_ledger(&env, 100);
    assert_eq!(
        client.try_attest(&oracle, &wallet, &hash, &50),
        Err(Ok(Error::InvalidExpiry))
    );
}

#[test]
fn test_reattest_replaces_previous_attestation() {
    let env = Env::default();
    let (oracle, client) = setup(&env);
    let wallet = Address::generate(&env);
    let hash1 = credential_hash(&env);
    let hash2 = credential_hash_2(&env);
    let current = env.ledger().sequence();

    client.attest(&oracle, &wallet, &hash1, &(current + 100));
    client.attest(&oracle, &wallet, &hash2, &(current + 200));

    let (returned_hash, _attested_ledger, expiry_ledger) = client.get_attestation(&wallet);
    assert_eq!(returned_hash, hash2);
    assert_eq!(expiry_ledger, current + 200);
}

// ── is_verified ───────────────────────────────────────────────────────────────

#[test]
fn test_is_verified_false_for_unattested_wallet() {
    let env = Env::default();
    let (_oracle, client) = setup(&env);
    let wallet = Address::generate(&env);
    assert!(!client.is_verified(&wallet));
}

#[test]
fn test_is_verified_true_for_live_attestation() {
    let env = Env::default();
    let (oracle, client) = setup(&env);
    let wallet = Address::generate(&env);
    let hash = credential_hash(&env);
    let current = env.ledger().sequence();

    client.attest(&oracle, &wallet, &hash, &(current + 500));
    assert!(client.is_verified(&wallet));
}

#[test]
fn test_is_verified_false_after_expiry() {
    let env = Env::default();
    let (oracle, client) = setup(&env);
    let wallet = Address::generate(&env);
    let hash = credential_hash(&env);
    let current = env.ledger().sequence();
    let expiry = current + 10;

    client.attest(&oracle, &wallet, &hash, &expiry);
    assert!(client.is_verified(&wallet));

    // Advance past expiry.
    advance_ledger(&env, 11);
    assert!(!client.is_verified(&wallet));
}

#[test]
fn test_is_verified_false_exactly_at_expiry_ledger() {
    // Expiry is *exclusive*: false when sequence == expiry_ledger.
    let env = Env::default();
    let (oracle, client) = setup(&env);
    let wallet = Address::generate(&env);
    let hash = credential_hash(&env);
    let current = env.ledger().sequence();
    let expiry = current + 10;

    client.attest(&oracle, &wallet, &hash, &expiry);
    advance_ledger(&env, 10); // sequence == expiry_ledger
    assert!(!client.is_verified(&wallet));
}

// ── revoke ────────────────────────────────────────────────────────────────────

#[test]
fn test_revoke_invalidates_attestation_immediately() {
    let env = Env::default();
    let (oracle, client) = setup(&env);
    let wallet = Address::generate(&env);
    let hash = credential_hash(&env);
    let current = env.ledger().sequence();

    client.attest(&oracle, &wallet, &hash, &(current + 500));
    assert!(client.is_verified(&wallet));

    client.revoke(&oracle, &wallet);
    assert!(!client.is_verified(&wallet));
}

#[test]
fn test_revoke_restricted_to_oracle() {
    let env = Env::default();
    let (oracle, client) = setup(&env);
    let impostor = Address::generate(&env);
    let wallet = Address::generate(&env);
    let hash = credential_hash(&env);
    let current = env.ledger().sequence();

    client.attest(&oracle, &wallet, &hash, &(current + 500));

    assert_eq!(
        client.try_revoke(&impostor, &wallet),
        Err(Ok(Error::Unauthorized))
    );
    // Attestation must still be valid after a failed revoke.
    assert!(client.is_verified(&wallet));
}

#[test]
fn test_revoke_unattested_wallet_returns_error() {
    let env = Env::default();
    let (oracle, client) = setup(&env);
    let wallet = Address::generate(&env);

    assert_eq!(
        client.try_revoke(&oracle, &wallet),
        Err(Ok(Error::NotAttested))
    );
}

#[test]
fn test_revoke_rejects_zero_wallet() {
    let env = Env::default();
    let (oracle, client) = setup(&env);

    assert_eq!(
        client.try_revoke(&oracle, &zero_address(&env)),
        Err(Ok(Error::InvalidAddress))
    );
}

// ── get_attestation ───────────────────────────────────────────────────────────

#[test]
fn test_get_attestation_returns_correct_data() {
    let env = Env::default();
    let (oracle, client) = setup(&env);
    let wallet = Address::generate(&env);
    let hash = credential_hash(&env);
    let current = env.ledger().sequence();
    let expiry = current + 300;

    client.attest(&oracle, &wallet, &hash, &expiry);

    let (returned_hash, attested_ledger, expiry_ledger) = client.get_attestation(&wallet);
    assert_eq!(returned_hash, hash);
    assert_eq!(attested_ledger, current);
    assert_eq!(expiry_ledger, expiry);
}

#[test]
fn test_get_attestation_error_for_unattested_wallet() {
    let env = Env::default();
    let (_oracle, client) = setup(&env);
    let wallet = Address::generate(&env);

    assert_eq!(
        client.try_get_attestation(&wallet),
        Err(Ok(Error::NotAttested))
    );
}

#[test]
fn test_get_attestation_still_readable_after_revocation() {
    // `get_attestation` is a data read; it should return the stored
    // attestation data even for revoked entries, so the oracle can audit the
    // credential that was revoked.
    let env = Env::default();
    let (oracle, client) = setup(&env);
    let wallet = Address::generate(&env);
    let hash = credential_hash(&env);
    let current = env.ledger().sequence();
    let expiry = current + 500;

    client.attest(&oracle, &wallet, &hash, &expiry);
    client.revoke(&oracle, &wallet);

    let (returned_hash, _attested_ledger, expiry_ledger) = client.get_attestation(&wallet);
    assert_eq!(returned_hash, hash);
    assert_eq!(expiry_ledger, expiry);
}

// ── Events ────────────────────────────────────────────────────────────────────

#[test]
fn test_wallet_attested_event_emitted_with_correct_data() {
    let env = Env::default();
    let (oracle, client) = setup(&env);
    let wallet = Address::generate(&env);
    let hash = credential_hash(&env);
    let current = env.ledger().sequence();
    let expiry = current + 200;

    client.attest(&oracle, &wallet, &hash, &expiry);

    let (topics, data) = last_event(&env);

    // topics[0] == "WalletAttested"
    let event_name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_name, Symbol::new(&env, "WalletAttested"));

    // topics[1] == wallet
    let event_wallet: Address = topics.get(1).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_wallet, wallet);

    // data == (credential_hash, attested_ledger, expiry_ledger)
    let (ev_hash, ev_attested, ev_expiry): (BytesN<32>, u32, u32) =
        data.try_into_val(&env).unwrap();
    assert_eq!(ev_hash, hash);
    assert_eq!(ev_attested, current);
    assert_eq!(ev_expiry, expiry);
}

#[test]
fn test_attestation_revoked_event_emitted_with_correct_data() {
    let env = Env::default();
    let (oracle, client) = setup(&env);
    let wallet = Address::generate(&env);
    let hash = credential_hash(&env);
    let current = env.ledger().sequence();

    client.attest(&oracle, &wallet, &hash, &(current + 500));

    advance_ledger(&env, 5);
    let revoke_ledger = env.ledger().sequence();
    client.revoke(&oracle, &wallet);

    let (topics, data) = last_event(&env);

    // topics[0] == "AttestationRevoked"
    let event_name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_name, Symbol::new(&env, "AttestationRevoked"));

    // topics[1] == wallet
    let event_wallet: Address = topics.get(1).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_wallet, wallet);

    // data == revoked_at_ledger
    let ev_revoked_at: u32 = data.try_into_val(&env).unwrap();
    assert_eq!(ev_revoked_at, revoke_ledger);
}
