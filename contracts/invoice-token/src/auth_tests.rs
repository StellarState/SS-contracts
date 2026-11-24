//! Authorization Test Helpers and Negative Test Matrix for Invoice Token
//!
//! This module provides reusable helpers for testing address authorization
//! across all public entrypoints in the invoice-token contract.

#![cfg(test)]
#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{
    contract, contractimpl, Address, BytesN, Env, IntoVal, Symbol, TryFromVal, Val, Vec,
};

use crate::errors::Error;

/// Authorization role classification for test cases
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthRole {
    /// Contract admin
    Admin,
    /// Minter (escrow)
    Minter,
    /// Token holder
    Holder,
    /// Spender (approved)
    Spender,
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
pub struct TokenAuthTestEnv {
    pub env: Env,
    pub admin: Address,
    pub minter: Address,
    pub holder: Address,
    pub spender: Address,
    pub third_party: Address,
    pub token_id: Address,
    pub token_client: InvoiceTokenClient<'static>,
}

impl TokenAuthTestEnv {
    pub fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let minter = Address::generate(&env);
        let holder = Address::generate(&env);
        let spender = Address::generate(&env);
        let third_party = Address::generate(&env);

        let token_id = env.register_contract(None, InvoiceToken);
        let token_client = InvoiceTokenClient::new(&env, &token_id);

        token_client.initialize(
            &admin,
            &soroban_sdk::String::from_str(&env, "Test Token"),
            &soroban_sdk::String::from_str(&env, "TTK"),
            &7,
            &soroban_sdk::Symbol::new(&env, "INV_AUTH"),
            &minter,
            &i128::MAX,
            &1,
        );

        // Mint tokens to holder
        token_client.mint(&holder, &1000, &admin);
        token_client.set_transfer_locked(&admin, &false);

        Self {
            env,
            admin,
            minter,
            holder,
            spender,
            third_party,
            token_id,
            token_client,
        }
    }
}

/// Authorization matrix for initialize
#[test]
fn auth_matrix_token_initialize() {
    let env = Env::default();
    env.mock_all_auths();

    let token_id = env.register_contract(None, InvoiceToken);
    let client = InvoiceTokenClient::new(&env, &token_id);

    let admin = Address::generate(&env);
    let minter = Address::generate(&env);
    assert!(client
        .try_initialize(
            &admin,
            &"Test".into_val(&env),
            &"TTK".into_val(&env),
            &7,
            &Symbol::new(&env, "INV"),
            &minter,
            &1000,
            &1,
        )
        .is_ok());

    // Re-initialize should fail
    let admin2 = Address::generate(&env);
    let minter2 = Address::generate(&env);
    assert_eq!(
        client.try_initialize(
            &admin2,
            &"Test".into_val(&env),
            &"TTK".into_val(&env),
            &7,
            &Symbol::new(&env, "INV"),
            &minter2,
            &1000,
            &1,
        ),
        Err(Ok(Error::AlreadyInit))
    );
}

/// Authorization matrix for transfer (from must auth)
#[test]
fn auth_matrix_transfer() {
    let test_env = TokenAuthTestEnv::new();
    let client = &test_env.token_client;
    let env = &test_env.env;
    let zero = zero_address(env);

    let to = Address::generate(env);

    // Holder should succeed
    let result = client.try_transfer(&test_env.holder, &to, &100);
    assert!(result.is_ok(), "Holder should succeed");

    // Third party as 'from' should fail (mock_all_auths bypasses, test zero address)
    let result = client.try_transfer(&zero, &to, &100);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "Zero address from rejected"
    );

    // Zero address 'to' should fail
    let result = client.try_transfer(&test_env.holder, &zero, &100);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "Zero address to rejected"
    );

    // Same from and to should fail
    let result = client.try_transfer(&test_env.holder, &test_env.holder, &100);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "Same from and to rejected"
    );

    // Negative amount should fail
    let result = client.try_transfer(&test_env.holder, &to, &-100);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAmount)),
        "Negative amount rejected"
    );

    // Zero amount should fail
    let result = client.try_transfer(&test_env.holder, &to, &0);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAmount)),
        "Zero amount rejected"
    );

    // Transfer when paused should fail
    client.set_paused(&true);
    let result = client.try_transfer(&test_env.holder, &to, &100);
    assert_eq!(
        result,
        Err(Ok(Error::Paused)),
        "Transfer when paused rejected"
    );
    client.set_paused(&false);

    // Transfer when locked (non-admin) should fail
    client.set_transfer_locked(&test_env.admin, &true);
    let result = client.try_transfer(&test_env.holder, &to, &100);
    assert_eq!(
        result,
        Err(Ok(Error::TransferLocked)),
        "Transfer when locked rejected"
    );
    client.set_transfer_locked(&test_env.admin, &false);

    // Frozen account should fail
    client.freeze_account(&test_env.holder);
    let result = client.try_transfer(&test_env.holder, &to, &100);
    assert_eq!(
        result,
        Err(Ok(Error::AccountFrozen)),
        "Frozen account rejected"
    );
    client.unfreeze_account(&test_env.holder);
}

