#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Env,
};

fn setup() -> (Env, OvercollateralRewardClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, OvercollateralReward);
    let client = OvercollateralRewardClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin).unwrap();
    (env, client, admin)
}

#[test]
fn test_check_overcollateral_no_record() {
    let (env, client, _admin) = setup();
    let issuer = Address::generate(&env);
    let (is_over, excess, days) = client.check_overcollateral(&issuer).unwrap();
    assert!(!is_over);
    assert_eq!(excess, 0);
    assert_eq!(days, 0);
}

#[test]
fn test_record_and_check_overcollateral() {
    let (env, client, _admin) = setup();
    let issuer = Address::generate(&env);
    client.record_collateral(&issuer, &1500, &1000).unwrap();
    let (is_over, excess, _days) = client.check_overcollateral(&issuer).unwrap();
    assert!(is_over);
    assert_eq!(excess, 500);
}

#[test]
fn test_grant_discount_before_30_days_fails() {
    let (env, client, _admin) = setup();
    let issuer = Address::generate(&env);
    client.record_collateral(&issuer, &1500, &1000).unwrap();
    assert_eq!(
        client.grant_fee_discount(&issuer),
        Err(Ok(Error::StreakTooShort))
    );
}

#[test]
fn test_grant_discount_after_30_days_succeeds() {
    let (env, client, _admin) = setup();
    let issuer = Address::generate(&env);
    client.record_collateral(&issuer, &1500, &1000).unwrap();
    // Advance 31 days.
    env.ledger().with_mut(|l| l.timestamp += 31 * 86_400);
    client.grant_fee_discount(&issuer).unwrap();
    let status = client.get_collateral_reward_status(&issuer).unwrap();
    assert!(status.discount_pending);
    assert_eq!(status.discount_bps, 1000);
}

#[test]
fn test_streak_broken_on_under_collateral() {
    let (env, client, _admin) = setup();
    let issuer = Address::generate(&env);
    client.record_collateral(&issuer, &1500, &1000).unwrap();
    client.record_collateral(&issuer, &500, &1000).unwrap();
    let (is_over, excess, days) = client.check_overcollateral(&issuer).unwrap();
    assert!(!is_over);
    assert_eq!(excess, 0);
    assert_eq!(days, 0);
}

#[test]
fn test_consume_discount() {
    let (env, client, _admin) = setup();
    let issuer = Address::generate(&env);
    client.record_collateral(&issuer, &1500, &1000).unwrap();
    env.ledger().with_mut(|l| l.timestamp += 31 * 86_400);
    client.grant_fee_discount(&issuer).unwrap();
    let bps = client.consume_discount(&issuer).unwrap();
    assert_eq!(bps, 1000);
    // Second consume returns 0.
    assert_eq!(client.consume_discount(&issuer).unwrap(), 0);
}

#[test]
fn test_pending_reward_survives_unqualified_duplicate_grant() {
    let (env, client, _admin) = setup();
    let issuer = Address::generate(&env);
    client.record_collateral(&issuer, &1500, &1000).unwrap();
    env.ledger().with_mut(|l| l.timestamp += 31 * 86_400);
    client.grant_fee_discount(&issuer).unwrap();

    client.record_collateral(&issuer, &500, &1000).unwrap();
    assert_eq!(
        client.grant_fee_discount(&issuer),
        Err(Ok(Error::DiscountAlreadyPending))
    );
    let status = client.get_collateral_reward_status(&issuer).unwrap();
    assert!(status.discount_pending);
    assert_eq!(status.discount_bps, 1000);

    assert_eq!(client.consume_discount(&issuer).unwrap(), 1000);
    assert_eq!(client.consume_discount(&issuer).unwrap(), 0);
    assert_eq!(
        client.grant_fee_discount(&issuer),
        Err(Ok(Error::NotOvercollateral))
    );
}

#[test]
fn test_one_discount_per_streak_and_requalification_after_break() {
    let (env, client, _admin) = setup();
    let issuer = Address::generate(&env);
    client.record_collateral(&issuer, &1500, &1000).unwrap();
    env.ledger().with_mut(|l| l.timestamp += 31 * 86_400);

    client.grant_fee_discount(&issuer).unwrap();
    assert_eq!(
        client.grant_fee_discount(&issuer),
        Err(Ok(Error::DiscountAlreadyPending))
    );
    assert_eq!(client.consume_discount(&issuer).unwrap(), 1000);
    assert_eq!(client.consume_discount(&issuer).unwrap(), 0);
    assert_eq!(
        client.grant_fee_discount(&issuer),
        Err(Ok(Error::DiscountAlreadyRewarded))
    );

    let status = client.get_collateral_reward_status(&issuer).unwrap();
    assert!(!status.discount_pending);
    assert!(status.discount_consumed);
    assert!(!status.discount_eligible);

    client.record_collateral(&issuer, &500, &1000).unwrap();
    assert_eq!(
        client.grant_fee_discount(&issuer),
        Err(Ok(Error::NotOvercollateral))
    );
    client.record_collateral(&issuer, &1500, &1000).unwrap();

    env.ledger().with_mut(|l| l.timestamp += 29 * 86_400);
    assert_eq!(
        client.grant_fee_discount(&issuer),
        Err(Ok(Error::StreakTooShort))
    );
    let status = client.get_collateral_reward_status(&issuer).unwrap();
    assert!(!status.discount_pending);
    assert!(!status.discount_consumed);
    assert!(!status.discount_eligible);

    env.ledger().with_mut(|l| l.timestamp += 86_400);
    let status = client.get_collateral_reward_status(&issuer).unwrap();
    assert!(status.discount_eligible);
    client.grant_fee_discount(&issuer).unwrap();
    assert_eq!(client.consume_discount(&issuer).unwrap(), 1000);
}
