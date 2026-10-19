#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Env};

#[test]
fn test_yield_adjustment_rules() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(YieldAdjustment, ());
    let client = YieldAdjustmentClient::new(&env, &contract_id);

    client.initialize(&admin);

    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    // Less than 5 ratings -> 0 bps adjustment
    for i in 0..4 {
        let user = Address::generate(&env);
        client.record_rating(&user, &invoice_id, &5);
    }
    let info = client.get_yield_adjustment(&invoice_id);
    assert_eq!(info.total_ratings, 4);
    assert_eq!(info.adjustment_bps, 0);

    // 5th rating makes 5 ratings with average 5.0 (>= 4.5) -> +25 bps bonus
    let user5 = Address::generate(&env);
    client.record_rating(&user5, &invoice_id, &5);
    let info2 = client.get_yield_adjustment(&invoice_id);
    assert_eq!(info2.total_ratings, 5);
    assert_eq!(info2.basis_rating_x100, 500);
    assert_eq!(info2.adjustment_bps, 25);

    // Test settlement hook
    let adjusted_yield = client.apply_settlement_adjustment(&invoice_id, &1000);
    assert_eq!(adjusted_yield, 1025);

    // Test invoice with rating between 4.0 and 4.5 -> 0 bps
    let invoice_mid = BytesN::from_array(&env, &[2u8; 32]);
    client.set_invoice_rating(&invoice_mid, &420, &5); // 4.2 rating, 5 reviews
    let info_mid = client.get_yield_adjustment(&invoice_mid);
    assert_eq!(info_mid.adjustment_bps, 0);
    assert_eq!(info_mid.basis_rating_x100, 420);

    // Test invoice with rating below 4.0 -> -10 bps discount
    let invoice_low = BytesN::from_array(&env, &[3u8; 32]);
    client.set_invoice_rating(&invoice_low, &350, &5); // 3.5 rating, 5 reviews
    let info_low = client.get_yield_adjustment(&invoice_low);
    assert_eq!(info_low.adjustment_bps, -10);
    assert_eq!(info_low.basis_rating_x100, 350);

    let settled_low = client.apply_settlement_adjustment(&invoice_low, &1000);
    assert_eq!(settled_low, 990);
}
