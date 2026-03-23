#![cfg(test)]
#![allow(deprecated)]

use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as AssetClient;
use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, Env, Symbol};

#[contract]
struct MockInvoiceToken;

#[contractimpl]
impl MockInvoiceToken {
    pub fn mint(env: Env, to: Address, amount: i128, _by: Address) {
        // Just mock the mint call
        env.storage().instance().set(&to, &amount);
    }
}

#[test]
fn test_create_and_fund() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

    let admin = Address::generate(&env);

    // Register the payment token
    let payment_token_admin = Address::generate(&env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = TokenClient::new(&env, &payment_token_id.address());
    let payment_token_asset = AssetClient::new(&env, &payment_token_id.address());

    // Register our mock invoice token
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    // Initialize escrow contract
    escrow_client.initialize(&admin, &300); // 3% fee

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let invoice_id = Symbol::new(&env, "INV123");
    let amount = 1000;

    // Buyer gets payment tokens
    payment_token_asset.mint(&buyer, &2000);

    // Create escrow
    escrow_client.create_escrow(
        &invoice_id,
        &seller,
        &amount,
        &1000000,
        &payment_token.address,
        &inv_token_id,
    );

    // Fund escrow
    escrow_client.fund_escrow(&invoice_id, &buyer);

    // Check status
    let status = escrow_client.get_escrow_status(&invoice_id);
    assert_eq!(status, EscrowStatus::Funded);

    // Check tokens transferred to escrow
    assert_eq!(payment_token.balance(&escrow_id), 1000);
    assert_eq!(payment_token.balance(&buyer), 1000);
}
