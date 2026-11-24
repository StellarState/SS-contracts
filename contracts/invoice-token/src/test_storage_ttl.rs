//! Storage TTL and retention regression tests for balance, allowance and
//! freeze entries (#773).

use super::{InvoiceToken, InvoiceTokenClient};
use crate::types::StorageKey;
use soroban_sdk::testutils::storage::Persistent as _;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env, String as SorobanString, Symbol};

fn setup(env: &Env) -> (InvoiceTokenClient<'_>, Address, Address, Address) {
    env.mock_all_auths();
    env.ledger().set_sequence_number(100);
    let id = env.register(InvoiceToken, ());
    let client = InvoiceTokenClient::new(env, &id);
    let admin = Address::generate(env);
    let minter = Address::generate(env);
    client.initialize(
        &admin,
        &SorobanString::from_str(env, "TTL Token"),
        &SorobanString::from_str(env, "TTL"),
        &7u32,
        &Symbol::new(env, "ttl_inv"),
        &minter,
        &i128::MAX,
        &1,
    );
    let holder = Address::generate(env);
    client.mint(&holder, &1_000, &admin);
    (client, id, minter, holder)
}

/// Absolute ledger until which `key` stays live.
fn live_until(env: &Env, id: &Address, key: &StorageKey) -> u32 {
    env.as_contract(id, || {
        env.ledger().sequence() + env.storage().persistent().get_ttl(key)
    })
}

#[test]
fn balance_reads_never_shorten_retention_and_keep_the_balance() {
    let env = Env::default();
    let (client, id, _, holder) = setup(&env);
    let key = StorageKey::Balance(holder.clone());
    let before = live_until(&env, &id, &key);

    env.ledger().set_sequence_number(600);
    assert_eq!(client.balance(&holder), 1_000);
    assert!(live_until(&env, &id, &key) >= before);
}

#[test]
fn allowance_is_live_through_its_expiration_ledger_only() {
    let env = Env::default();
    let (client, _, _, holder) = setup(&env);
    let spender = Address::generate(&env);
    client.approve(&holder, &spender, &500, &200);

    env.ledger().set_sequence_number(200);
    assert_eq!(client.allowance(&holder, &spender), 500);

    // Past expiry the entry may still exist but must not be treated as live.
    env.ledger().set_sequence_number(201);
    assert_eq!(client.allowance(&holder, &spender), 0);
    assert!(client
        .try_extend_allowance(&holder, &spender, &300)
        .is_err());
    assert_eq!(client.balance(&holder), 1_000);
}

#[test]
fn allowance_extension_never_shortens_expiration() {
    let env = Env::default();
    let (client, _, _, holder) = setup(&env);
    let spender = Address::generate(&env);
    client.approve(&holder, &spender, &500, &200);

    assert!(client
        .try_extend_allowance(&holder, &spender, &150)
        .is_err());
    assert!(client
        .try_extend_allowance(&holder, &spender, &200)
        .is_err());
    client.extend_allowance(&holder, &spender, &300);

    env.ledger().set_sequence_number(250);
    assert_eq!(client.allowance(&holder, &spender), 500);
}

#[test]
fn freeze_state_survives_ledger_advance_and_unfreeze_removes_the_key() {
    let env = Env::default();
    let (client, id, _, holder) = setup(&env);
    client.freeze_account(&holder);

    env.ledger().set_sequence_number(2_100);
    assert!(client.is_frozen(&holder));
    assert_eq!(client.balance(&holder), 1_000);

    client.unfreeze_account(&holder);
    let has_key = env.as_contract(&id, || {
        env.storage()
            .persistent()
            .has(&StorageKey::Frozen(holder.clone()))
    });
    assert!(!has_key);
}
