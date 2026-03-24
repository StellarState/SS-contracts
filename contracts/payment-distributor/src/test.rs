#![cfg(test)]
#![allow(deprecated)]

use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as AssetClient;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_initialize_and_distribute() {
    let env = Env::default();
    env.mock_all_auths();

    let distributor_id = env.register_contract(None, PaymentDistributor);
    let client = PaymentDistributorClient::new(&env, &distributor_id);

    let admin = Address::generate(&env);

    // Initialize
    client.initialize(&admin);

    // Register token
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_id.address());
    let token_asset = AssetClient::new(&env, &token_id.address());

    // Mint token to contract
    let amount = 1000;
    token_asset.mint(&distributor_id, &amount);

    let recipient = Address::generate(&env);

    // Distribute
    let distribute_amt = 400;
    client.distribute(&token_client.address, &recipient, &distribute_amt);

    assert_eq!(token_client.balance(&recipient), distribute_amt);
    assert_eq!(
        token_client.balance(&distributor_id),
        amount - distribute_amt
    );
}
