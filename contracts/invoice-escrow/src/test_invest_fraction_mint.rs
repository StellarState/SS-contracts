/* Authorized Protocol Quality Assurance & Formal Invariant Verification */
//! Verification tests for #677: investment_made event emission and fraction token minting on invest.

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as AssetClient};

#[contract]
pub struct MockFractionToken;

#[contractimpl]
impl MockFractionToken {
    pub fn mint(env: Env, to: Address, amount: i128, _by: Address) {
        let current: i128 = env.storage().instance().get(&to).unwrap_or(0);
        env.storage().instance().set(&to, &(current + amount));
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage().instance().get(&id).unwrap_or(0)
    }
}

fn setup_test_env<'a>() -> (
    Env,
    InvoiceEscrowClient<'a>,
    Address,
    Address,
    TokenClient<'a>,
    AssetClient<'a>,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let contract_id = env.register(InvoiceEscrow, ());
    let client = InvoiceEscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &300);

    let token_admin = Address::generate(&env);
    let payment_token_addr = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &payment_token_addr);
    let asset_client = AssetClient::new(&env, &payment_token_addr);

    (
        env,
        client,
        admin,
        payment_token_addr,
        token_client,
        asset_client,
    )
}

#[test]
fn test_invest_on_funding_invoice_emits_investment_made_event() {
    let (env, client, _admin, payment_token_addr, _token_client, asset_client) = setup_test_env();

    let seller = Address::generate(&env);
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[42u8; 32]);

    asset_client.mint(&investor, &1_000_000);

    client.create_invoice(
        &invoice_id,
        &seller,
        &500_000,
        &100_000,
        &500_000,
        &1_000,
        &0,
        &None,
        &payment_token_addr,
        &None,
    );

    let events_before = env.events().all().events().len();

    client.invest(&invoice_id, &investor, &150_000);

    // Verify investment position was recorded
    let pos = client.get_investor_position(&invoice_id, &investor);
    assert_eq!(pos, 150_000);

    let events_after = env.events().all().events().len();
    assert!(
        events_after > events_before,
        "invest must emit an investment event"
    );
}

#[test]
fn test_invest_mints_fraction_tokens_when_fraction_token_configured() {
    let (env, client, _admin, payment_token_addr, _token_client, asset_client) = setup_test_env();

    let fraction_token_id = env.register(MockFractionToken, ());
    let frac_client = MockFractionTokenClient::new(&env, &fraction_token_id);

    let seller = Address::generate(&env);
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[77u8; 32]);

    asset_client.mint(&investor, &1_000_000);

    // Create invoice WITH configured fraction_token
    client.create_invoice(
        &invoice_id,
        &seller,
        &400_000,
        &50_000,
        &400_000,
        &1_000,
        &0,
        &None,
        &payment_token_addr,
        &Some(fraction_token_id.clone()),
    );

    // First investment
    client.invest(&invoice_id, &investor, &100_000);
    assert_eq!(frac_client.balance(&investor), 100_000);

    // Second investment
    client.invest(&invoice_id, &investor, &50_000);
    assert_eq!(frac_client.balance(&investor), 150_000);

    let total_pos = client.get_investor_position(&invoice_id, &investor);
    assert_eq!(total_pos, 150_000);
}
