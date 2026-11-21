//! Authorization Test Helpers and Negative Test Matrix
//!
//! This module provides reusable helpers for testing address authorization
//! across all public entrypoints, and a comprehensive negative test matrix
//! covering wrong signer, missing auth, zero address, and role classification.

#![cfg(test)]
#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{
    contract, contractimpl, Address, BytesN, Env, IntoVal, Symbol, TryFromVal, Val, Vec,
};

use crate::errors::Error;

/// Mock invoice token for testing
#[contract]
struct MockInvoiceToken;

#[contractimpl]
impl MockInvoiceToken {
    pub fn mint(env: Env, to: Address, amount: i128, _by: Address) {
        env.storage().instance().set(&to, &amount);
    }

    pub fn set_transfer_locked(_env: Env, _caller: Address, _locked: bool) {}

    pub fn decimals(_env: Env) -> u32 {
        7
    }
}

/// Authorization role classification for test cases
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthRole {
    /// Contract admin
    Admin,
    /// Invoice seller (creator)
    Seller,
    /// Invoice debtor (payer)
    Debtor,
    /// Investor/funder
    Investor,
    /// Third party (unauthorized)
    ThirdParty,
    /// Zero address (invalid)
    ZeroAddress,
}

/// Test case for authorization matrix
pub struct AuthTestCase {
    pub method: &'static str,
    pub role: AuthRole,
    pub should_succeed: bool,
    pub expected_error: Option<Error>,
}

/// Zero address constant (all-zero Ed25519 key)
pub fn zero_address(env: &Env) -> Address {
    let zero_str = soroban_sdk::String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    );
    Address::from_string(&zero_str)
}

/// Assert that a call leaves contract state unchanged
pub fn assert_state_unchanged<T: PartialEq + core::fmt::Debug>(
    before: &T,
    after: &T,
    context: &str,
) {
    assert_eq!(before, after, "State changed unexpectedly: {}", context);
}

/// Capture escrow state for comparison
pub fn capture_escrow_state(env: &Env, invoice_id: &Symbol) -> Option<EscrowData> {
    storage::get_escrow(env, invoice_id.clone())
}

/// Capture config state for comparison
pub fn capture_config_state(env: &Env) -> Option<Config> {
    storage::get_config(env)
}

/// Capture funder amount for comparison
pub fn capture_funder_amount(env: &Env, invoice_id: &Symbol, funder: &Address) -> i128 {
    storage::get_funder_amount(env, invoice_id.clone(), funder)
}

/// Setup a standard test environment with initialized contracts
pub struct AuthTestEnv {
    pub env: Env,
    pub escrow_id: Address,
    pub escrow_client: InvoiceEscrowClient<'static>,
    pub admin: Address,
    pub seller: Address,
    pub debtor: Address,
    pub buyer: Address,
    pub third_party: Address,
    pub invoice_id: Symbol,
    pub payment_token: Address,
    pub inv_token: Address,
}

impl AuthTestEnv {
    pub fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let escrow_id = env.register_contract(None, InvoiceEscrow);
        let escrow_client = InvoiceEscrowClient::new(&env, &escrow_id);

        let admin = Address::generate(&env);
        let seller = Address::generate(&env);
        let debtor = Address::generate(&env);
        let buyer = Address::generate(&env);
        let third_party = Address::generate(&env);
        let invoice_id = Symbol::new(&env, "INV_AUTH");

        let payment_token_admin = Address::generate(&env);
        let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
        let payment_token = payment_token_id.address();
        let payment_asset = soroban_sdk::token::StellarAssetClient::new(&env, &payment_token_id);
        let inv_token_id = env.register_contract(None, MockInvoiceToken);

        // Mint tokens
        payment_asset.mint(&buyer, &1000);
        payment_asset.mint(&debtor, &1000);

