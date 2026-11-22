#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::{
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient as AssetClient},
    Address, BytesN, Env,
};

fn setup_token(env: &Env, mint_to: &Address, amount: i128) -> Address {
    let token_admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let asset = AssetClient::new(env, &token_id.address());
    asset.mint(mint_to, &amount);
    token_id.address()
}

#[test]
fn test_auto_invest_rule_and_execution() {
    let env = Env::default();
    // Operator invokes; token.transfer still needs the investor's auth.
    env.mock_all_auths_allowing_non_root_auth();

    let contract_id = env.register(AutoInvest, ());
    let client = AutoInvestClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let investor = Address::generate(&env);
    let token = setup_token(&env, &investor, 10_000);

    client.initialize(&admin, &operator);

    let criteria = AutoInvestCriteria {
        min_yield_bps: 800, // 8%
        max_duration_days: 60,
        max_risk_tier: 2,
        token: token.clone(),
    };

    client.set_auto_invest_rule(&investor, &criteria, &500, &Some(2000));

    let rule = client.get_auto_invest_rule(&investor);
    assert_eq!(rule.max_amount_per_invoice, 500);
    assert_eq!(rule.total_budget, 2000);
    assert_eq!(rule.total_invested, 0);
    assert!(rule.active);

    // Register invoice matching criteria: 10% yield, 30 days duration, tier 1
    let id = BytesN::from_array(&env, &[1u8; 32]);
    client.register_listing(&id, &1000, &30, &1, &token, &1000);

    // Execute auto-invest
    let invested = client.execute_auto_invest(&operator, &investor, &id);
    assert_eq!(invested, 500);

    let updated_rule = client.get_auto_invest_rule(&investor);
    assert_eq!(updated_rule.total_invested, 500);

    let updated_listing = client.get_listing(&id);
    assert_eq!(updated_listing.funding_needed, 500);
    assert!(updated_listing.is_open);

    let token_client = TokenClient::new(&env, &token);
    assert_eq!(token_client.balance(&contract_id), 500);
    assert_eq!(token_client.balance(&investor), 9_500);
}

#[test]
fn test_auto_invest_reverts_on_criteria_mismatch() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AutoInvest, ());
    let client = AutoInvestClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let investor = Address::generate(&env);
    let token = setup_token(&env, &investor, 1_000);

    client.initialize(&admin, &operator);

    let criteria = AutoInvestCriteria {
        min_yield_bps: 1200, // 12% minimum
        max_duration_days: 30,
        max_risk_tier: 1,
        token: token.clone(),
    };

    client.set_auto_invest_rule(&investor, &criteria, &500, &Some(1000));

    // Listing only has 800 bps (< 1200) -> Mismatch!
    let id = BytesN::from_array(&env, &[1u8; 32]);
    client.register_listing(&id, &800, &20, &1, &token, &1000);

    let res = client.try_execute_auto_invest(&operator, &investor, &id);
    assert!(res.is_err());
}

#[test]
fn test_disable_auto_invest_prevents_execution() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AutoInvest, ());
    let client = AutoInvestClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let investor = Address::generate(&env);
    let token = setup_token(&env, &investor, 1_000);

    client.initialize(&admin, &operator);

    let criteria = AutoInvestCriteria {
        min_yield_bps: 500,
        max_duration_days: 60,
        max_risk_tier: 3,
        token: token.clone(),
    };

    client.set_auto_invest_rule(&investor, &criteria, &500, &Some(1000));

    // Disable auto invest
    client.disable_auto_invest(&investor);

    let rule = client.get_auto_invest_rule(&investor);
    assert_eq!(rule.active, false);

    let id = BytesN::from_array(&env, &[1u8; 32]);
    client.register_listing(&id, &800, &30, &1, &token, &1000);

    // Should revert because rule is disabled
    let res = client.try_execute_auto_invest(&operator, &investor, &id);
    assert!(res.is_err());
}

#[test]
fn test_register_listing_retry_preserves_consumed_capacity_and_closed_state() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let contract_id = env.register(AutoInvest, ());
    let client = AutoInvestClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let investor = Address::generate(&env);
    let token = setup_token(&env, &investor, 10_000);

    client.initialize(&admin, &operator);
    let criteria = AutoInvestCriteria {
        min_yield_bps: 500,
        max_duration_days: 60,
        max_risk_tier: 2,
        token: token.clone(),
    };
    client.set_auto_invest_rule(&investor, &criteria, &500, &Some(2_000));

    let partly_funded = BytesN::from_array(&env, &[2u8; 32]);
    client.register_listing(&partly_funded, &1_000, &30, &1, &token, &1_000);
    assert_eq!(
        client.execute_auto_invest(&operator, &investor, &partly_funded),
        500
    );

    // A stale retry must not restore the original 1,000 capacity.
    client.register_listing(&partly_funded, &1_000, &30, &1, &token, &1_000);
    let listing = client.get_listing(&partly_funded);
    assert_eq!(listing.funding_needed, 500);
    assert!(listing.is_open);

    let fully_funded = BytesN::from_array(&env, &[3u8; 32]);
    client.register_listing(&fully_funded, &1_000, &30, &1, &token, &500);
    assert_eq!(
        client.execute_auto_invest(&operator, &investor, &fully_funded),
        500
    );

    // Replaying the original registration must not reopen a closed listing.
    client.register_listing(&fully_funded, &1_000, &30, &1, &token, &500);
    let listing = client.get_listing(&fully_funded);
    assert_eq!(listing.funding_needed, 0);
    assert!(!listing.is_open);
}

#[test]
fn test_register_listing_rejects_conflicting_terms_without_mutating_listing() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AutoInvest, ());
    let client = AutoInvestClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let token = Address::generate(&env);

    client.initialize(&admin, &operator);
    let id = BytesN::from_array(&env, &[4u8; 32]);
    client.register_listing(&id, &1_000, &30, &1, &token, &1_000);

    let result = client.try_register_listing(&id, &1_100, &30, &1, &token, &9_000);
    assert!(result.is_err());

    let listing = client.get_listing(&id);
    assert_eq!(listing.yield_bps, 1_000);
    assert_eq!(listing.funding_needed, 1_000);
    assert!(listing.is_open);
}