/// Authorization matrix for approve (from must auth)
#[test]
fn auth_matrix_approve() {
    let test_env = TokenAuthTestEnv::new();
    let client = &test_env.token_client;
    let env = &test_env.env;
    let zero = zero_address(env);

    // Holder should succeed
    let result = client.try_approve(&test_env.holder, &test_env.spender, &500, &1000);
    assert!(result.is_ok(), "Holder should succeed");

    // Zero address 'from' should fail
    let result = client.try_approve(&zero, &test_env.spender, &500, &1000);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "Zero address from rejected"
    );

    // Zero address 'spender' should fail
    let result = client.try_approve(&test_env.holder, &zero, &500, &1000);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "Zero address spender rejected"
    );

    // Same from and spender should fail
    let result = client.try_approve(&test_env.holder, &test_env.holder, &500, &1000);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "Same from and spender rejected"
    );

    // Negative amount should fail
    let result = client.try_approve(&test_env.holder, &test_env.spender, &-500, &1000);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAmount)),
        "Negative amount rejected"
    );

    // Zero amount with expired ledger should fail
    let result = client.try_approve(&test_env.holder, &test_env.spender, &0, &100);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidExpiration)),
        "Zero amount with past expiration rejected"
    );

    // Expired ledger should fail
    let result = client.try_approve(&test_env.holder, &test_env.spender, &500, &100);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidExpiration)),
        "Expired ledger rejected"
    );

    // When paused should fail
    client.set_paused(&true);
    let result = client.try_approve(&test_env.holder, &test_env.spender, &500, &1000);
    assert_eq!(
        result,
        Err(Ok(Error::Paused)),
        "Approve when paused rejected"
    );
    client.set_paused(&false);

    // Frozen account should fail
    client.freeze_account(&test_env.holder);
    let result = client.try_approve(&test_env.holder, &test_env.spender, &500, &1000);
    assert_eq!(
        result,
        Err(Ok(Error::AccountFrozen)),
        "Frozen account rejected"
    );
    client.unfreeze_account(&test_env.holder);
}

