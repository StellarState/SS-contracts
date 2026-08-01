#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]
//! Integration tests for the InvoiceToken contract.
//!
//! These tests spin up a real `InvoiceToken` instance next to a lightweight
//! mock settlement escrow. The mock escrow drives the cross-contract
//! settlement callback flow that `invoice-escrow` performs on-chain:
//!
//! - On funding, it mints fractional ownership shares to the investor.
//! - On settlement, it burns the investor's shares (burn-on-settlement).
//!
//! The suite asserts the returned error codes on every failure path and
//! verifies state storage persistence (balances, total supply, allowances,
//! transfer-lock) after each execution.

use super::{InvoiceToken, InvoiceTokenClient};
use crate::errors::Error;
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{
    contract, contractimpl, Address, ConversionError, Env, IntoVal, InvokeError,
    String as SorobanString, Symbol, TryFromVal, TryIntoVal, Val, Vec,
};

fn parse_event(env: &Env, event: &soroban_sdk::xdr::ContractEvent) -> (Address, Vec<Val>, Val) {
    let contract_addr = match &event.contract_id {
        Some(hash) => Address::try_from_val(
            env,
            &soroban_sdk::xdr::ScVal::Address(soroban_sdk::xdr::ScAddress::Contract(hash.clone())),
        )
        .unwrap(),
        None => Address::generate(env),
    };
    let soroban_sdk::xdr::ContractEventBody::V0(v0) = &event.body;
    let topics = Vec::<Val>::try_from_val(
        env,
        &soroban_sdk::xdr::ScVal::Vec(Some(v0.topics.clone().into())),
    )
    .unwrap();
    let data = Val::try_from_val(env, &v0.data).unwrap();
    (contract_addr, topics, data)
}

/// Mock settlement escrow that drives the burn-on-settlement callback against
/// the real `InvoiceToken` contract, mirroring `invoice-escrow`'s on-chain
/// calls (mint on funding, burn on settlement, unlock after settlement).
#[contract]
struct MockSettlementEscrow;

/// Flatten a cross-contract `try_` client result into `Result<(), Error>`,
/// forwarding the token's own typed error so tests can assert on it.
fn relay(
    res: Result<Result<(), ConversionError>, Result<Error, InvokeError>>,
) -> Result<(), Error> {
    match res {
        Ok(Ok(())) => Ok(()),
        Err(Ok(e)) => Err(e),
        _ => panic!("unexpected cross-contract result"),
    }
}

#[contractimpl]
impl MockSettlementEscrow {
    /// Callback invoked when the invoice is funded: mint shares to `to`.
    pub fn mint_on_fund(env: Env, token: Address, to: Address, amount: i128) -> Result<(), Error> {
        let client = InvoiceTokenClient::new(&env, &token);
        relay(client.try_mint(&to, &amount, &env.current_contract_address()))
    }

    /// Settlement callback: burn the investor's fractional shares.
    pub fn burn_on_settlement(
        env: Env,
        token: Address,
        from: Address,
        amount: i128,
    ) -> Result<(), Error> {
        let client = InvoiceTokenClient::new(&env, &token);
        relay(client.try_burn(&from, &amount))
    }

    /// Settlement callback using allowance-based burn (`burn_from`).
    pub fn burn_from_on_settlement(
        env: Env,
        token: Address,
        from: Address,
        amount: i128,
    ) -> Result<(), Error> {
        let client = InvoiceTokenClient::new(&env, &token);
        relay(client.try_burn_from(&env.current_contract_address(), &from, &amount))
    }

    /// Unlock token transfers once the invoice is fully settled/refunded.
    pub fn unlock_token(env: Env, token: Address) -> Result<(), Error> {
        let client = InvoiceTokenClient::new(&env, &token);
        relay(client.try_set_transfer_locked(&env.current_contract_address(), &false))
    }
}

/// All addresses and clients needed by most tests.
struct Ctx {
    env: Env,
    admin: Address,
    investor: Address,
    escrow_id: Address,
    escrow: MockSettlementEscrowClient<'static>,
    token_id: Address,
    token: InvoiceTokenClient<'static>,
    invoice_id: Symbol,
}

