//! Authorization Test Helpers and Negative Test Matrix for Payment Distributor
//!
//! This module provides reusable helpers for testing address authorization
//! across all public entrypoints in the payment-distributor contract.

#![cfg(test)]
#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{
    contract, contractimpl, Address, BytesN, Env, IntoVal, Symbol, TryFromVal, Val, Vec,
    token,
};

use crate::errors::Error;
use invoice_escrow::{InvoiceEscrow, InvoiceEscrowClient};
use invoice_token::{InvoiceToken, InvoiceTokenClient};

/// Authorization role classification for test cases
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthRole {
    /// Contract admin
    Admin,
    /// Escrow contract (authorized caller)
    Escrow,
    /// Operator (role-based)
    Operator,
    /// Third party (unauthorized)
    ThirdParty,
    /// Zero address (invalid)
    ZeroAddress,
}

/// Zero address constant (all-zero Ed25519 key)
pub fn zero_address(env: &Env) -> Address {
    let zero_str = soroban_sdk::String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    );
    Address::from_string(&zero_str)
}

/// Setup a standard test environment
pub struct DistributorAuthTestEnv {
    pub env: Env,
    pub admin: Address,
    pub escrow_id: Address,
    pub escrow_client: InvoiceEscrowClient<'static>,
    pub distributor_id: Address,
    pub distributor_client: PaymentDistributorClient<'static>,
    pub inv_token: InvoiceTokenClient<'static>,
    pub payment_token: Address,
    pub invoice_id: Symbol,
    pub third_party: Address,
    pub operator: Address,
}

impl DistributorAuthTestEnv {
    pub fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let third_party = Address::generate(&env);
        let operator = Address::generate(&env);

        let escrow_id = env.register_contract(None, InvoiceEscrow);
        let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

        let distributor_id = env.register_contract(None, PaymentDistributor);
        let distributor_client = PaymentDistributorClient::new(&env, &distributor_id);

        let inv_token_id = env.register_contract(None, InvoiceToken);
        let inv_token = InvoiceTokenClient::new(&env, &inv_token_id);

        let token_admin = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract_v2(token_admin);
        let payment_token = token::Client::new(&env, &token_id.address());
        let payment_asset = soroban_sdk::token::StellarAssetClient::new(&env, &token_id.address());

        let invoice_id = Symbol::new(&env, "INV_DIST_AUTH");

        inv_token.initialize(
            &admin,
            &soroban_sdk::String::from_str(&env, "Invoice Dist"),
            &soroban_sdk::String::from_str(&env, "INVD"),
            &7,
            &invoice_id,
            &escrow_id,
        );

        escrow_client.initialize(&admin, &0);
        distributor_client.initialize(&admin);
        distributor_client.set_escrow_contract(&admin, &escrow_id);
        escrow_client.set_payment_distributor(&distributor_id);

        // Grant operator role
        let operator_role = Symbol::new(&env, "operator");
        distributor_client.grant_role(&admin, &operator_role, &operator);

        Self {
            env,
            admin,
            escrow_id,
            escrow_client,
            distributor_id,
            distributor_client,
            inv_token,
            payment_token,
            invoice_id,
            third_party,
            operator,
        }
    }
}

/// Authorization matrix for initialize
#[test]
fn auth_matrix_distributor_initialize() {
    let env = Env::default();
    env.mock_all_auths();

    let distributor_id = env.register_contract(None, PaymentDistributor);
    let client = PaymentDistributorClient::new(&env, &distributor_id);

    let admin = Address::generate(&env);
    assert!(client.try_initialize(&admin).is_ok());

    // Re-initialize should fail
    let admin2 = Address::generate(&env);
    assert_eq!(
        client.try_initialize(&admin2),
        Err(Ok(Error::AlreadyInit))
    );
}