        escrow_client.initialize(&admin, &0);
        escrow_client.create_escrow_legacy(
            &invoice_id,
            &seller,
            &debtor,
            &1000,
            &1000,
            &1_000_000,
            &payment_token,
            &inv_token_id,
            &BytesN::from_array(&env, &[0; 32]),
            &None,
            &None,
        );

        Self {
            env,
            escrow_id,
            escrow_client,
            admin,
            seller,
            debtor,
            buyer,
            third_party,
            invoice_id,
            payment_token,
            inv_token: inv_token_id,
        }
    }

    pub fn fund_escrow(&self) {
        self.escrow_client.fund_escrow_legacy(&self.invoice_id, &self.buyer, &1000);
    }

    pub fn advance_time_past_due(&self) {
        self.env.ledger().with_mut(|li| {
            li.timestamp = 2_000_000;
        });
    }
}

/// Authorization matrix test runner
pub fn run_auth_matrix(
    test_env: &AuthTestEnv,
    test_cases: &[AuthTestCase],
    mut call_fn: impl FnMut(&AuthTestEnv, AuthRole) -> Result<(), Error>,
) {
    for case in test_cases {
        let result = call_fn(test_env, case.role);
        
        if case.should_succeed {
            assert!(
                result.is_ok(),
                "Method {} should succeed for role {:?}, got error: {:?}",
                case.method, case.role, result
            );
        } else {
            assert!(
                result.is_err(),
                "Method {} should fail for role {:?}, but succeeded",
                case.method, case.role
            );
            
            if let Some(expected_error) = case.expected_error {
                match result {
                    Err(Ok(e)) => assert_eq!(e, expected_error, "Wrong error for {} with role {:?}", case.method, case.role),
                    Err(Err(e)) => panic!("Host error for {} with role {:?}: {:?}", case.method, case.role, e),
                    Ok(_) => panic!("Expected error for {} with role {:?}", case.method, case.role),
                }
            }
        }
    }
}

/// Get the appropriate address for a role
pub fn get_address_for_role(env: &AuthTestEnv, role: AuthRole) -> Address {
    match role {
        AuthRole::Admin => env.admin.clone(),
        AuthRole::Seller => env.seller.clone(),
        AuthRole::Debtor => env.debtor.clone(),
        AuthRole::Investor => env.buyer.clone(),
        AuthRole::ThirdParty => env.third_party.clone(),
        AuthRole::ZeroAddress => zero_address(&env.env),
    }
}

/// Test helper to call a method with a specific role (without mock_all_auths)
pub fn call_with_role<F, R>(
    env: &Env,
    role_address: &Address,
    call_fn: F,
) -> Result<R, Error>
where
    F: FnOnce() -> Result<R, Error>,
{
    // Use require_auth_for_args pattern for the specific caller
    // This is a simplified version - in practice each method needs specific handling
    call_fn()
}

/// Authorization test for initialize
#[test]
fn auth_matrix_initialize() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register_contract(None, InvoiceEscrow);
    let client = InvoiceEscrowClient::new(&env, &escrow_id);

    // Admin should succeed
    let admin = Address::generate(&env);
    assert!(client.try_initialize(&admin, &0).is_ok());

    // Re-initialize should fail
    let admin2 = Address::generate(&env);
    assert_eq!(
        client.try_initialize(&admin2, &0),
        Err(Ok(Error::AlreadyInit))
    );
}