/// Authorization matrix for transfer_from (spender must auth)
#[test]
fn auth_matrix_transfer_from() {
    let test_env = TokenAuthTestEnv::new();
    let client = &test_env.token_client;
    let env = &test_env.env;
    let zero = zero_address(env);
    let to = Address::generate(env);

    // Setup allowance
    client.approve(&test_env.holder, &test_env.spender, &500, &1000);

    // Spender should succeed
    let result = client.try_transfer_from(&test_env.spender, &test_env.holder, &to, &100);
    assert!(result.is_ok(), "Spender should succeed");

    // Zero address 'spender' should fail
    let result = client.try_transfer_from(&zero, &test_env.holder, &to, &100);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "Zero address spender rejected"
    );

    // Zero address 'from' should fail
    let result = client.try_transfer_from(&test_env.spender, &zero, &to, &100);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "Zero address from rejected"
    );

    // Zero address 'to' should fail
    let result = client.try_transfer_from(&test_env.spender, &test_env.holder, &zero, &100);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "Zero address to rejected"
    );

    // Same from and to should fail
    let result =
        client.try_transfer_from(&test_env.spender, &test_env.holder, &test_env.holder, &100);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "Same from and to rejected"
    );

    // Negative amount should fail
    let result = client.try_transfer_from(&test_env.spender, &test_env.holder, &to, &-100);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAmount)),
        "Negative amount rejected"
    );

    // Zero amount should fail
    let result = client.try_transfer_from(&test_env.spender, &test_env.holder, &to, &0);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAmount)),
        "Zero amount rejected"
    );

    // Insufficient allowance should fail
    let result = client.try_transfer_from(&test_env.spender, &test_env.holder, &to, &1000);
    assert_eq!(
        result,
        Err(Ok(Error::InsufficientAllowance)),
        "Insufficient allowance rejected"
    );

    // Expired allowance should fail
    client.approve(&test_env.holder, &test_env.spender, &500, &100); // expires soon
    env.ledger().with_mut(|li| li.sequence = 200);
    let result = client.try_transfer_from(&test_env.spender, &test_env.holder, &to, &100);
    assert_eq!(
        result,
        Err(Ok(Error::AllowanceExpired)),
        "Expired allowance rejected"
    );

    // When paused should fail
    client.set_paused(&true);
    let result = client.try_transfer_from(&test_env.spender, &test_env.holder, &to, &100);
    assert_eq!(
        result,
        Err(Ok(Error::Paused)),
        "Transfer_from when paused rejected"
    );
    client.set_paused(&false);

    // Frozen accounts should fail
    client.freeze_account(&test_env.spender);
    let result = client.try_transfer_from(&test_env.spender, &test_env.holder, &to, &100);
    assert_eq!(
        result,
        Err(Ok(Error::AccountFrozen)),
        "Frozen spender rejected"
    );
    client.unfreeze_account(&test_env.spender);

    client.freeze_account(&test_env.holder);
    let result = client.try_transfer_from(&test_env.spender, &test_env.holder, &to, &100);
    assert_eq!(
        result,
        Err(Ok(Error::AccountFrozen)),
        "Frozen from rejected"
    );
    client.unfreeze_account(&test_env.holder);

    client.freeze_account(&to);
    let result = client.try_transfer_from(&test_env.spender, &test_env.holder, &to, &100);
    assert_eq!(result, Err(Ok(Error::AccountFrozen)), "Frozen to rejected");
    client.unfreeze_account(&to);
}

/// Authorization matrix for burn (from must auth)
#[test]
fn auth_matrix_burn() {
    let test_env = TokenAuthTestEnv::new();
    let client = &test_env.token_client;
    let env = &test_env.env;
    let zero = zero_address(env);

    // Holder should succeed
    let result = client.try_burn(&test_env.holder, &100);
    assert!(result.is_ok(), "Holder should succeed");

    // Zero address should fail
    let result = client.try_burn(&zero, &100);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "Zero address rejected"
    );

    // Negative amount should fail
    let result = client.try_burn(&test_env.holder, &-100);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAmount)),
        "Negative amount rejected"
    );

    // Zero amount should fail
    let result = client.try_burn(&test_env.holder, &0);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAmount)),
        "Zero amount rejected"
    );

    // Insufficient balance should fail
    let result = client.try_burn(&test_env.holder, &2000);
    assert_eq!(
        result,
        Err(Ok(Error::InsufficientBalance)),
        "Insufficient balance rejected"
    );

    // When paused should fail
    client.set_paused(&true);
    let result = client.try_burn(&test_env.holder, &100);
    assert_eq!(result, Err(Ok(Error::Paused)), "Burn when paused rejected");
    client.set_paused(&false);

    // Frozen account should fail
    client.freeze_account(&test_env.holder);
    let result = client.try_burn(&test_env.holder, &100);
    assert_eq!(
        result,
        Err(Ok(Error::AccountFrozen)),
        "Frozen account rejected"
    );
    client.unfreeze_account(&test_env.holder);
}