/// Authorization matrix for distribute_payment (escrow only)
#[test]
fn auth_matrix_distribute_payment() {
    let test_env = DistributorAuthTestEnv::new();
    let client = &test_env.distributor_client;
    let env = &test_env.env;

    // Setup: create and fund escrow
    let seller = Address::generate(env);
    let debtor = Address::generate(env);
    let buyer = Address::generate(env);

    let payment_asset = soroban_sdk::token::StellarAssetClient::new(env, &test_env.payment_token);
    payment_asset.mint(&buyer, &1000);
    payment_asset.mint(&debtor, &1000);

    test_env.escrow_client.create_escrow_legacy(
        &test_env.invoice_id,
        &seller,
        &debtor,
        &1000,
        &1000,
        &1_000_000,
        &test_env.payment_token,
        &test_env.inv_token.address(),
        &BytesN::from_array(env, &[0; 32]),
        &None,
        &None,
    );
    test_env.escrow_client.fund_escrow_legacy(&test_env.invoice_id, &buyer, &1000);

    let _ = env.events().all();

    test_env.escrow_client.record_payment(&test_env.invoice_id, &debtor, &1000);

    // The escrow calls distribute_payment internally, so we test via escrow
    // Direct call with wrong caller should fail
    let zero = zero_address(env);
    let result = client.try_distribute_payment(
        &zero,
        &test_env.invoice_id,
        &soroban_sdk::vec![env, test_env.payment_token, seller.clone(), buyer.clone(), test_env.admin.clone()],
        &soroban_sdk::vec![env, 1000, 1000, 0, 0],
        &1,
    );
    assert_eq!(result, Err(Ok(Error::UnauthorizedEscrow)), "Zero address escrow rejected");

    // Third party escrow should fail
    let fake_escrow = env.register_contract(None, InvoiceEscrow);
    let result = client.try_distribute_payment(
        &fake_escrow,
        &test_env.invoice_id,
        &soroban_sdk::vec![env, test_env.payment_token, seller.clone(), buyer.clone(), test_env.admin.clone()],
        &soroban_sdk::vec![env, 1000, 1000, 0, 0],
        &1,
    );
    assert_eq!(result, Err(Ok(Error::UnauthorizedEscrow)), "Unauthorized escrow rejected");
}

/// Authorization matrix for distribute_refund (escrow only)
#[test]
fn auth_matrix_distribute_refund() {
    let test_env = DistributorAuthTestEnv::new();
    let client = &test_env.distributor_client;
    let env = &test_env.env;

    let seller = Address::generate(env);
    let debtor = Address::generate(env);
    let buyer = Address::generate(env);

    let payment_asset = soroban_sdk::token::StellarAssetClient::new(env, &test_env.payment_token);
    payment_asset.mint(&buyer, &1000);

    test_env.escrow_client.create_escrow_legacy(
        &test_env.invoice_id,
        &seller,
        &debtor,
        &1000,
        &1000,
        &1_000_000,
        &test_env.payment_token,
        &test_env.inv_token.address(),
        &BytesN::from_array(env, &[1; 32]),
        &None,
        &None,
    );
    test_env.escrow_client.fund_escrow_legacy(&test_env.invoice_id, &buyer, &1000);

    let _ = env.events().all();

    // Fast-forward
    env.ledger().with_mut(|li| li.timestamp = 2_000_000);
    test_env.escrow_client.refund_escrow(&test_env.invoice_id);

    // Direct call with wrong caller should fail
    let zero = zero_address(env);
    let result = client.try_distribute_refund(
        &zero,
        &test_env.invoice_id,
        &soroban_sdk::vec![env, test_env.payment_token, buyer.clone()],
        &soroban_sdk::vec![env, 1000],
        &3,
    );
    assert_eq!(result, Err(Ok(Error::UnauthorizedEscrow)), "Zero address escrow rejected");
}