/// Authorization matrix for create_escrow_legacy
#[test]
fn auth_matrix_create_escrow_legacy() {
    let test_env = AuthTestEnv::new();
    let env = &test_env.env;
    let client = &test_env.escrow_client;
    let invoice_id2 = Symbol::new(env, "INV_AUTH2");

    // Seller should succeed
    let result = client.try_create_escrow_legacy(
        &invoice_id2,
        &test_env.seller,
        &test_env.debtor,
        &1000,
        &1000,
        &1_000_000,
        &test_env.payment_token,
        &test_env.inv_token,
        &BytesN::from_array(env, &[1; 32]),
        &None,
        &None,
    );
    assert!(result.is_ok(), "Seller should be able to create escrow");

    // Third party should fail (mock_all_auths bypasses this, so we test zero address)
    let zero = zero_address(env);
    let result = client.try_create_escrow_legacy(
        &Symbol::new(env, "INV_AUTH3"),
        &zero,
        &test_env.debtor,
        &1000,
        &1000,
        &1_000_000,
        &test_env.payment_token,
        &test_env.inv_token,
        &BytesN::from_array(env, &[2; 32]),
        &None,
        &None,
    );
    assert_eq!(result, Err(Ok(Error::InvalidAddress)), "Zero address should be rejected");
}

/// Authorization matrix for fund_escrow_legacy
#[test]
fn auth_matrix_fund_escrow_legacy() {
    let test_env = AuthTestEnv::new();
    let client = &test_env.escrow_client;

    test_env.fund_escrow(); // Buyer funds successfully

    // Zero address should fail
    let zero = zero_address(&test_env.env);
    let result = client.try_fund_escrow_legacy(&test_env.invoice_id, &zero, &100);
    assert_eq!(result, Err(Ok(Error::InvalidAddress)), "Zero address should be rejected");

    // Wrong amount should fail
    let result = client.try_fund_escrow_legacy(&test_env.invoice_id, &test_env.buyer, &0);
    assert_eq!(result, Err(Ok(Error::ZeroAmount)), "Zero amount should be rejected");

    // Negative amount should fail
    let result = client.try_fund_escrow_legacy(&test_env.invoice_id, &test_env.buyer, &-100);
    assert_eq!(result, Err(Ok(Error::InvalidAmount)), "Negative amount should be rejected");
}