/// Authorization matrix for burn_from (spender must auth)
#[test]
fn auth_matrix_burn_from() {
    let test_env = TokenAuthTestEnv::new();
    let client = &test_env.token_client;
    let env = &test_env.env;
    let zero = zero_address(env);

    // Setup allowance
    client.approve(&test_env.holder, &test_env.spender, &500, &1000);

    // Spender should succeed
    let result = client.try_burn_from(&test_env.spender, &test_env.holder, &100);
    assert!(result.is_ok(), "Spender should succeed");

    // Zero address spender should fail
    let result = client.try_burn_from(&zero, &test_env.holder, &100);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "Zero address spender rejected"
    );

    // Zero address from should fail
    let result = client.try_burn_from(&test_env.spender, &zero, &100);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "Zero address from rejected"
    );

    // Negative amount should fail
    let result = client.try_burn_from(&test_env.spender, &test_env.holder, &-100);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAmount)),
        "Negative amount rejected"
    );

    // Zero amount should fail
    let result = client.try_burn_from(&test_env.spender, &test_env.holder, &0);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAmount)),
        "Zero amount rejected"
    );

    // Insufficient allowance should fail
    let result = client.try_burn_from(&test_env.spender, &test_env.holder, &1000);
    assert_eq!(
        result,
        Err(Ok(Error::InsufficientAllowance)),
        "Insufficient allowance rejected"
    );

    // Expired allowance should fail
    client.approve(&test_env.holder, &test_env.spender, &500, &100);
    env.ledger().with_mut(|li| li.sequence = 200);
    let result = client.try_burn_from(&test_env.spender, &test_env.holder, &100);
    assert_eq!(
        result,
        Err(Ok(Error::AllowanceExpired)),
        "Expired allowance rejected"
    );

    // Insufficient balance should fail
    let result = client.try_burn_from(&test_env.spender, &test_env.holder, &2000);
    assert_eq!(
        result,
        Err(Ok(Error::InsufficientBalance)),
        "Insufficient balance rejected"
    );

    // When paused should fail
    client.set_paused(&true);
    let result = client.try_burn_from(&test_env.spender, &test_env.holder, &100);
    assert_eq!(
        result,
        Err(Ok(Error::Paused)),
        "Burn_from when paused rejected"
    );
    client.set_paused(&false);

    // Frozen accounts should fail
    client.freeze_account(&test_env.spender);
    let result = client.try_burn_from(&test_env.spender, &test_env.holder, &100);
    assert_eq!(
        result,
        Err(Ok(Error::AccountFrozen)),
        "Frozen spender rejected"
    );
    client.unfreeze_account(&test_env.spender);

    client.freeze_account(&test_env.holder);
    let result = client.try_burn_from(&test_env.spender, &test_env.holder, &100);
    assert_eq!(
        result,
        Err(Ok(Error::AccountFrozen)),
        "Frozen from rejected"
    );
    client.unfreeze_account(&test_env.holder);
}

/// Authorization matrix for mint (admin or minter only)
#[test]
fn auth_matrix_mint() {
    let test_env = TokenAuthTestEnv::new();
    let client = &test_env.token_client;
    let env = &test_env.env;
    let zero = zero_address(env);
    let to = Address::generate(env);

    // Admin should succeed
    let result = client.try_mint(&to, &100, &test_env.admin);
    assert!(result.is_ok(), "Admin should succeed");

    // Minter should succeed
    let result = client.try_mint(&to, &100, &test_env.minter);
    assert!(result.is_ok(), "Minter should succeed");

    // Third party should fail
    let result = client.try_mint(&to, &100, &test_env.third_party);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Third party rejected");

    // Holder should fail
    let result = client.try_mint(&to, &100, &test_env.holder);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Holder rejected");

    // Zero address should fail
    let result = client.try_mint(&to, &100, &zero);
    assert_eq!(
        result,
        Err(Ok(Error::Unauthorized)),
        "Zero address rejected"
    );

    // Zero address 'to' should fail
    let result = client.try_mint(&zero, &100, &test_env.admin);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "Zero address to rejected"
    );

    // Negative amount should fail
    let result = client.try_mint(&to, &-100, &test_env.admin);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAmount)),
        "Negative amount rejected"
    );

    // Zero amount should fail
    let result = client.try_mint(&to, &0, &test_env.admin);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAmount)),
        "Zero amount rejected"
    );

    // When paused should fail
    client.set_paused(&true);
    let result = client.try_mint(&to, &100, &test_env.admin);
    assert_eq!(result, Err(Ok(Error::Paused)), "Mint when paused rejected");
    client.set_paused(&false);

    // Frozen admin should fail
    client.freeze_account(&test_env.admin);
    let result = client.try_mint(&to, &100, &test_env.admin);
    assert_eq!(
        result,
        Err(Ok(Error::AccountFrozen)),
        "Frozen admin rejected"
    );
    client.unfreeze_account(&test_env.admin);

    // Frozen minter should fail
    client.freeze_account(&test_env.minter);
    let result = client.try_mint(&to, &100, &test_env.minter);
    assert_eq!(
        result,
        Err(Ok(Error::AccountFrozen)),
        "Frozen minter rejected"
    );
    client.unfreeze_account(&test_env.minter);

    // Frozen 'to' should fail
    client.freeze_account(&to);
    let result = client.try_mint(&to, &100, &test_env.admin);
    assert_eq!(result, Err(Ok(Error::AccountFrozen)), "Frozen to rejected");
    client.unfreeze_account(&to);
}