/// Authorization matrix for admin-only functions
#[test]
fn auth_matrix_distributor_admin_functions() {
    let test_env = DistributorAuthTestEnv::new();
    let client = &test_env.distributor_client;
    let env = &test_env.env;
    let zero = zero_address(env);

    // set_investor_bonus_bps - admin only
    let result = client.try_set_investor_bonus_bps(&test_env.admin, &test_env.admin, &100);
    assert!(result.is_ok(), "Admin should succeed");

    let result = client.try_set_investor_bonus_bps(&test_env.admin, &test_env.third_party, &100);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Third party cannot set bonus");

    let result = client.try_set_investor_bonus_bps(&test_env.admin, &zero, &100);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Zero address admin rejected");

    // set_fee_tiers - admin only
    let tier = crate::types::FeeTier { min_amount: 0, max_amount: 0, fee_bps: 100 };
    let result = client.try_set_fee_tiers(&test_env.admin, &test_env.admin, &soroban_sdk::vec![env, tier]);
    assert!(result.is_ok(), "Admin should succeed");

    let result = client.try_set_fee_tiers(&test_env.admin, &test_env.third_party, &soroban_sdk::vec![env, tier]);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Third party cannot set fee tiers");

    // set_fee_promotion - admin only
    let promo = crate::types::FeePromotion { fee_bps: 50, start_ledger: 100, end_ledger: 200 };
    let result = client.try_set_fee_promotion(&test_env.admin, &test_env.admin, &Some(promo));
    assert!(result.is_ok(), "Admin should succeed");

    let result = client.try_set_fee_promotion(&test_env.admin, &test_env.third_party, &Some(promo));
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Third party cannot set promo");

    // set_fee_recipient - admin only
    let new_recipient = Address::generate(env);
    let result = client.try_set_fee_recipient(&test_env.admin, &test_env.admin, &new_recipient);
    assert!(result.is_ok(), "Admin should succeed");

    let result = client.try_set_fee_recipient(&test_env.admin, &test_env.third_party, &new_recipient);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Third party cannot set fee recipient");

    let result = client.try_set_fee_recipient(&test_env.admin, &zero, &new_recipient);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Zero address admin rejected");

    // set_escrow_contract - admin only
    let new_escrow = env.register_contract(None, InvoiceEscrow);
    let result = client.try_set_escrow_contract(&test_env.admin, &test_env.admin, &new_escrow);
    assert!(result.is_ok(), "Admin should succeed");

    let result = client.try_set_escrow_contract(&test_env.admin, &test_env.third_party, &new_escrow);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Third party cannot set escrow contract");

    // emergency_withdraw - admin only
    let token_admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let payment_token = token::Client::new(env, &token_id.address());
    let payment_asset = soroban_sdk::token::StellarAssetClient::new(env, &token_id.address());
    payment_asset.mint(&test_env.distributor_id, &1000);

    let to = Address::generate(env);
    let result = client.try_emergency_withdraw(&test_env.admin, &test_env.admin, &payment_token.address(), &to);
    assert!(result.is_ok(), "Admin should succeed");

    let result = client.try_emergency_withdraw(&test_env.admin, &test_env.third_party, &payment_token.address(), &to);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Third party cannot emergency withdraw");

    // sweep_dust - admin only
    payment_asset.mint(&test_env.distributor_id, &1000);
    let result = client.try_sweep_dust(&test_env.admin, &test_env.admin, &payment_token.address());
    assert!(result.is_ok(), "Admin should succeed");

    let result = client.try_sweep_dust(&test_env.admin, &test_env.third_party, &payment_token.address());
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Third party cannot sweep dust");

    // validate_recipients - admin/operator only
    let recipients = soroban_sdk::vec![env, test_env.admin.clone()];
    let result = client.try_validate_recipients(&test_env.admin, &test_env.admin, &test_env.payment_token, &recipients);
    assert!(result.is_ok(), "Admin should succeed");

    // Operator should succeed
    let result = client.try_validate_recipients(&test_env.admin, &test_env.operator, &test_env.payment_token, &recipients);
    assert!(result.is_ok(), "Operator should succeed");

    let result = client.try_validate_recipients(&test_env.admin, &test_env.third_party, &test_env.payment_token, &recipients);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Third party cannot validate");

    // distribute_multi_asset - admin/operator only
    let split = crate::types::DistributionSplit {
        recipients: soroban_sdk::vec![env, test_env.admin.clone()],
        shares_bps: soroban_sdk::vec![env, 10000],
        referral_bps: 0,
        referral: None,
    };
    let route = crate::types::AssetRoute {
        token: payment_token.address(),
        amount: 1000,
        split,
    };

    payment_asset.mint(&test_env.distributor_id, &1000);
    let result = client.try_distribute_multi_asset(&test_env.admin, &test_env.admin, &soroban_sdk::vec![env, route]);
    assert!(result.is_ok(), "Admin should succeed");

    payment_asset.mint(&test_env.distributor_id, &1000);
    let result = client.try_distribute_multi_asset(&test_env.admin, &test_env.operator, &soroban_sdk::vec![env, route]);
    assert!(result.is_ok(), "Operator should succeed");

    let result = client.try_distribute_multi_asset(&test_env.admin, &test_env.third_party, &soroban_sdk::vec![env, route]);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Third party cannot distribute multi-asset");
}

/// Authorization matrix for refund_excess (escrow only)
#[test]
fn auth_matrix_refund_excess() {
    let test_env = DistributorAuthTestEnv::new();
    let client = &test_env.distributor_client;
    let env = &test_env.env;

    let seller = Address::generate(env);
    let debtor = Address::generate(env);
    let buyer = Address::generate(env);

    let payment_asset = soroban_sdk::token::StellarAssetClient::new(env, &test_env.payment_token);
    payment_asset.mint(&buyer, &1000);
    payment_asset.mint(&debtor, &1000);

    test_env.escrow_client.create_escrow_legacy(
        &test_env.invoice_id,
        &seller,
        &debtor,
        &1000,
        &1000,
        &1_000_000,
        &test_env.payment_token,
        &test_env.inv_token.address(),
        &BytesN::from_array(env, &[2; 32]),
        &None,
        &None,
    );
    test_env.escrow_client.fund_escrow_legacy(&test_env.invoice_id, &buyer, &1000);

    let _ = env.events().all();
    test_env.escrow_client.record_payment(&test_env.invoice_id, &debtor, &1000);

    // Overfund distributor
    payment_asset.mint(&test_env.distributor_id, &500);

    // Escrow should succeed
    let result = client.try_refund_excess(&test_env.escrow_id, &test_env.payment_token, &test_env.invoice_id);
    assert!(result.is_ok(), "Escrow should succeed");

    // Wrong escrow should fail
    let fake_escrow = env.register_contract(None, InvoiceEscrow);
    let result = client.try_refund_excess(&fake_escrow, &test_env.payment_token, &test_env.invoice_id);
    assert_eq!(result, Err(Ok(Error::UnauthorizedEscrow)), "Unauthorized escrow rejected");

    // Zero address escrow should fail
    let zero = zero_address(env);
    let result = client.try_refund_excess(&zero, &test_env.payment_token, &test_env.invoice_id);
    assert_eq!(result, Err(Ok(Error::UnauthorizedEscrow)), "Zero address escrow rejected");
}

