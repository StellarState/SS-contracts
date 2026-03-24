#![cfg(test)]

use super::*;
use invoice_token::{InvoiceToken, InvoiceTokenClient};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as AssetClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, String as SorobanString, Symbol,
};

#[test]
fn test_integration_escrow_lifecycle_happy_path() {
    let env = Env::default();
    env.mock_all_auths();

    // 1. Setup Identities
    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let payer = Address::generate(&env);

    // 2. Register Escrow Contract
    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    // 3. Register Token Contract
    let inv_token_id = env.register_contract(None, InvoiceToken);
    let inv_token_client = InvoiceTokenClient::new(&env, &inv_token_id);

    // 4. Register Payment Token (Stellar Asset)
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token_client = TokenClient::new(&env, &payment_token_id.address());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());

    // 5. Initialize Contracts
    let invoice_id = Symbol::new(&env, "INV001");
    // Token initialization with escrow as minter
    inv_token_client.initialize(
        &admin,
        &SorobanString::from_str(&env, "Invoice #1"),
        &SorobanString::from_str(&env, "INV1"),
        &18,
        &invoice_id,
        &escrow_id,
    );

    // Escrow initialization
    escrow_client.initialize(&admin, &300); // 3% fee

    // 6. Fund Buyer Account
    let amount = 1000;
    payment_token_asset.mint(&buyer, &amount);
    payment_token_asset.mint(&payer, &amount);

    // 7. Create Escrow
    let due_date = 10000;
    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &amount,
        &due_date,
        &payment_token_id.address(),
        &inv_token_id,
    );

    // 8. Fund Escrow (Buyer buys the invoice)
    escrow_client.fund_escrow(&invoice_id, &buyer);

    // Verify buyer received invoice tokens and paid payment tokens
    assert_eq!(inv_token_client.balance(&buyer), amount);
    assert_eq!(payment_token_client.balance(&buyer), 0);
    assert_eq!(payment_token_client.balance(&escrow_id), amount);

    // 9. Record Payment (Payer settles the invoice)
    escrow_client.record_payment(&invoice_id, &payer, &amount);

    // Verify settlement balances after the payer transfers funds into escrow.
    // The investor receives the net proceeds, the admin receives the fee,
    // and the original funded principal remains locked in escrow.
    assert_eq!(payment_token_client.balance(&payer), 0);
    assert_eq!(payment_token_client.balance(&admin), 30);
    assert_eq!(payment_token_client.balance(&buyer), 970);
    assert_eq!(payment_token_client.balance(&escrow_id), 1000);

    // Status check
    assert_eq!(
        escrow_client.get_escrow_status(&invoice_id),
        EscrowStatus::Settled
    );
}

#[test]
fn test_integration_refund_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let inv_token_id = env.register_contract(None, InvoiceToken);
    let inv_token_client = InvoiceTokenClient::new(&env, &inv_token_id);

    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token_client = TokenClient::new(&env, &payment_token_id.address());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());

    let invoice_id = Symbol::new(&env, "INVFAIL");
    inv_token_client.initialize(
        &admin,
        &SorobanString::from_str(&env, "Failed Invoice"),
        &SorobanString::from_str(&env, "INV_FAIL"),
        &18,
        &invoice_id,
        &escrow_id,
    );

    escrow_client.initialize(&admin, &300);

    let amount = 1000;
    payment_token_asset.mint(&buyer, &amount);

    let due_date = 10000;
    env.ledger().set_timestamp(5000); // Current time is 5000

    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &amount,
        &due_date,
        &payment_token_id.address(),
        &inv_token_id,
    );

    escrow_client.fund_escrow(&invoice_id, &buyer);

    // Attempt refund before due date (should fail)
    let res = escrow_client.try_refund(&invoice_id);
    assert!(res.is_err());

    // Advance time beyond due date
    env.ledger().set_timestamp(10001);

    // Attempt refund (should pass)
    escrow_client.refund(&invoice_id);

    // Verify buyer got their funds back $1000
    assert_eq!(payment_token_client.balance(&buyer), 1000);
    assert_eq!(
        escrow_client.get_escrow_status(&invoice_id),
        EscrowStatus::Refunded
    );
}