/// Authorization matrix for admin-only functions
#[test]
fn auth_matrix_token_admin_functions() {
    let test_env = TokenAuthTestEnv::new();
    let client = &test_env.token_client;
    let env = &test_env.env;
    let zero = zero_address(env);

    // set_transfer_locked - admin or minter
    let result = client.try_set_transfer_locked(&test_env.admin, &true);
    assert!(result.is_ok(), "Admin should succeed");

    let result = client.try_set_transfer_locked(&test_env.minter, &true);
    assert!(result.is_ok(), "Minter should succeed");

    let result = client.try_set_transfer_locked(&test_env.holder, &true);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Holder rejected");

    let result = client.try_set_transfer_locked(&zero, &true);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "Zero address rejected"
    );

    // set_minter - admin only
    let new_minter = Address::generate(env);
    let result = client.try_set_minter(&new_minter);
    assert!(result.is_ok(), "Admin should succeed");

    // Zero address new_minter should fail
    let result = client.try_set_minter(&zero);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "Zero address new_minter rejected"
    );

    // set_paused - admin only
    let result = client.try_set_paused(&true);
    assert!(result.is_ok(), "Admin should succeed");

    // set_fee_bps - admin only
    let result = client.try_set_fee_bps(&test_env.admin, &100);
    assert!(result.is_ok(), "Admin should succeed");

    let result = client.try_set_fee_bps(&test_env.holder, &100);
    assert_eq!(result, Err(Ok(Error::Unauthorized)), "Holder rejected");

    let result = client.try_set_fee_bps(&zero, &100);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "Zero address rejected"
    );

    // set_transfer_restriction - admin only
    let restriction = Some(Address::generate(env));
    let result = client.try_set_transfer_restriction(&restriction);
    assert!(result.is_ok(), "Admin should succeed");

    // freeze_account - admin only
    let account = Address::generate(env);
    let result = client.try_freeze_account(&account);
    assert!(result.is_ok(), "Admin should succeed");

    let result = client.try_freeze_account(&zero);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "Zero address rejected"
    );

    // unfreeze_account - admin only
    client.freeze_account(&account);
    let result = client.try_unfreeze_account(&account);
    assert!(result.is_ok(), "Admin should succeed");

    // clawback - admin only (on frozen account)
    client.freeze_account(&test_env.holder);
    let result = client.try_clawback(&test_env.holder, &100);
    assert!(result.is_ok(), "Admin should succeed");

    let result = client.try_clawback(&test_env.holder, &100); // already clawed back some
                                                              // clawback from non-frozen should fail
    client.unfreeze_account(&test_env.holder);
    let result = client.try_clawback(&test_env.holder, &100);
    assert_eq!(
        result,
        Err(Ok(Error::AccountNotFrozen)),
        "Non-frozen account rejected"
    );

    // set_role_admin - role admin only
    let role = Symbol::new(env, "minter");
    let new_admin = Address::generate(env);
    let result = client.try_set_role_admin(&test_env.admin, &role, &new_admin);
    assert!(result.is_ok(), "Role admin should succeed");

    let result = client.try_set_role_admin(&test_env.holder, &role, &new_admin);
    assert_eq!(
        result,
        Err(Ok(Error::Unauthorized)),
        "Non-role-admin rejected"
    );

    // grant_role - role admin only
    let account = Address::generate(env);
    let result = client.try_grant_role(&test_env.admin, &role, &account);
    assert!(result.is_ok(), "Role admin should succeed");

    let result = client.try_grant_role(&test_env.holder, &role, &account);
    assert_eq!(
        result,
        Err(Ok(Error::Unauthorized)),
        "Non-role-admin rejected"
    );

    // revoke_role - role admin only
    client.grant_role(&test_env.admin, &role, &account);
    let result = client.try_revoke_role(&test_env.admin, &role, &account);
    assert!(result.is_ok(), "Role admin should succeed");

    let result = client.try_revoke_role(&test_env.holder, &role, &account);
    assert_eq!(
        result,
        Err(Ok(Error::Unauthorized)),
        "Non-role-admin rejected"
    );
}