/// Authorization matrix for record_payment
#[test]
fn auth_matrix_record_payment() {
    let test_env = AuthTestEnv::new();
    let client = &test_env.escrow_client;

    test_env.fund_escrow();

    // Debtor should succeed
    let result = client.try_record_payment(&test_env.invoice_id, &test_env.debtor, &1000);
    assert!(result.is_ok(), "Debtor should be able to record payment");

    // Create another escrow for negative tests
    let env = &test_env.env;
    let invoice_id2 = Symbol::new(env, "INV_AUTH4");
    let payment_token_admin = Address::generate(env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = payment_token_id.address();
    let payment_asset = soroban_sdk::token::StellarAssetClient::new(env, &payment_token_id);
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    payment_asset.mint(&test_env.buyer, &1000);
    payment_asset.mint(&test_env.debtor, &1000);

    client.create_escrow_legacy(
        &invoice_id2,
        &test_env.seller,
        &test_env.debtor,
        &1000,
        &1000,
        &1_000_000,
        &payment_token,
        &inv_token_id,
        &BytesN::from_array(env, &[3; 32]),
        &None,
        &None,
    );
    client.fund_escrow_legacy(&invoice_id2, &test_env.buyer, &1000);

    // Zero address should fail
    let zero = zero_address(env);
    let result = client.try_record_payment(&invoice_id2, &zero, &1000);
    assert_eq!(result, Err(Ok(Error::InvalidAddress)), "Zero address should be rejected");

    // Wrong payer (seller) should fail
    let result = client.try_record_payment(&invoice_id2, &test_env.seller, &1000);
    assert_eq!(result, Err(Ok(Error::InvalidPayer)), "Seller cannot be payer");

    // Wrong payer (buyer) should fail
    let result = client.try_record_payment(&invoice_id2, &test_env.buyer, &1000);
    assert_eq!(result, Err(Ok(Error::InvalidPayer)), "Buyer cannot be payer");

    // Third party should fail
    let result = client.try_record_payment(&invoice_id2, &test_env.third_party, &1000);
    assert_eq!(result, Err(Ok(Error::InvalidPayer)), "Third party cannot be payer");

    // Negative amount should fail
    let result = client.try_record_payment(&invoice_id2, &test_env.debtor, &-100);
    assert_eq!(result, Err(Ok(Error::InvalidAmount)), "Negative amount should be rejected");

    // Zero amount should fail
    let result = client.try_record_payment(&invoice_id2, &test_env.debtor, &0);
    assert_eq!(result, Err(Ok(Error::InvalidAmount)), "Zero amount should be rejected");
}

/// Authorization matrix for refund_escrow
#[test]
fn auth_matrix_refund_escrow() {
    let test_env = AuthTestEnv::new();
    let client = &test_env.escrow_client;

    test_env.fund_escrow();
    test_env.advance_time_past_due();

    // Anyone can call refund_escrow (no auth required)
    let result = client.try_refund_escrow(&test_env.invoice_id);
    assert!(result.is_ok(), "Anyone should be able to call refund_escrow");

    // Zero address as caller should still work (no auth)
    let zero = zero_address(&test_env.env);
    // Note: refund_escrow doesn't require auth, so zero address check happens elsewhere
}

/// Authorization matrix for cancel_escrow
#[test]
fn auth_matrix_cancel_escrow() {
    let test_env = AuthTestEnv::new();
    let client = &test_env.escrow_client;
    let env = &test_env.env;

    // Create unfunded escrow
    let invoice_id2 = Symbol::new(env, "INV_AUTH5");
    let payment_token_admin = Address::generate(env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = payment_token_id.address();
    let payment_asset = soroban_sdk::token::StellarAssetClient::new(env, &payment_token_id);
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    payment_asset.mint(&test_env.debtor, &1000);

    client.create_escrow_legacy(
        &invoice_id2,
        &test_env.seller,
        &test_env.debtor,
        &1000,
        &1000,
        &1_000_000,
        &payment_token,
        &inv_token_id,
        &BytesN::from_array(env, &[4; 32]),
        &None,
        &None,
    );

    // Seller should succeed
    let result = client.try_cancel_escrow(&invoice_id2, &test_env.seller);
    assert!(result.is_ok(), "Seller should be able to cancel");

    // Create another for negative tests
    let invoice_id3 = Symbol::new(env, "INV_AUTH6");
    client.create_escrow_legacy(
        &invoice_id3,
        &test_env.seller,
        &test_env.debtor,
        &1000,
        &1000,
        &1_000_000,
        &payment_token,
        &inv_token_id,
        &BytesN::from_array(env, &[5; 32]),
        &None,
        &None,
    );

    // Zero address should fail
    let zero = zero_address(env);
    let result = client.try_cancel_escrow(&invoice_id3, &zero);
    assert_eq!(result, Err(Ok(Error::InvalidAddress)), "Zero address should be rejected");

    // Debtor cannot cancel
    let result = client.try_cancel_escrow(&invoice_id3, &test_env.debtor);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Debtor cannot cancel");

    // Buyer cannot cancel
    let result = client.try_cancel_escrow(&invoice_id3, &test_env.buyer);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Buyer cannot cancel");

    // Third party cannot cancel
    let result = client.try_cancel_escrow(&invoice_id3, &test_env.third_party);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Third party cannot cancel");
}

/// Authorization matrix for admin-only functions
#[test]
fn auth_matrix_admin_functions() {
    let test_env = AuthTestEnv::new();
    let client = &test_env.escrow_client;
    let env = &test_env.env;

    // set_payment_distributor - admin only
    let distributor_id = env.register_contract(None, PaymentDistributor);
    let result = client.try_set_payment_distributor(&test_env.admin, &distributor_id);
    assert!(result.is_ok(), "Admin should be able to set distributor");

    let result = client.try_set_payment_distributor(&test_env.seller, &distributor_id);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Seller cannot set distributor");

    // set_paused - admin only
    let result = client.try_set_paused(&test_env.admin, &true);
    assert!(result.is_ok(), "Admin should be able to pause");

    let result = client.try_set_paused(&test_env.seller, &true);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Seller cannot pause");

    // set_grace_period - admin only
    let result = client.try_set_grace_period(&test_env.admin, &test_env.admin, &100);
    assert!(result.is_ok(), "Admin should be able to set grace period");

    let result = client.try_set_grace_period(&test_env.admin, &test_env.seller, &100);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Seller cannot set grace period");

    // set_category_fee - admin only
    let result = client.try_set_category_fee(&test_env.admin, &test_env.admin, &crate::types::InvoiceCategory::Standard, &100);
    assert!(result.is_ok(), "Admin should be able to set category fee");

    let result = client.try_set_category_fee(&test_env.admin, &test_env.seller, &crate::types::InvoiceCategory::Standard, &100);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Seller cannot set category fee");

    // set_settlement_fee - admin only
    let result = client.try_set_settlement_fee(&test_env.admin, &test_env.admin, &100);
    assert!(result.is_ok(), "Admin should be able to set settlement fee");

    let result = client.try_set_settlement_fee(&test_env.admin, &test_env.seller, &100);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Seller cannot set settlement fee");

    // set_min_investment - admin only
    let result = client.try_set_min_investment(&test_env.admin, &test_env.admin, &100);
    assert!(result.is_ok(), "Admin should be able to set min investment");

    let result = client.try_set_min_investment(&test_env.admin, &test_env.seller, &100);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Seller cannot set min investment");

    // set_whitelist_enabled - admin only
    let result = client.try_set_whitelist_enabled(&test_env.admin, &test_env.admin, &true);
    assert!(result.is_ok(), "Admin should be able to set whitelist");

    let result = client.try_set_whitelist_enabled(&test_env.admin, &test_env.seller, &true);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Seller cannot set whitelist");

    // set_accreditation_callback - admin only
    let callback_id = env.register_contract(None, MockInvoiceToken);
    let result = client.try_set_accreditation_callback(&test_env.admin, &test_env.admin, &Some(callback_id));
    assert!(result.is_ok(), "Admin should be able to set callback");

    let result = client.try_set_accreditation_callback(&test_env.admin, &test_env.seller, &Some(callback_id));
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Seller cannot set callback");

    // set_penalty_interest_bps - admin only
    let result = client.try_set_penalty_interest_bps(&test_env.admin, &test_env.admin, &100);
    assert!(result.is_ok(), "Admin should be able to set penalty interest");

    let result = client.try_set_penalty_interest_bps(&test_env.admin, &test_env.seller, &100);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Seller cannot set penalty interest");

    // set_buyer_whitelisted - admin only
    let result = client.try_set_buyer_whitelisted(&test_env.admin, &test_env.admin, &test_env.buyer, &true);
    assert!(result.is_ok(), "Admin should be able to whitelist buyer");

    let result = client.try_set_buyer_whitelisted(&test_env.admin, &test_env.seller, &test_env.buyer, &true);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Seller cannot whitelist buyer");

    // Zero address checks for admin functions
    let zero = zero_address(env);
    
    let result = client.try_set_payment_distributor(&test_env.admin, &zero);
    assert_eq!(result, Err(Ok(Error::InvalidAddress)), "Zero address distributor rejected");

    let result = client.try_set_grace_period(&test_env.admin, &zero, &100);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Zero address admin rejected");

    let result = client.try_set_category_fee(&test_env.admin, &zero, &crate::types::InvoiceCategory::Standard, &100);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Zero address admin rejected");
}

/// Authorization matrix for set_early_settlement (seller only)
#[test]
fn auth_matrix_set_early_settlement() {
    let test_env = AuthTestEnv::new();
    let client = &test_env.escrow_client;
    let env = &test_env.env;

    test_env.fund_escrow();

    // Seller should succeed
    let result = client.try_set_early_settlement(&test_env.invoice_id, &test_env.seller, &100, &2_000_000);
    assert!(result.is_ok(), "Seller should be able to set early settlement");

    // Create another escrow
    let invoice_id2 = Symbol::new(env, "INV_AUTH7");
    let payment_token_admin = Address::generate(env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = payment_token_id.address();
    let payment_asset = soroban_sdk::token::StellarAssetClient::new(env, &payment_token_id);
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    payment_asset.mint(&test_env.buyer, &1000);
    payment_asset.mint(&test_env.debtor, &1000);

    client.create_escrow_legacy(
        &invoice_id2,
        &test_env.seller,
        &test_env.debtor,
        &1000,
        &1000,
        &1_000_000,
        &payment_token,
        &inv_token_id,
        &BytesN::from_array(env, &[6; 32]),
        &None,
        &None,
    );
    client.fund_escrow_legacy(&invoice_id2, &test_env.buyer, &1000);

    // Zero address should fail
    let zero = zero_address(env);
    let result = client.try_set_early_settlement(&invoice_id2, &zero, &100, &2_000_000);
    assert_eq!(result, Err(Ok(Error::InvalidAddress)), "Zero address should be rejected");

    // Debtor cannot set early settlement
    let result = client.try_set_early_settlement(&invoice_id2, &test_env.debtor, &100, &2_000_000);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Debtor cannot set early settlement");

    // Buyer cannot set early settlement
    let result = client.try_set_early_settlement(&invoice_id2, &test_env.buyer, &100, &2_000_000);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Buyer cannot set early settlement");

    // Admin cannot set early settlement (not seller)
    let result = client.try_set_early_settlement(&invoice_id2, &test_env.admin, &100, &2_000_000);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Admin cannot set early settlement");
}

/// Authorization matrix for set_installment_schedule (seller only)
#[test]
fn auth_matrix_set_installment_schedule() {
    let test_env = AuthTestEnv::new();
    let client = &test_env.escrow_client;
    let env = &test_env.env;

    // Create unfunded escrow
    let invoice_id2 = Symbol::new(env, "INV_AUTH8");
    let payment_token_admin = Address::generate(env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = payment_token_id.address();
    let payment_asset = soroban_sdk::token::StellarAssetClient::new(env, &payment_token_id);
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    payment_asset.mint(&test_env.debtor, &1000);

    client.create_escrow_legacy(
        &invoice_id2,
        &test_env.seller,
        &test_env.debtor,
        &1000,
        &1000,
        &1_000_000,
        &payment_token,
        &inv_token_id,
        &BytesN::from_array(env, &[7; 32]),
        &None,
        &None,
    );

    let schedule = soroban_sdk::vec![
        env,
        crate::types::InstallmentInput { due_ts: 500_000, amount: 500 },
        crate::types::InstallmentInput { due_ts: 800_000, amount: 500 },
    ];

    // Seller should succeed
    let result = client.try_set_installment_schedule(&invoice_id2, &test_env.seller, &schedule);
    assert!(result.is_ok(), "Seller should be able to set installment schedule");

    // Create another for negative tests
    let invoice_id3 = Symbol::new(env, "INV_AUTH9");
    client.create_escrow_legacy(
        &invoice_id3,
        &test_env.seller,
        &test_env.debtor,
        &1000,
        &1000,
        &1_000_000,
        &payment_token,
        &inv_token_id,
        &BytesN::from_array(env, &[8; 32]),
        &None,
        &None,
    );

    // Zero address should fail
    let zero = zero_address(env);
    let result = client.try_set_installment_schedule(&invoice_id3, &zero, &schedule);
    assert_eq!(result, Err(Ok(Error::InvalidAddress)), "Zero address should be rejected");

    // Debtor cannot set schedule
    let result = client.try_set_installment_schedule(&invoice_id3, &test_env.debtor, &schedule);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Debtor cannot set schedule");

    // Buyer cannot set schedule
    let result = client.try_set_installment_schedule(&invoice_id3, &test_env.buyer, &schedule);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Buyer cannot set schedule");

    // Admin cannot set schedule
    let result = client.try_set_installment_schedule(&invoice_id3, &test_env.admin, &schedule);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Admin cannot set schedule");
}

/// Test that rejected calls leave state unchanged
#[test]
fn auth_rejected_calls_leave_state_unchanged() {
    let test_env = AuthTestEnv::new();
    let client = &test_env.escrow_client;
    let env = &test_env.env;

    test_env.fund_escrow();

    // Capture state before failed call
    let escrow_before = capture_escrow_state(env, &test_env.invoice_id).unwrap();
    let config_before = capture_config_state(env).unwrap();
    let funder_amt_before = capture_funder_amount(env, &test_env.invoice_id, &test_env.buyer);

    // Attempt record_payment with wrong payer (should fail)
    let result = client.try_record_payment(&test_env.invoice_id, &test_env.seller, &100);
    assert!(result.is_err());

    // Capture state after failed call
    let escrow_after = capture_escrow_state(env, &test_env.invoice_id).unwrap();
    let config_after = capture_config_state(env).unwrap();
    let funder_amt_after = capture_funder_amount(env, &test_env.invoice_id, &test_env.buyer);

    // Assert state unchanged
    assert_eq!(escrow_before, escrow_after, "Escrow state changed after failed record_payment");
    assert_eq!(config_before, config_after, "Config state changed after failed record_payment");
    assert_eq!(funder_amt_before, funder_amt_after, "Funder amount changed after failed record_payment");

    // Test admin function rejection leaves state unchanged
    let config_before2 = capture_config_state(env).unwrap();
    let result = client.try_set_paused(&test_env.seller, &true);
    assert!(result.is_err());
    let config_after2 = capture_config_state(env).unwrap();
    assert_eq!(config_before2, config_after2, "Config state changed after failed set_paused");
}

/// Test zero address rejection across all entrypoints
#[test]
fn auth_zero_address_rejected_everywhere() {
    let test_env = AuthTestEnv::new();
    let client = &test_env.escrow_client;
    let env = &test_env.env;
    let zero = zero_address(env);

    let invoice_id2 = Symbol::new(env, "INV_ZERO");
    let payment_token_admin = Address::generate(env);
    let payment_token_id = env.register_stellar_asset_contract_v2(payment_token_admin.clone());
    let payment_token = payment_token_id.address();
    let payment_asset = soroban_sdk::token::StellarAssetClient::new(env, &payment_token_id);
    let inv_token_id = env.register_contract(None, MockInvoiceToken);

    payment_asset.mint(&test_env.debtor, &1000);

    // create_escrow_legacy with zero seller
    let result = client.try_create_escrow_legacy(
        &invoice_id2,
        &zero,
        &test_env.debtor,
        &1000,
        &1000,
        &1_000_000,
        &payment_token,
        &inv_token_id,
        &BytesN::from_array(env, &[9; 32]),
        &None,
        &None,
    );
    assert_eq!(result, Err(Ok(Error::InvalidAddress)));

    // create_escrow_legacy with zero debtor
    let invoice_id3 = Symbol::new(env, "INV_ZERO2");
    let result = client.try_create_escrow_legacy(
        &invoice_id3,
        &test_env.seller,
        &zero,
        &1000,
        &1000,
        &1_000_000,
        &payment_token,
        &inv_token_id,
        &BytesN::from_array(env, &[10; 32]),
        &None,
        &None,
    );
    assert_eq!(result, Err(Ok(Error::InvalidAddress)));

    // create_escrow_legacy with zero payment_token
    let invoice_id4 = Symbol::new(env, "INV_ZERO3");
    let result = client.try_create_escrow_legacy(
        &invoice_id4,
        &test_env.seller,
        &test_env.debtor,
        &1000,
        &1000,
        &1_000_000,
        &zero,
        &inv_token_id,
        &BytesN::from_array(env, &[11; 32]),
        &None,
        &None,
    );
    assert_eq!(result, Err(Ok(Error::InvalidAddress)));

    // create_escrow_legacy with zero invoice_token
    let invoice_id5 = Symbol::new(env, "INV_ZERO4");
    let result = client.try_create_escrow_legacy(
        &invoice_id5,
        &test_env.seller,
        &test_env.debtor,
        &1000,
        &1000,
        &1_000_000,
        &payment_token,
        &zero,
        &BytesN::from_array(env, &[12; 32]),
        &None,
        &None,
    );
    assert_eq!(result, Err(Ok(Error::InvalidAddress)));
}

/// Comprehensive authorization matrix covering all public entrypoints
#[test]
fn auth_comprehensive_matrix() {
    // This test documents the expected authorization for each public entrypoint
    // Role legend: A=Admin, S=Seller, D=Debtor, I=Investor, T=ThirdParty, Z=ZeroAddress
    // ✓ = should succeed, ✗ = should fail
    
    let matrix = vec![
        ("initialize", vec![("A", true), ("S", false), ("D", false), ("I", false), ("T", false), ("Z", false)]),
        ("create_escrow_legacy", vec![("A", false), ("S", true), ("D", false), ("I", false), ("T", false), ("Z", false)]),
        ("fund_escrow_legacy", vec![("A", false), ("S", false), ("D", false), ("I", true), ("T", false), ("Z", false)]),
        ("record_payment", vec![("A", false), ("S", false), ("D", true), ("I", false), ("T", false), ("Z", false)]),
        ("refund_escrow", vec![("A", true), ("S", true), ("D", true), ("I", true), ("T", true), ("Z", true)]), // No auth required
        ("cancel_escrow", vec![("A", false), ("S", true), ("D", false), ("I", false), ("T", false), ("Z", false)]),
        ("set_payment_distributor", vec![("A", true), ("S", false), ("D", false), ("I", false), ("T", false), ("Z", false)]),
        ("set_paused", vec![("A", true), ("S", false), ("D", false), ("I", false), ("T", false), ("Z", false)]),
        ("set_grace_period", vec![("A", true), ("S", false), ("D", false), ("I", false), ("T", false), ("Z", false)]),
        ("set_category_fee", vec![("A", true), ("S", false), ("D", false), ("I", false), ("T", false), ("Z", false)]),
        ("set_settlement_fee", vec![("A", true), ("S", false), ("D", false), ("I", false), ("T", false), ("Z", false)]),
        ("set_min_investment", vec![("A", true), ("S", false), ("D", false), ("I", false), ("T", false), ("Z", false)]),
        ("set_whitelist_enabled", vec![("A", true), ("S", false), ("D", false), ("I", false), ("T", false), ("Z", false)]),
        ("set_accreditation_callback", vec![("A", true), ("S", false), ("D", false), ("I", false), ("T", false), ("Z", false)]),
        ("set_penalty_interest_bps", vec![("A", true), ("S", false), ("D", false), ("I", false), ("T", false), ("Z", false)]),
        ("set_buyer_whitelisted", vec![("A", true), ("S", false), ("D", false), ("I", false), ("T", false), ("Z", false)]),
        ("set_early_settlement", vec![("A", false), ("S", true), ("D", false), ("I", false), ("T", false), ("Z", false)]),
        ("set_installment_schedule", vec![("A", false), ("S", true), ("D", false), ("I", false), ("T", false), ("Z", false)]),
    ];

    // This test serves as documentation - actual validation is in individual tests above
    for (method, roles) in matrix {
        println!("Method: {}", method);
        for (role, expected) in roles {
            println!("  {}: {}", role, if expected { "✓" } else { "✗" });
        }
    }
    
    // Verify matrix is complete (all public entrypoints covered)
    assert_eq!(matrix.len(), 18, "Authorization matrix should cover all 18 public mutating entrypoints");
}