/// Stand up a real `InvoiceToken` contract with the mock escrow as minter.
fn setup(env: &Env) -> Ctx {
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(env);
    let investor = Address::generate(env);

    let escrow_id = env.register(MockSettlementEscrow, ());
    let escrow = MockSettlementEscrowClient::new(env, &escrow_id);
    let escrow_client =
        unsafe { core::mem::transmute::<_, MockSettlementEscrowClient<'static>>(escrow) };

    let token_id = env.register(InvoiceToken, ());
    let token = InvoiceTokenClient::new(env, &token_id);
    let token_client = unsafe { core::mem::transmute::<_, InvoiceTokenClient<'static>>(token) };

    let invoice_id = Symbol::new(env, "INV_SETTLE");
    token_client.initialize(
        &admin,
        &SorobanString::from_str(env, "Invoice Settle"),
        &SorobanString::from_str(env, "INVS"),
        &7,
        &invoice_id,
        &escrow_id,
    );

    Ctx {
        env: env.clone(),
        admin,
        investor,
        escrow_id,
        escrow: escrow_client,
        token_id,
        token: token_client,
        invoice_id,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 1. Burn-on-settlement callback — happy path
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_burn_on_settlement_callback_happy_path() {
    let env = Env::default();
    let ctx = setup(&env);

    // Funding: the escrow mints shares to the investor.
    ctx.escrow
        .mint_on_fund(&ctx.token_id, &ctx.investor, &1_000);
    assert_eq!(ctx.token.balance(&ctx.investor), 1_000);
    assert_eq!(ctx.token.total_supply(), 1_000);
    assert!(ctx.token.transfer_locked());

    // Settlement: the escrow burns the investor's shares.
    ctx.escrow
        .burn_on_settlement(&ctx.token_id, &ctx.investor, &400);

    // Burn event emitted with correct topics and amount. Captured immediately
    // after the burn so it reflects the settlement callback invocation.
    let events = env.events().all();
    let event = events.events().last().unwrap();
    let (_addr, topics, data) = parse_event(&env, event);
    assert_eq!(
        topics,
        (Symbol::new(&env, "burn"), ctx.investor.clone()).into_val(&env)
    );
    let emitted: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(emitted, 400);

    assert_eq!(ctx.token.balance(&ctx.investor), 600);
    assert_eq!(ctx.token.total_supply(), 600);

    // State persists after the callback.
    assert_eq!(ctx.token.balance(&ctx.admin), 0);
    assert_eq!(ctx.token.balance(&ctx.investor), 600);
    assert_eq!(ctx.token.total_supply(), 600);
    assert_eq!(ctx.token.invoice_id(), ctx.invoice_id);
}

// ──────────────────────────────────────────────────────────────────────────────
// 2. Burn-on-settlement callback — investor can transfer remaining shares
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_after_settlement_unlock_and_transfer() {
    let env = Env::default();
    let ctx = setup(&env);

    ctx.escrow
        .mint_on_fund(&ctx.token_id, &ctx.investor, &1_000);

    // Partial burn during settlement.
    ctx.escrow
        .burn_on_settlement(&ctx.token_id, &ctx.investor, &300);
    assert_eq!(ctx.token.balance(&ctx.investor), 700);

    // Escrow unlocks transfers after the settlement callback.
    ctx.escrow.unlock_token(&ctx.token_id);
    assert!(!ctx.token.transfer_locked());

    // Investor can now transfer the remaining shares.
    let recipient = Address::generate(&env);
    ctx.token.transfer(&ctx.investor, &recipient, &700);
    assert_eq!(ctx.token.balance(&ctx.investor), 0);
    assert_eq!(ctx.token.balance(&recipient), 700);
    assert_eq!(ctx.token.total_supply(), 700);
}

// ──────────────────────────────────────────────────────────────────────────────
// 3. Burn-on-settlement callback — allowance-based burn_from
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_burn_from_on_settlement_callback() {
    let env = Env::default();
    let ctx = setup(&env);

    ctx.escrow
        .mint_on_fund(&ctx.token_id, &ctx.investor, &1_000);

    // Investor approves the escrow (spender) to burn on settlement.
    let expiration = env.ledger().sequence() + 100;
    ctx.token
        .approve(&ctx.investor, &ctx.escrow_id, &500, &expiration);
    assert_eq!(ctx.token.allowance(&ctx.investor, &ctx.escrow_id), 500);

    // Settlement callback burns via allowance.
    ctx.escrow
        .burn_from_on_settlement(&ctx.token_id, &ctx.investor, &200);
    assert_eq!(ctx.token.balance(&ctx.investor), 800);
    assert_eq!(ctx.token.total_supply(), 800);
    // Allowance is reduced and persists.
    assert_eq!(ctx.token.allowance(&ctx.investor, &ctx.escrow_id), 300);
}

// ──────────────────────────────────────────────────────────────────────────────
// 4. Failure paths — error code emissions
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_burn_callback_insufficient_balance_rejected() {
    let env = Env::default();
    let ctx = setup(&env);

    ctx.escrow.mint_on_fund(&ctx.token_id, &ctx.investor, &100);

    let result = ctx
        .escrow
        .try_burn_on_settlement(&ctx.token_id, &ctx.investor, &200);
    assert_eq!(result, Err(Ok(Error::InsufficientBalance)));
    // State unchanged.
    assert_eq!(ctx.token.balance(&ctx.investor), 100);
    assert_eq!(ctx.token.total_supply(), 100);
}

#[test]
fn test_integration_burn_callback_invalid_amounts_rejected() {
    let env = Env::default();
    let ctx = setup(&env);

    ctx.escrow.mint_on_fund(&ctx.token_id, &ctx.investor, &100);

    assert_eq!(
        ctx.escrow
            .try_burn_on_settlement(&ctx.token_id, &ctx.investor, &0),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        ctx.escrow
            .try_burn_on_settlement(&ctx.token_id, &ctx.investor, &-1),
        Err(Ok(Error::InvalidAmount))
    );
    // State unchanged.
    assert_eq!(ctx.token.balance(&ctx.investor), 100);
    assert_eq!(ctx.token.total_supply(), 100);
}

#[test]
fn test_integration_burn_from_callback_insufficient_allowance_rejected() {
    let env = Env::default();
    let ctx = setup(&env);

    ctx.escrow
        .mint_on_fund(&ctx.token_id, &ctx.investor, &1_000);

    // No allowance set yet.
    let result = ctx
        .escrow
        .try_burn_from_on_settlement(&ctx.token_id, &ctx.investor, &100);
    assert_eq!(result, Err(Ok(Error::InsufficientAllowance)));
    assert_eq!(ctx.token.balance(&ctx.investor), 1_000);
    assert_eq!(ctx.token.total_supply(), 1_000);

    // Allowance set but too small.
    let expiration = env.ledger().sequence() + 100;
    ctx.token
        .approve(&ctx.investor, &ctx.escrow_id, &50, &expiration);
    let result = ctx
        .escrow
        .try_burn_from_on_settlement(&ctx.token_id, &ctx.investor, &100);
    assert_eq!(result, Err(Ok(Error::InsufficientAllowance)));
    assert_eq!(ctx.token.balance(&ctx.investor), 1_000);
}

#[test]
fn test_integration_burn_from_callback_expired_allowance_rejected() {
    let env = Env::default();
    let ctx = setup(&env);

    ctx.escrow
        .mint_on_fund(&ctx.token_id, &ctx.investor, &1_000);

    // Approve with an expiration, then move the ledger past it so the
    // allowance is expired at settlement time.
    env.ledger().with_mut(|l| l.sequence_number = 1_000);
    let expiration = env.ledger().sequence() + 10;
    ctx.token
        .approve(&ctx.investor, &ctx.escrow_id, &100, &expiration);
    env.ledger()
        .with_mut(|l| l.sequence_number = expiration + 1);

    let result = ctx
        .escrow
        .try_burn_from_on_settlement(&ctx.token_id, &ctx.investor, &100);
    assert_eq!(result, Err(Ok(Error::AllowanceExpired)));
    assert_eq!(ctx.token.balance(&ctx.investor), 1_000);
    assert_eq!(ctx.token.total_supply(), 1_000);
}

#[test]
fn test_integration_burn_callback_paused_rejected() {
    let env = Env::default();
    let ctx = setup(&env);

    ctx.escrow.mint_on_fund(&ctx.token_id, &ctx.investor, &100);

    ctx.token.set_paused(&true);
    assert!(ctx.token.paused());

    assert_eq!(
        ctx.escrow
            .try_burn_on_settlement(&ctx.token_id, &ctx.investor, &50),
        Err(Ok(Error::Paused))
    );
    assert_eq!(
        ctx.escrow
            .try_burn_from_on_settlement(&ctx.token_id, &ctx.investor, &50),
        Err(Ok(Error::Paused))
    );
    // State unchanged.
    assert_eq!(ctx.token.balance(&ctx.investor), 100);
    assert_eq!(ctx.token.total_supply(), 100);
}

#[test]
fn test_integration_burn_callback_on_uninitialized_rejected() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    // A token that was never initialized.
    let token_id = env.register(InvoiceToken, ());
    let token = InvoiceTokenClient::new(&env, &token_id);

    let escrow_id = env.register(MockSettlementEscrow, ());
    let escrow = MockSettlementEscrowClient::new(&env, &escrow_id);

    let investor = Address::generate(&env);
    assert_eq!(
        escrow.try_burn_on_settlement(&token_id, &investor, &100),
        Err(Ok(Error::NotInit))
    );
    assert_eq!(token.try_total_supply(), Err(Ok(Error::NotInit)));
}

// ──────────────────────────────────────────────────────────────────────────────
// 5. State storage persistence after execution
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_burn_on_settlement_state_persistence() {
    let env = Env::default();
    let ctx = setup(&env);

    ctx.escrow
        .mint_on_fund(&ctx.token_id, &ctx.investor, &1_000);

    // Re-read state before the callback.
    assert_eq!(ctx.token.balance(&ctx.investor), 1_000);
    assert_eq!(ctx.token.total_supply(), 1_000);
    assert!(ctx.token.transfer_locked());

    // Burn and unlock exactly as the settlement lifecycle does.
    ctx.escrow
        .burn_on_settlement(&ctx.token_id, &ctx.investor, &1_000);
    ctx.escrow.unlock_token(&ctx.token_id);

    // Balances, supply, and lock state persist after execution.
    assert_eq!(ctx.token.balance(&ctx.investor), 0);
    assert_eq!(ctx.token.balance(&ctx.admin), 0);
    assert_eq!(ctx.token.total_supply(), 0);
    assert!(!ctx.token.transfer_locked());
    assert_eq!(ctx.token.decimals(), 7);
    assert_eq!(
        ctx.token.name(),
        SorobanString::from_str(&env, "Invoice Settle")
    );
    assert_eq!(ctx.token.symbol(), SorobanString::from_str(&env, "INVS"));
    assert_eq!(ctx.token.invoice_id(), ctx.invoice_id);
}

#[test]
fn test_integration_partial_burn_state_persistence() {
    let env = Env::default();
    let ctx = setup(&env);

    ctx.escrow
        .mint_on_fund(&ctx.token_id, &ctx.investor, &1_000);
    ctx.escrow
        .burn_on_settlement(&ctx.token_id, &ctx.investor, &300);

    // Intermediate state persists across separate reads.
    assert_eq!(ctx.token.balance(&ctx.investor), 700);
    assert_eq!(ctx.token.total_supply(), 700);

    ctx.escrow
        .burn_on_settlement(&ctx.token_id, &ctx.investor, &700);
    assert_eq!(ctx.token.balance(&ctx.investor), 0);
    assert_eq!(ctx.token.total_supply(), 0);
}
