#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, BytesN, Env, IntoVal,
};

#[test]
fn test_default_limit_and_custom_setting() {
    let env = Env::default();
    let contract_id = env.register(DiversificationLimit, ());
    let client = DiversificationLimitClient::new(&env, &contract_id);

    let wallet = Address::generate(&env);
    // Default limit should be 3000 bps (30%)
    assert_eq!(client.get_diversification_limit(&wallet), 3000);

    // Setting custom limit
    env.mock_all_auths();
    client.set_diversification_limit(&wallet, &2500);
    assert_eq!(client.get_diversification_limit(&wallet), 2500);

    // Invalid limits should fail
    let err_zero = client.try_set_diversification_limit(&wallet, &0);
    assert_eq!(err_zero, Err(Ok(Error::InvalidLimit)));

    let err_over = client.try_set_diversification_limit(&wallet, &10001);
    assert_eq!(err_over, Err(Ok(Error::InvalidLimit)));
}

#[test]
fn test_check_diversification_scenarios() {
    let env = Env::default();
    let contract_id = env.register(DiversificationLimit, ());
    let client = DiversificationLimitClient::new(&env, &contract_id);

    let wallet = Address::generate(&env);
    let inv1 = BytesN::from_array(&env, &[1u8; 32]);
    let inv2 = BytesN::from_array(&env, &[2u8; 32]);

    env.mock_all_auths();
    // Set base portfolio of 10,000
    client.set_portfolio_total(&wallet, &10000);

    // Default limit is 30% (3000 bps). An investment of 2000 into inv1 gives 20%
    let check = client.check_diversification(&wallet, &inv1, &2000);
    assert!(check.eligible);
    assert!(!check.breach);
    assert_eq!(check.current_bps, 0);
    assert_eq!(check.projected_bps, 1666); // 2000 / 12000 = 16.66%
    assert_eq!(check.max_bps, 3000);

    // Check an investment that breaches 30% (e.g. 5000: 5000 / 15000 = 33.33% = 3333 bps)
    let breach_check = client.check_diversification(&wallet, &inv1, &5000);
    assert!(!breach_check.eligible);
    assert!(breach_check.breach);
    assert_eq!(breach_check.projected_bps, 3333);
    assert_eq!(breach_check.max_bps, 3000);

    // Negative amount check
    let neg_check = client.try_check_diversification(&wallet, &inv1, &-100);
    assert_eq!(neg_check, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_invest_enforces_limit_and_blocks_breach() {
    let env = Env::default();
    let contract_id = env.register(DiversificationLimit, ());
    let client = DiversificationLimitClient::new(&env, &contract_id);

    let wallet = Address::generate(&env);
    let inv1 = BytesN::from_array(&env, &[1u8; 32]);
    let inv2 = BytesN::from_array(&env, &[2u8; 32]);
    let inv3 = BytesN::from_array(&env, &[3u8; 32]);

    env.mock_all_auths();
    // Allow up to 40% (4000 bps)
    client.set_diversification_limit(&wallet, &4000);
    client.set_portfolio_total(&wallet, &10000);

    // Invest 2000 in inv1 -> 2000 / 12000 = 16.66% (<= 40%) -> OK
    client.invest(&wallet, &inv1, &2000);
    assert_eq!(client.get_position(&wallet, &inv1), 2000);

    // Invest 3000 in inv2 -> 3000 / 15000 = 20% (<= 40%) -> OK
    client.invest(&wallet, &inv2, &3000);
    assert_eq!(client.get_position(&wallet, &inv2), 3000);

    // Now try to invest 8000 more in inv1
    // Total becomes 15000 + 8000 = 23000, inv1 becomes 10000
    // 10000 / 23000 = 43.47% = 4347 bps > 4000 bps -> blocked!
    let res = client.try_invest(&wallet, &inv1, &8000);
    assert_eq!(res, Err(Ok(Error::DiversificationLimitExceeded)));

    // Position in inv1 should remain 2000
    assert_eq!(client.get_position(&wallet, &inv1), 2000);

    // Invest within limit in inv3 -> 2000 / 17000 = 11.76% -> OK
    client.invest(&wallet, &inv3, &2000);
    assert_eq!(client.get_position(&wallet, &inv3), 2000);
}

#[test]
fn test_get_portfolio_concentration() {
    let env = Env::default();
    let contract_id = env.register(DiversificationLimit, ());
    let client = DiversificationLimitClient::new(&env, &contract_id);

    let wallet = Address::generate(&env);
    let inv1 = BytesN::from_array(&env, &[1u8; 32]);
    let inv2 = BytesN::from_array(&env, &[2u8; 32]);

    env.mock_all_auths();
    client.set_diversification_limit(&wallet, &5000); // 50%
    client.set_portfolio_total(&wallet, &10000);

    client.invest(&wallet, &inv1, &2500);
    client.invest(&wallet, &inv2, &2500);

    let concentrations = client.get_portfolio_concentration(&wallet);
    assert_eq!(concentrations.len(), 2);

    let c1 = concentrations.get(0).unwrap();
    assert_eq!(c1.invoice_id, inv1);
    assert_eq!(c1.amount, 2500);
    // 2500 / 10000 = 25.00% = 2500 bps
    assert_eq!(c1.concentration_bps, 2500);

    let c2 = concentrations.get(1).unwrap();
    assert_eq!(c2.invoice_id, inv2);
    assert_eq!(c2.amount, 2500);
    assert_eq!(c2.concentration_bps, 2500);
}
