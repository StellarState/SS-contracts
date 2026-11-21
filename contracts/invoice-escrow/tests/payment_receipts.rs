use invoice_escrow::{EscrowStatus, InvoiceEscrow, InvoiceEscrowClient};
use soroban_sdk::{
    contract, contractimpl, testutils::Address as _, token::StellarAssetClient, Address, BytesN,
    Env, Symbol,
};

#[contract]
struct MockInvoiceToken;

#[contractimpl]
impl MockInvoiceToken {
    pub fn mint(_env: Env, _to: Address, _amount: i128, _by: Address) {}

    pub fn decimals(_env: Env) -> u32 {
        7
    }

    pub fn set_transfer_locked(_env: Env, _caller: Address, _locked: bool) {}
}

fn commitment(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[9; 32])
}

#[test]
fn payment_receipts_are_ordered_paginated_and_match_cumulative_payment() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let client = InvoiceEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let debtor = Address::generate(&env);
    let buyer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let token = token_id.address();
    let payment_assets = StellarAssetClient::new(&env, &token);
    let invoice_token = env.register(MockInvoiceToken, ());
    let invoice_id = Symbol::new(&env, "RECEIPTS");

    client.initialize(&admin, &0);
    payment_assets.mint(&buyer, &1_000);
    payment_assets.mint(&debtor, &1_000);
    client.create_escrow_legacy(
        &invoice_id,
        &seller,
        &debtor,
        &1_000,
        &1_000,
        &1_000_000,
        &token,
        &invoice_token,
        &commitment(&env),
        &None,
        &None,
    );
    client.fund_escrow_legacy(&invoice_id, &buyer, &1_000);
    client.record_payment(&invoice_id, &debtor, &400);
    client.record_payment(&invoice_id, &debtor, &600);

    let first = client.get_payment_receipts(&invoice_id, &0, &1);
    assert_eq!(first.receipts.len(), 1);
    let first_receipt = first.receipts.get(0).unwrap();
    assert_eq!(first_receipt.sequence, 1);
    assert_eq!(first_receipt.payer, debtor);
    assert_eq!(first_receipt.amount, 400);
    assert_eq!(first.next_cursor, 1);
    assert_eq!(first.total_count, 2);

    let second = client.get_payment_receipts(&invoice_id, &first.next_cursor, &1);
    assert_eq!(second.receipts.len(), 1);
    let second_receipt = second.receipts.get(0).unwrap();
    assert_eq!(second_receipt.sequence, 2);
    assert_eq!(second_receipt.amount, 600);
    assert_eq!(
        first_receipt.amount + second_receipt.amount,
        client.get_escrow(&invoice_id).paid_amt
    );
    assert_eq!(client.get_payment_receipt_count(&invoice_id), 2);
    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Settled);

    assert_eq!(
        client.try_get_payment_receipts(&invoice_id, &3, &1),
        Err(Ok(invoice_escrow::Error::InvalidCursor))
    );
}

#[test]
fn payment_receipt_history_is_bounded() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let client = InvoiceEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let debtor = Address::generate(&env);
    let buyer = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token = token_id.address();
    let payment_assets = StellarAssetClient::new(&env, &token);
    let invoice_token = env.register(MockInvoiceToken, ());
    let invoice_id = Symbol::new(&env, "RECEIPT_CAP");

    client.initialize(&admin, &0);
    payment_assets.mint(&buyer, &257);
    payment_assets.mint(&debtor, &257);
    client.create_escrow_legacy(
        &invoice_id,
        &seller,
        &debtor,
        &257,
        &257,
        &1_000_000,
        &token,
        &invoice_token,
        &commitment(&env),
        &None,
        &None,
    );
    client.fund_escrow_legacy(&invoice_id, &buyer, &257);
    for _ in 0..256 {
        client.record_payment(&invoice_id, &debtor, &1);
    }

    assert_eq!(client.get_payment_receipt_count(&invoice_id), 256);
    assert_eq!(
        client.try_record_payment(&invoice_id, &debtor, &1),
        Err(Ok(invoice_escrow::Error::ReceiptLimitReached))
    );
}
