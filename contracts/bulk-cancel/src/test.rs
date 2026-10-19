#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Env};

#[test]
fn test_cancel_all_orders_and_batch() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token_contract.address();
    let token_client = token::Client::new(&env, &token_addr);
    let asset_client = token::StellarAssetClient::new(&env, &token_addr);

    let contract_id = env.register(BulkCancel, ());
    let client = BulkCancelClient::new(&env, &contract_id);

    client.initialize(&token_addr);

    let buyer = Address::generate(&env);
    asset_client.mint(&buyer, &10_000);

    let invoice_1 = BytesN::from_array(&env, &[1u8; 32]);
    let invoice_2 = BytesN::from_array(&env, &[2u8; 32]);

    let o1 = client.place_buy_order(&buyer, &invoice_1, &100, &1000);
    let o2 = client.place_buy_order(&buyer, &invoice_1, &200, &2000);
    let o3 = client.place_buy_order(&buyer, &invoice_2, &300, &3000);

    assert_eq!(token_client.balance(&buyer), 4000);

    // Cancel all on invoice_1
    let (count, refunded) = client.cancel_all_orders(&buyer, &invoice_1);
    assert_eq!(count, 2);
    assert_eq!(refunded, 3000);
    assert_eq!(token_client.balance(&buyer), 7000);

    // Cancel o3 via batch
    let batch = vec![&env, o3];
    let (count_batch, refunded_batch) = client.cancel_orders_batch(&buyer, &batch);
    assert_eq!(count_batch, 1);
    assert_eq!(refunded_batch, 3000);
    assert_eq!(token_client.balance(&buyer), 10_000);

    // Check batch too large error
    let mut large_batch = vec![&env];
    for i in 0..21 {
        large_batch.push_back(i);
    }
    let res = client.try_cancel_orders_batch(&buyer, &large_batch);
    assert_eq!(res, Err(Ok(Error::BatchTooLarge)));
}