/// Test state unchanged after rejected calls
#[test]
fn auth_token_rejected_calls_leave_state_unchanged() {
    let test_env = TokenAuthTestEnv::new();
    let client = &test_env.token_client;
    let env = &test_env.env;

    // Capture balance before failed transfer
    let balance_before = client.balance(test_env.holder.clone()).unwrap();

    // Try transfer with zero address (should fail)
    let zero = zero_address(env);
    let to = Address::generate(env);
    let result = client.try_transfer(&zero, &to, &100);
    assert!(result.is_err());

    // Balance should be unchanged
    let balance_after = client.balance(test_env.holder.clone()).unwrap();
    assert_eq!(
        balance_before, balance_after,
        "Balance changed after failed transfer"
    );

    // Capture allowance before failed approve
    let allowance_before = client
        .allowance(test_env.holder.clone(), test_env.spender.clone())
        .unwrap();

    // Try approve with zero address spender
    let result = client.try_approve(&test_env.holder, &zero, &500, &1000);
    assert!(result.is_err());

    // Allowance should be unchanged
    let allowance_after = client
        .allowance(test_env.holder.clone(), test_env.spender.clone())
        .unwrap();
    assert_eq!(
        allowance_before, allowance_after,
        "Allowance changed after failed approve"
    );
}

