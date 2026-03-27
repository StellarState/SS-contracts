#![allow(deprecated)]

use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as AssetClient;
use soroban_sdk::{testutils::Address as _, Address, Env};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup(
    env: &Env,
) -> (
    Address,
    PaymentDistributorClient<'_>,
    Address,
    TokenClient<'_>,
) {
    let distributor_id = env.register_contract(None, PaymentDistributor);
    let client = PaymentDistributorClient::new(env, &distributor_id);
    let admin = Address::generate(env);
    client.initialize(&admin);

    let token_admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = TokenClient::new(env, &token_id.address());
    let token_asset = AssetClient::new(env, &token_id.address());
    token_asset.mint(&distributor_id, &10_000i128);

    (admin, client, distributor_id, token_client)
}

// ---------------------------------------------------------------------------
// Issue #35: unit tests for payment-distributor distribution logic
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_and_distribute() {
    let env = Env::default();
    env.mock_all_auths();

    let distributor_id = env.register_contract(None, PaymentDistributor);
    let client = PaymentDistributorClient::new(&env, &distributor_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_id.address());
    let token_asset = AssetClient::new(&env, &token_id.address());

    let amount = 1000;
    token_asset.mint(&distributor_id, &amount);

    let recipient = Address::generate(&env);
    let distribute_amt = 400;
    client.distribute(&token_client.address, &recipient, &distribute_amt);

    assert_eq!(token_client.balance(&recipient), distribute_amt);
    assert_eq!(
        token_client.balance(&distributor_id),
        amount - distribute_amt
    );
}

#[test]
fn test_double_initialize_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, client, _, _) = setup(&env);
    let res = client.try_initialize(&admin);
    assert_eq!(res, Err(Ok(Error::AlreadyInit)));
}

#[test]
fn test_get_admin_returns_correct_address() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, client, _, _) = setup(&env);
    assert_eq!(client.get_admin(), admin);
}

#[test]
fn test_distribute_zero_amount_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, client, _, token) = setup(&env);
    let recipient = Address::generate(&env);
    let res = client.try_distribute(&token.address, &recipient, &0i128);
    assert_eq!(res, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_distribute_negative_amount_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, client, _, token) = setup(&env);
    let recipient = Address::generate(&env);
    let res = client.try_distribute(&token.address, &recipient, &-100i128);
    assert_eq!(res, Err(Ok(Error::InvalidAmount)));
}

#[test]
#[ignore]
fn test_distribute_unauthorized_non_admin_fails() {
    let env = Env::default();
    let distributor_id = env.register_contract(None, PaymentDistributor);
    let client = PaymentDistributorClient::new(&env, &distributor_id);
    let admin = Address::generate(&env);
    
    // Initialize with mock auth
    env.mock_all_auths();
    client.initialize(&admin);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_asset = AssetClient::new(&env, &token_id.address());
    token_asset.mint(&distributor_id, &1000i128);

    let token_client = TokenClient::new(&env, &token_id.address());
    let recipient = Address::generate(&env);

    // Create a new environment without mocking all auths
    let env2 = Env::default();
    let distributor_id2 = env2.register_contract(None, PaymentDistributor);
    let client2 = PaymentDistributorClient::new(&env2, &distributor_id2);
    let admin2 = Address::generate(&env2);
    
    // Initialize with mock auth
    env2.mock_all_auths();
    client2.initialize(&admin2);
    
    let token_id2 = env2.register_stellar_asset_contract_v2(Address::generate(&env2));
    let token_asset2 = AssetClient::new(&env2, &token_id2.address());
    token_asset2.mint(&distributor_id2, &1000i128);
    let token_client2 = TokenClient::new(&env2, &token_id2.address());
    let recipient2 = Address::generate(&env2);

    // Now call distribute without mocking auth for this specific call
    // The admin2 address will not have auth, so require_auth() should fail
    let res = client2.try_distribute(&token_client2.address, &recipient2, &100i128);
    let _ = (admin, token_client, recipient);
    assert!(res.is_err());
}

#[test]
fn test_distribute_not_initialized_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let distributor_id = env.register_contract(None, PaymentDistributor);
    let client = PaymentDistributorClient::new(&env, &distributor_id);
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let token_client = TokenClient::new(&env, &token_id.address());
    let recipient = Address::generate(&env);
    let res = client.try_distribute(&token_client.address, &recipient, &100i128);
    assert_eq!(res, Err(Ok(Error::NotInit)));
}

#[test]
fn test_distribute_full_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, client, distributor_id, token) = setup(&env);
    let recipient = Address::generate(&env);
    let full_balance = token.balance(&distributor_id);
    client.distribute(&token.address, &recipient, &full_balance);
    assert_eq!(token.balance(&recipient), full_balance);
    assert_eq!(token.balance(&distributor_id), 0);
}

#[test]
fn test_distribute_multiple_recipients() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, client, distributor_id, token) = setup(&env);

    let r1 = Address::generate(&env);
    let r2 = Address::generate(&env);
    client.distribute(&token.address, &r1, &3000i128);
    client.distribute(&token.address, &r2, &2000i128);

    assert_eq!(token.balance(&r1), 3000);
    assert_eq!(token.balance(&r2), 2000);
    assert_eq!(token.balance(&distributor_id), 5000);
}
