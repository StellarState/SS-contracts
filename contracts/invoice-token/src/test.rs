//! Unit tests for the invoice token contract.

use super::{InvoiceToken, InvoiceTokenClient};
use soroban_sdk::{testutils::Address as _, Address, Env, String as SorobanString, Symbol};

fn setup_token(env: &Env) -> (InvoiceTokenClient, Address, Address) {
    let contract_id = env.register(InvoiceToken, ());
    let client = InvoiceTokenClient::new(&env, &contract_id);
    let admin = Address::generate(env);
    let minter = Address::generate(env);
    let name = SorobanString::from_str(env, "Invoice INV-001");
    let symbol = SorobanString::from_str(env, "INV001");
    let invoice_id = Symbol::new(env, "inv_001");
    client.initialize(&admin, &name, &symbol, &7u32, &invoice_id, &minter);
    (client, admin, minter)
}

#[test]
fn test_initialize_and_metadata() {
    let env = Env::default();
    let (client, admin, _minter) = setup_token(&env);

    assert_eq!(
        client.name(),
        SorobanString::from_str(&env, "Invoice INV-001")
    );
    assert_eq!(client.symbol(), SorobanString::from_str(&env, "INV001"));
    assert_eq!(client.decimals(), 7);
    assert_eq!(client.total_supply(), 0);
    assert_eq!(client.balance(&admin), 0);
    assert_eq!(client.invoice_id(), Symbol::new(&env, "inv_001"));
    assert!(client.transfer_locked());

    let other = Address::generate(&env);
    assert_eq!(client.balance(&other), 0);
    assert_eq!(client.allowance(&admin, &other), 0);
}