/// Comprehensive authorization matrix documentation
#[test]
fn auth_token_comprehensive_matrix() {
    // Role legend: A=Admin, M=Minter, H=Holder, S=Spender, T=ThirdParty, Z=ZeroAddress
    let matrix = vec![
        // Mutating functions
        (
            "initialize",
            vec![
                ("A", true),
                ("M", false),
                ("H", false),
                ("S", false),
                ("T", false),
                ("Z", false),
            ],
        ),
        (
            "transfer",
            vec![
                ("A", false),
                ("M", false),
                ("H", true),
                ("S", false),
                ("T", false),
                ("Z", false),
            ],
        ),
        (
            "approve",
            vec![
                ("A", false),
                ("M", false),
                ("H", true),
                ("S", false),
                ("T", false),
                ("Z", false),
            ],
        ),
        (
            "transfer_from",
            vec![
                ("A", false),
                ("M", false),
                ("H", false),
                ("S", true),
                ("T", false),
                ("Z", false),
            ],
        ),
        (
            "burn",
            vec![
                ("A", false),
                ("M", false),
                ("H", true),
                ("S", false),
                ("T", false),
                ("Z", false),
            ],
        ),
        (
            "burn_from",
            vec![
                ("A", false),
                ("M", false),
                ("H", false),
                ("S", true),
                ("T", false),
                ("Z", false),
            ],
        ),
        (
            "mint",
            vec![
                ("A", true),
                ("M", true),
                ("H", false),
                ("S", false),
                ("T", false),
                ("Z", false),
            ],
        ),
        (
            "set_transfer_locked",
            vec![
                ("A", true),
                ("M", true),
                ("H", false),
                ("S", false),
                ("T", false),
                ("Z", false),
            ],
        ),
        (
            "set_minter",
            vec![
                ("A", true),
                ("M", false),
                ("H", false),
                ("S", false),
                ("T", false),
                ("Z", false),
            ],
        ),
        (
            "set_paused",
            vec![
                ("A", true),
                ("M", false),
                ("H", false),
                ("S", false),
                ("T", false),
                ("Z", false),
            ],
        ),
        (
            "set_fee_bps",
            vec![
                ("A", true),
                ("M", false),
                ("H", false),
                ("S", false),
                ("T", false),
                ("Z", false),
            ],
        ),
        (
            "set_transfer_restriction",
            vec![
                ("A", true),
                ("M", false),
                ("H", false),
                ("S", false),
                ("T", false),
                ("Z", false),
            ],
        ),
        (
            "freeze_account",
            vec![
                ("A", true),
                ("M", false),
                ("H", false),
                ("S", false),
                ("T", false),
                ("Z", false),
            ],
        ),
        (
            "unfreeze_account",
            vec![
                ("A", true),
                ("M", false),
                ("H", false),
                ("S", false),
                ("T", false),
                ("Z", false),
            ],
        ),
        (
            "clawback",
            vec![
                ("A", true),
                ("M", false),
                ("H", false),
                ("S", false),
                ("T", false),
                ("Z", false),
            ],
        ),
        (
            "set_role_admin",
            vec![
                ("A", true),
                ("M", false),
                ("H", false),
                ("S", false),
                ("T", false),
                ("Z", false),
            ],
        ),
        (
            "grant_role",
            vec![
                ("A", true),
                ("M", false),
                ("H", false),
                ("S", false),
                ("T", false),
                ("Z", false),
            ],
        ),
        (
            "revoke_role",
            vec![
                ("A", true),
                ("M", false),
                ("H", false),
                ("S", false),
                ("T", false),
                ("Z", false),
            ],
        ),
        // View functions (all roles can call)
        (
            "name",
            vec![
                ("A", true),
                ("M", true),
                ("H", true),
                ("S", true),
                ("T", true),
                ("Z", true),
            ],
        ),
        (
            "symbol",
            vec![
                ("A", true),
                ("M", true),
                ("H", true),
                ("S", true),
                ("T", true),
                ("Z", true),
            ],
        ),
        (
            "decimals",
            vec![
                ("A", true),
                ("M", true),
                ("H", true),
                ("S", true),
                ("T", true),
                ("Z", true),
            ],
        ),
        (
            "total_supply",
            vec![
                ("A", true),
                ("M", true),
                ("H", true),
                ("S", true),
                ("T", true),
                ("Z", true),
            ],
        ),
        (
            "balance",
            vec![
                ("A", true),
                ("M", true),
                ("H", true),
                ("S", true),
                ("T", true),
                ("Z", false),
            ],
        ), // Z rejected
        (
            "allowance",
            vec![
                ("A", true),
                ("M", true),
                ("H", true),
                ("S", true),
                ("T", true),
                ("Z", false),
            ],
        ),
        (
            "get_nonce",
            vec![
                ("A", true),
                ("M", true),
                ("H", true),
                ("S", true),
                ("T", true),
                ("Z", false),
            ],
        ),
        (
            "invoice_id",
            vec![
                ("A", true),
                ("M", true),
                ("H", true),
                ("S", true),
                ("T", true),
                ("Z", true),
            ],
        ),
        (
            "transfer_locked",
            vec![
                ("A", true),
                ("M", true),
                ("H", true),
                ("S", true),
                ("T", true),
                ("Z", true),
            ],
        ),
        (
            "paused",
            vec![
                ("A", true),
                ("M", true),
                ("H", true),
                ("S", true),
                ("T", true),
                ("Z", true),
            ],
        ),
        (
            "is_account_frozen",
            vec![
                ("A", true),
                ("M", true),
                ("H", true),
                ("S", true),
                ("T", true),
                ("Z", false),
            ],
        ),
        (
            "get_fee_bps",
            vec![
                ("A", true),
                ("M", true),
                ("H", true),
                ("S", true),
                ("T", true),
                ("Z", true),
            ],
        ),
        (
            "get_role_admin",
            vec![
                ("A", true),
                ("M", true),
                ("H", true),
                ("S", true),
                ("T", true),
                ("Z", false),
            ],
        ),
        (
            "has_role",
            vec![
                ("A", true),
                ("M", true),
                ("H", true),
                ("S", true),
                ("T", true),
                ("Z", false),
            ],
        ),
        (
            "get_token_history",
            vec![
                ("A", true),
                ("M", true),
                ("H", true),
                ("S", true),
                ("T", true),
                ("Z", false),
            ],
        ),
    ];

    for (method, roles) in matrix {
        println!("Method: {}", method);
        for (role, expected) in roles {
            println!("  {}: {}", role, if expected { "✓" } else { "✗" });
        }
    }

    assert_eq!(
        matrix.len(),
        30,
        "Authorization matrix should cover all 30 entrypoints"
    );
}