/// Test state unchanged after rejected calls
#[test]
fn auth_distributor_rejected_calls_leave_state_unchanged() {
    let test_env = DistributorAuthTestEnv::new();
    let client = &test_env.distributor_client;
    let env = &test_env.env;

    // Capture admin before failed call
    let admin_before = client.get_admin().unwrap();

    // Try to set fee recipient with third party
    let new_recipient = Address::generate(env);
    let result = client.try_set_fee_recipient(&test_env.admin, &test_env.third_party, &new_recipient);
    assert!(result.is_err());

    // Admin should be unchanged
    let admin_after = client.get_admin().unwrap();
    assert_eq!(admin_before, admin_after, "Admin changed after failed set_fee_recipient");

    // Try to set escrow contract with third party
    let new_escrow = env.register_contract(None, InvoiceEscrow);
    let escrow_before = client.get_escrow_contract().unwrap();
    let result = client.try_set_escrow_contract(&test_env.admin, &test_env.third_party, &new_escrow);
    assert!(result.is_err());

    let escrow_after = client.get_escrow_contract().unwrap();
    assert_eq!(escrow_before, escrow_after, "Escrow contract changed after failed set_escrow_contract");
}

/// Comprehensive authorization matrix documentation
#[test]
fn auth_distributor_comprehensive_matrix() {
    // Role legend: A=Admin, E=Escrow, O=Operator, T=ThirdParty, Z=ZeroAddress
    let matrix = vec![
        ("initialize", vec![("A", true), ("E", false), ("O", false), ("T", false), ("Z", false)]),
        ("distribute_payment", vec![("A", false), ("E", true), ("O", false), ("T", false), ("Z", false)]),
        ("distribute_refund", vec![("A", false), ("E", true), ("O", false), ("T", false), ("Z", false)]),
        ("set_investor_bonus_bps", vec![("A", true), ("E", false), ("O", false), ("T", false), ("Z", false)]),
        ("set_fee_tiers", vec![("A", true), ("E", false), ("O", false), ("T", false), ("Z", false)]),
        ("set_fee_promotion", vec![("A", true), ("E", false), ("O", false), ("T", false), ("Z", false)]),
        ("set_fee_recipient", vec![("A", true), ("E", false), ("O", false), ("T", false), ("Z", false)]),
        ("set_escrow_contract", vec![("A", true), ("E", false), ("O", false), ("T", false), ("Z", false)]),
        ("emergency_withdraw", vec![("A", true), ("E", false), ("O", false), ("T", false), ("Z", false)]),
        ("sweep_dust", vec![("A", true), ("E", false), ("O", false), ("T", false), ("Z", false)]),
        ("validate_recipients", vec![("A", true), ("E", false), ("O", true), ("T", false), ("Z", false)]),
        ("distribute_multi_asset", vec![("A", true), ("E", false), ("O", true), ("T", false), ("Z", false)]),
        ("refund_excess", vec![("A", false), ("E", true), ("O", false), ("T", false), ("Z", false)]),
        ("get_admin", vec![("A", true), ("E", true), ("O", true), ("T", true), ("Z", true)]), // view
        ("get_fee_recipient", vec![("A", true), ("E", true), ("O", true), ("T", true), ("Z", true)]), // view
        ("get_escrow_contract", vec![("A", true), ("E", true), ("O", true), ("T", true), ("Z", true)]), // view
        ("get_distribution_state", vec![("A", true), ("E", true), ("O", true), ("T", true), ("Z", true)]), // view
        ("get_investor_bonus_bps", vec![("A", true), ("E", true), ("O", true), ("T", true), ("Z", true)]), // view
        ("get_fee_promotion", vec![("A", true), ("E", true), ("O", true), ("T", true), ("Z", true)]), // view
        ("get_fee_tiers", vec![("A", true), ("E", true), ("O", true), ("T", true), ("Z", true)]), // view
    ];

    for (method, roles) in matrix {
        println!("Method: {}", method);
        for (role, expected) in roles {
            println!("  {}: {}", role, if expected { "✓" } else { "✗" });
        }
    }

    assert_eq!(matrix.len(), 19, "Authorization matrix should cover all 19 entrypoints");
}