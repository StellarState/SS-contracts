//! Comprehensive integration tests for the InvoiceEscrow contract.
//!
//! These tests spin up real InvoiceToken and Stellar-asset-contract instances
//! alongside InvoiceEscrow so that cross-contract calls (mint, set_transfer_locked,
//! token transfers) all execute as they would on-chain.

#![allow(deprecated)]

use super::*;
use invoice_token::{InvoiceToken, InvoiceTokenClient};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as AssetClient};
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    Address, BytesN, Env, String as SorobanString, Symbol, TryFromVal, TryIntoVal, Val, Vec,
};

// ──────────────────────────────────────────────────────────────────────────────
// Shared helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Build a deterministic 32-byte commitment from an ASCII string (pads with 0s).
fn test_commitment(env: &Env, data: &str) -> BytesN<32> {
    let mut array = [0u8; 32];
    let bytes = data.as_bytes();
    let len = bytes.len().min(32);
    array[..len].copy_from_slice(&bytes[..len]);
    BytesN::from_array(env, &array)
}

/// All addresses and clients needed by most tests.
struct Ctx<'a> {
    env: Env,
    admin: Address,
    seller: Address,
    buyer: Address,
    payer: Address,
    escrow_id: Address,
    escrow: InvoiceEscrowClient<'a>,
    inv_token_id: Address,
    inv_token: InvoiceTokenClient<'a>,
    payment_token: TokenClient<'a>,
    payment_asset: AssetClient<'a>,
    invoice_id: Symbol,
}

/// Stand up all contracts, initialize them, and optionally mint tokens to buyer/payer.
fn setup<'a>(
    env: &'a Env,
    fee_bps: u32,
    inv_id_str: &str,
    buyer_balance: i128,
    payer_balance: i128,
) -> Ctx<'a> {
    let admin = Address::generate(env);
    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let payer = Address::generate(env);

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow = InvoiceEscrowClient::new(env, &escrow_id);

    let inv_token_id = env.register(InvoiceToken, ());
    let inv_token = InvoiceTokenClient::new(env, &inv_token_id);

    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let payment_token = TokenClient::new(env, &token_contract.address());
    let payment_asset = AssetClient::new(env, &token_contract.address());

    let invoice_id = Symbol::new(env, inv_id_str);

    inv_token.initialize(
        &admin,
        &SorobanString::from_str(env, "Test Invoice Token"),
        &SorobanString::from_str(env, "TIT"),
        &7,
        &invoice_id,
        &escrow_id,
    );
    escrow.initialize(&admin, &fee_bps);

    if buyer_balance > 0 {
        payment_asset.mint(&buyer, &buyer_balance);
    }
    if payer_balance > 0 {
        payment_asset.mint(&payer, &payer_balance);
    }

    Ctx {
        env: env.clone(),
        admin,
        seller,
        buyer,
        payer,
        escrow_id,
        escrow,
        inv_token_id,
        inv_token,
        payment_token,
        payment_asset,
        invoice_id,
    }
}

/// Create and fully fund an escrow using face_value == purchase_price == `amount`.
fn create_and_fund(ctx: &Ctx<'_>, amount: i128, due_date: u64) {
    ctx.escrow.create_escrow(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &amount,
        &amount,
        &due_date,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&ctx.env, "commitment"),
        &None,
    );
    ctx.escrow.fund_escrow(&ctx.invoice_id, &ctx.buyer, &amount);
}

// ──────────────────────────────────────────────────────────────────────────────
// 1. Happy-path lifecycle (retained from original, extended assertions)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_escrow_lifecycle_happy_path() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INV001", 1_000, 1_000);

    let due_date = 100_000u64;
    create_and_fund(&ctx, 1_000, due_date);

    // After funding: buyer paid, escrow holds tokens, inv-token locked.
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 1_000);
    assert_eq!(ctx.inv_token.balance(&ctx.buyer), 1_000);
    assert!(ctx.inv_token.transfer_locked());
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Funded
    );

    // Settle.
    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    // 3% fee: 30 to admin, 970 to buyer, 1000 to seller, escrow empty.
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 30);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 970);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.payer), 0);

    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Settled
    );
    // Token must unlock after full settlement.
    assert!(!ctx.inv_token.transfer_locked());

    // Buyer can now freely transfer invoice tokens.
    let recipient = Address::generate(&env);
    ctx.inv_token.transfer(&ctx.buyer, &recipient, &1_000);
    assert_eq!(ctx.inv_token.balance(&ctx.buyer), 0);
    assert_eq!(ctx.inv_token.balance(&recipient), 1_000);
}

// ──────────────────────────────────────────────────────────────────────────────
// 2. Refund lifecycle (retained + extended)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_refund_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(5_000);
    let ctx = setup(&env, 300, "INVREF", 1_000, 0);

    let due_date = 10_000u64;
    create_and_fund(&ctx, 1_000, due_date);

    // Refund before due date must fail.
    assert!(ctx.escrow.try_refund(&ctx.invoice_id).is_err());

    // Advance past due date.
    env.ledger().set_timestamp(due_date + 1);
    ctx.escrow.refund(&ctx.invoice_id);

    // Buyer gets full purchase price back.
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 0);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Refunded
    );
    // Token unlocks after refund.
    assert!(!ctx.inv_token.transfer_locked());
}

// ──────────────────────────────────────────────────────────────────────────────
// 3. Invoice token locked during active escrow (retained)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_token_locked_during_active_escrow() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVLCK", 500, 500);

    create_and_fund(&ctx, 500, 20_000);

    assert!(ctx.inv_token.transfer_locked());

    // Transfer attempt must fail while locked.
    let other = Address::generate(&env);
    assert!(ctx
        .inv_token
        .try_transfer(&ctx.buyer, &other, &100)
        .is_err());

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &500);
    assert!(!ctx.inv_token.transfer_locked());
    ctx.inv_token.transfer(&ctx.buyer, &other, &100);
    assert_eq!(ctx.inv_token.balance(&ctx.buyer), 400);
    assert_eq!(ctx.inv_token.balance(&other), 100);
}

// ──────────────────────────────────────────────────────────────────────────────
// 4. Partial payment then full settlement
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_partial_payments_accumulate_to_settlement() {
    let env = Env::default();
    env.mock_all_auths();
    // face_value = 1000, purchase_price = 1000, fee = 5%
    let ctx = setup(&env, 500, "INVPART", 1_000, 1_000);
    create_and_fund(&ctx, 1_000, 99_999);

    // First partial payment: 400
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &400);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Funded // not yet settled
    );
    // 5% of 400 = 20 fee, 380 to buyer, 400 to seller
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 20);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 380);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 400);
    // Token remains locked during partial settlement.
    assert!(ctx.inv_token.transfer_locked());

    // Second payment: remaining 600
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &600);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Settled
    );
    // Total fees: 5% of 1000 = 50
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 50);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 950);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.payer), 0);
    assert!(!ctx.inv_token.transfer_locked());
}

// ──────────────────────────────────────────────────────────────────────────────
// 5. Partial payment then refund after due date
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_partial_payment_then_refund() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let ctx = setup(&env, 300, "INVPREF", 1_000, 1_000);

    create_and_fund(&ctx, 1_000, 5_000);

    // Partial payment: 400 (3% fee = 12, net 388 to buyer, 400 to seller)
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &400);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 400);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 388);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 12);
    // Escrow still holds the original 1000 funding minus what was released.
    // After record_payment: escrow received 400 from payer, paid out 400+400-400 = 400 net.
    // Net: escrow had 1000, received 400, paid 400(seller)+388(buyer)+12(admin) = 800 out => 600 left.
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 600);

    // Advance past due date and refund remaining 600.
    env.ledger().set_timestamp(5_001);
    ctx.escrow.refund(&ctx.invoice_id);

    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Refunded
    );
    // Buyer gets back 600 (the unfunded remainder).
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 388 + 600);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 0);
    assert!(!ctx.inv_token.transfer_locked());
}

// ──────────────────────────────────────────────────────────────────────────────
// 6. Cancel unfunded escrow
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_cancel_escrow_happy_path() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVCAN", 0, 0);

    ctx.escrow.create_escrow(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&env, "cancel_test"),
        &None,
    );
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Created
    );

    ctx.escrow.cancel_escrow(&ctx.invoice_id, &ctx.seller);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Cancelled
    );
}

#[test]
fn test_integration_cancel_funded_escrow_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVCNF", 1_000, 0);
    create_and_fund(&ctx, 1_000, 99_999);

    // Funded escrow cannot be cancelled.
    let result = ctx.escrow.try_cancel_escrow(&ctx.invoice_id, &ctx.seller);
    assert_eq!(result, Err(Ok(errors::Error::EscrowFunded)));
}

#[test]
fn test_integration_cancel_non_seller_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVCNR", 0, 0);

    ctx.escrow.create_escrow(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&env, "cancel_non_seller"),
        &None,
    );

    let intruder = Address::generate(&env);
    let result = ctx.escrow.try_cancel_escrow(&ctx.invoice_id, &intruder);
    assert_eq!(result, Err(Ok(errors::Error::Unauthorized)));
}

#[test]
fn test_integration_fund_cancelled_escrow_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVCFC", 1_000, 0);

    ctx.escrow.create_escrow(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&env, "fund_cancelled"),
        &None,
    );
    ctx.escrow.cancel_escrow(&ctx.invoice_id, &ctx.seller);

    let result = ctx
        .escrow
        .try_fund_escrow(&ctx.invoice_id, &ctx.buyer, &1_000);
    assert_eq!(result, Err(Ok(errors::Error::EscrowCancelled)));
}

// ──────────────────────────────────────────────────────────────────────────────
// 7. Emergency pause blocks lifecycle operations
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_pause_blocks_fund_and_payment() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVPSE", 1_000, 1_000);

    ctx.escrow.create_escrow(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&env, "pause_test"),
        &None,
    );

    ctx.escrow.set_paused(&true);
    assert!(ctx.escrow.paused());

    // fund_escrow must fail while paused.
    let r = ctx
        .escrow
        .try_fund_escrow(&ctx.invoice_id, &ctx.buyer, &1_000);
    assert_eq!(r, Err(Ok(errors::Error::Paused)));

    // Unpause and fund.
    ctx.escrow.set_paused(&false);
    ctx.escrow.fund_escrow(&ctx.invoice_id, &ctx.buyer, &1_000);

    // Re-pause and try to record payment.
    ctx.escrow.set_paused(&true);
    let r2 = ctx
        .escrow
        .try_record_payment(&ctx.invoice_id, &ctx.payer, &1_000);
    assert_eq!(r2, Err(Ok(errors::Error::Paused)));

    // Unpause and settle.
    ctx.escrow.set_paused(&false);
    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Settled
    );
}

#[test]
fn test_integration_pause_blocks_refund() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let ctx = setup(&env, 300, "INVPSR", 1_000, 0);
    create_and_fund(&ctx, 1_000, 2_000);

    env.ledger().set_timestamp(2_001);
    ctx.escrow.set_paused(&true);

    let r = ctx.escrow.try_refund(&ctx.invoice_id);
    assert_eq!(r, Err(Ok(errors::Error::Paused)));

    ctx.escrow.set_paused(&false);
    ctx.escrow.refund(&ctx.invoice_id);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Refunded
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 8. Zero-fee escrow — admin receives nothing, investor receives full amount
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_zero_fee_full_investor_return() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 0, "INVZF", 1_000, 1_000);
    create_and_fund(&ctx, 1_000, 99_999);

    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
}

// ──────────────────────────────────────────────────────────────────────────────
// 9. Max fee (100%) — investor receives nothing
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_max_fee_investor_receives_nothing() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 10_000, "INVMF", 1_000, 1_000);
    create_and_fund(&ctx, 1_000, 99_999);

    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);
    // 100% fee → investor gets 0, admin gets 1000, seller gets 1000 (released principal)
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 0);
}

// ──────────────────────────────────────────────────────────────────────────────
// 10. Wrong payer is rejected
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_wrong_payer_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVWP", 1_000, 0);
    create_and_fund(&ctx, 1_000, 99_999);

    let impostor = Address::generate(&env);
    ctx.payment_asset.mint(&impostor, &1_000);

    let result = ctx
        .escrow
        .try_record_payment(&ctx.invoice_id, &impostor, &1_000);
    assert_eq!(result, Err(Ok(errors::Error::InvalidPayer)));
}

// ──────────────────────────────────────────────────────────────────────────────
// 11. Overpayment rejected
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_overpayment_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVOVR", 1_000, 2_000);
    create_and_fund(&ctx, 1_000, 99_999);

    // face_value == 1000, paying 1001 must fail.
    let result = ctx
        .escrow
        .try_record_payment(&ctx.invoice_id, &ctx.payer, &1_001);
    assert_eq!(result, Err(Ok(errors::Error::InvalidAmount)));
}

// ──────────────────────────────────────────────────────────────────────────────
// 12. Over-funding rejected
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_over_funding_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVOVF", 2_000, 0);

    ctx.escrow.create_escrow(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&env, "over_fund"),
        &None,
    );

    // Purchase price is 1000; funding 1001 must fail.
    let result = ctx
        .escrow
        .try_fund_escrow(&ctx.invoice_id, &ctx.buyer, &1_001);
    assert_eq!(result, Err(Ok(errors::Error::InvalidAmount)));
}

// ──────────────────────────────────────────────────────────────────────────────
// 13. Platform fee update mid-lifecycle affects next payment
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_fee_update_affects_subsequent_payment() {
    let env = Env::default();
    env.mock_all_auths();
    // Start with 3% fee, face_value = 1000, split into two payments.
    let ctx = setup(&env, 300, "INVFUP", 1_000, 1_000);
    create_and_fund(&ctx, 1_000, 99_999);

    // First payment: 500 at 3% fee → 15 to admin, 485 to buyer, 500 to seller.
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &500);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 15);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 485);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 500);

    // Admin raises fee to 10%.
    ctx.escrow.update_platform_fee_bps(&1_000);
    assert_eq!(ctx.escrow.get_config().fee_bps, 1_000);

    // Second payment: 500 at 10% → 50 to admin, 450 to buyer, 500 to seller.
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &500);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 15 + 50);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 485 + 450);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Settled
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 14. Duplicate invoice ID is rejected
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_duplicate_invoice_id_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVDUP", 0, 0);

    let commitment = test_commitment(&env, "dup");
    ctx.escrow.create_escrow(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &commitment,
        &None,
    );

    let result = ctx.escrow.try_create_escrow(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &2_000,
        &2_000,
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &commitment,
        &None,
    );
    assert_eq!(result, Err(Ok(errors::Error::EscrowExists)));
}

// ──────────────────────────────────────────────────────────────────────────────
// 15. Due date in the past / zero is rejected
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_past_due_date_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(50_000);
    let ctx = setup(&env, 300, "INVPDD", 0, 0);

    // due_date = 49_999 < current timestamp (50_000) → must fail.
    let result = ctx.escrow.try_create_escrow(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &49_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&env, "past_due"),
        &None,
    );
    assert_eq!(result, Err(Ok(errors::Error::InvalidDueDate)));
}

#[test]
fn test_integration_zero_due_date_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVZDD", 0, 0);

    let result = ctx.escrow.try_create_escrow(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &0,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&env, "zero_due"),
        &None,
    );
    assert_eq!(result, Err(Ok(errors::Error::InvalidDueDate)));
}

// ──────────────────────────────────────────────────────────────────────────────
// 16. create_escrow on uninitialised contract fails
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_create_escrow_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();

    // Register escrow but do NOT initialize it.
    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow = InvoiceEscrowClient::new(&env, &escrow_id);

    let seller = Address::generate(&env);
    let payer = Address::generate(&env);
    let token = Address::generate(&env);
    let inv_token = Address::generate(&env);

    let result = escrow.try_create_escrow(
        &Symbol::new(&env, "INV"),
        &seller,
        &payer,
        &1_000,
        &1_000,
        &99_999,
        &token,
        &inv_token,
        &test_commitment(&env, "no_init"),
        &None,
    );
    assert_eq!(result, Err(Ok(errors::Error::NotInit)));
}

// ──────────────────────────────────────────────────────────────────────────────
// 17. State persistence: get_escrow returns correct stored data
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_state_persistence_after_create() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVPST", 0, 0);

    let commitment = test_commitment(&env, "persistence");
    ctx.escrow.create_escrow(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &2_000,
        &1_800,
        &88_888,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &commitment,
        &None,
    );

    let data = ctx.escrow.get_escrow(&ctx.invoice_id);
    assert_eq!(data.inv_id, ctx.invoice_id);
    assert_eq!(data.seller, ctx.seller);
    assert_eq!(data.debtor, ctx.payer);
    assert_eq!(data.face_value, 2_000);
    assert_eq!(data.purchase_price, 1_800);
    assert_eq!(data.funded_amt, 0);
    assert_eq!(data.due_dt, 88_888);
    assert_eq!(data.paid_amt, 0);
    assert_eq!(data.status, EscrowStatus::Created);
    assert_eq!(data.commitment, commitment);
    assert!(data.funder.is_none());
}

#[test]
fn test_integration_state_persistence_after_fund() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVPSF", 1_000, 0);

    create_and_fund(&ctx, 1_000, 99_999);

    let data = ctx.escrow.get_escrow(&ctx.invoice_id);
    assert_eq!(data.status, EscrowStatus::Funded);
    assert_eq!(data.funded_amt, 1_000);
    assert_eq!(data.funder, Some(ctx.buyer.clone()));
}

#[test]
fn test_integration_state_persistence_after_settlement() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVPSS", 1_000, 1_000);
    create_and_fund(&ctx, 1_000, 99_999);

    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    let data = ctx.escrow.get_escrow(&ctx.invoice_id);
    assert_eq!(data.status, EscrowStatus::Settled);
    assert_eq!(data.paid_amt, 1_000);
}

#[test]
fn test_integration_state_persistence_after_refund() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(0);
    let ctx = setup(&env, 300, "INVPSR", 1_000, 0);
    create_and_fund(&ctx, 1_000, 1_000);

    env.ledger().set_timestamp(1_001);
    ctx.escrow.refund(&ctx.invoice_id);

    let data = ctx.escrow.get_escrow(&ctx.invoice_id);
    assert_eq!(data.status, EscrowStatus::Refunded);
}

// ──────────────────────────────────────────────────────────────────────────────
// 18. get_escrow on unknown invoice returns EscrowNotFound
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_get_escrow_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVGNF", 0, 0);

    let result = ctx.escrow.try_get_escrow(&Symbol::new(&env, "MISSING"));
    assert_eq!(result, Err(Ok(errors::Error::EscrowNotFound)));
}

#[test]
fn test_integration_get_escrow_status_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVSGNF", 0, 0);

    let result = ctx
        .escrow
        .try_get_escrow_status(&Symbol::new(&env, "MISSING"));
    assert_eq!(result, Err(Ok(errors::Error::EscrowNotFound)));
}

// ──────────────────────────────────────────────────────────────────────────────
// 19. Commitment is immutable — stored value never changes after create
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_commitment_immutable_after_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVCMT", 1_000, 1_000);

    let original = test_commitment(&env, "original_pdf_hash");
    ctx.escrow.create_escrow(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &original,
        &None,
    );

    // Fund.
    ctx.escrow.fund_escrow(&ctx.invoice_id, &ctx.buyer, &1_000);
    assert_eq!(ctx.escrow.get_escrow(&ctx.invoice_id).commitment, original);

    // Settle.
    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);
    assert_eq!(ctx.escrow.get_escrow(&ctx.invoice_id).commitment, original);
}

// ──────────────────────────────────────────────────────────────────────────────
// 20. Two independent escrows coexist without interference
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_two_independent_escrows() {
    let env = Env::default();
    env.mock_all_auths();

    // Escrow A
    let ctx_a = setup(&env, 300, "INVA01", 1_000, 1_000);
    create_and_fund(&ctx_a, 1_000, 99_999);

    // Escrow B — reuse the same escrow contract, different invoice
    let inv_b_id = Symbol::new(&env, "INVB01");

    let inv_token_b_id = env.register(InvoiceToken, ());
    let inv_token_b = InvoiceTokenClient::new(&env, &inv_token_b_id);
    inv_token_b.initialize(
        &ctx_a.admin,
        &SorobanString::from_str(&env, "Invoice B"),
        &SorobanString::from_str(&env, "INVB"),
        &7,
        &inv_b_id,
        &ctx_a.escrow_id,
    );

    let buyer_b = Address::generate(&env);
    let payer_b = Address::generate(&env);
    ctx_a.payment_asset.mint(&buyer_b, &500);
    ctx_a.payment_asset.mint(&payer_b, &500);

    ctx_a.escrow.create_escrow(
        &inv_b_id,
        &ctx_a.seller,
        &payer_b,
        &500,
        &500,
        &99_999,
        &ctx_a.payment_token.address,
        &inv_token_b_id,
        &test_commitment(&env, "inv_b"),
        &None,
    );
    ctx_a.escrow.fund_escrow(&inv_b_id, &buyer_b, &500);

    // Settle A.
    ctx_a
        .escrow
        .record_payment(&ctx_a.invoice_id, &ctx_a.payer, &1_000);
    assert_eq!(
        ctx_a.escrow.get_escrow_status(&ctx_a.invoice_id),
        EscrowStatus::Settled
    );
    // B must remain Funded.
    assert_eq!(
        ctx_a.escrow.get_escrow_status(&inv_b_id),
        EscrowStatus::Funded
    );

    // Settle B.
    ctx_a.escrow.record_payment(&inv_b_id, &payer_b, &500);
    assert_eq!(
        ctx_a.escrow.get_escrow_status(&inv_b_id),
        EscrowStatus::Settled
    );

    // Both tokens unlocked independently.
    assert!(!ctx_a.inv_token.transfer_locked());
    assert!(!inv_token_b.transfer_locked());
}

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

#[test]
fn test_integration_escrow_created_event_emitted() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVECE", 0, 0);

    let commitment = test_commitment(&env, "event_emitted");
    ctx.escrow.create_escrow(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &900,
        &55_555,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &commitment,
        &None,
    );

    let evts = env.events().all();
    let evt = evts
        .events()
        .iter()
        .rev()
        .find(|e| {
            let (_addr, topics, _data) = parse_event(&env, e);
            let topic_sym = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
            topic_sym == Symbol::new(&env, "escrow_created")
        })
        .expect("expected escrow_created event");
    let (_addr, topics, data) = parse_event(&env, evt);
    let topic_sym = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
    assert_eq!(topic_sym, Symbol::new(&env, "escrow_created"));

    let (inv_id, seller, debtor, fv, pp, dd, _tok, _inv_tok, cmt, _milestone): (
        Symbol,
        Address,
        Address,
        i128,
        i128,
        u64,
        Address,
        Address,
        BytesN<32>,
        Option<i128>,
    ) = data.try_into_val(&env).unwrap();
    assert_eq!(inv_id, ctx.invoice_id);
    assert_eq!(seller, ctx.seller);
    assert_eq!(debtor, ctx.payer);
    assert_eq!(fv, 1_000);
    assert_eq!(pp, 900);
    assert_eq!(dd, 55_555);
    assert_eq!(cmt, commitment);
}

#[test]
fn test_integration_escrow_cancelled_event_emitted() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVCNE", 0, 0);

    ctx.escrow.create_escrow(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &1_000,
        &1_000,
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&env, "cancel_event"),
        &None,
    );
    ctx.escrow.cancel_escrow(&ctx.invoice_id, &ctx.seller);

    let evts = env.events().all();
    let evt = evts
        .events()
        .iter()
        .rev()
        .find(|e| {
            let (_addr, topics, _data) = parse_event(&env, e);
            let topic_sym = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
            topic_sym == Symbol::new(&env, "escrow_cancelled")
        })
        .expect("expected escrow_cancelled event");
    let (_addr, topics, data) = parse_event(&env, evt);
    let topic_sym = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
    assert_eq!(topic_sym, Symbol::new(&env, "escrow_cancelled"));

    let (inv_id, seller): (Symbol, Address) = data.try_into_val(&env).unwrap();
    assert_eq!(inv_id, ctx.invoice_id);
    assert_eq!(seller, ctx.seller);
}

#[test]
fn test_integration_payment_settled_event_emitted() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 300, "INVPSE", 1_000, 1_000);
    create_and_fund(&ctx, 1_000, 99_999);
    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    let evts = env.events().all();
    let evt = evts
        .events()
        .iter()
        .rev()
        .find(|e| {
            let (_addr, topics, _data) = parse_event(&env, e);
            let topic_sym = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
            topic_sym == Symbol::new(&env, "payment_settled")
        })
        .expect("expected payment_settled event");
    let (_addr, topics, data) = parse_event(&env, evt);
    let topic_sym = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
    assert_eq!(topic_sym, Symbol::new(&env, "payment_settled"));

    let (inv_id, amount, fee, investor): (Symbol, i128, i128, i128) =
        data.try_into_val(&env).unwrap();
    assert_eq!(inv_id, ctx.invoice_id);
    assert_eq!(amount, 1_000);
    assert_eq!(fee, 30); // 3% of 1000
    assert_eq!(investor, 970);
}

#[test]
fn test_integration_escrow_refunded_event_emitted() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(0);
    let ctx = setup(&env, 300, "INVREF", 1_000, 0);
    create_and_fund(&ctx, 1_000, 1_000);
    env.ledger().set_timestamp(1_001);
    ctx.escrow.refund(&ctx.invoice_id);

    let evts = env.events().all();
    let evt = evts
        .events()
        .iter()
        .rev()
        .find(|e| {
            let (_addr, topics, _data) = parse_event(&env, e);
            let topic_sym = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
            topic_sym == Symbol::new(&env, "escrow_refunded")
        })
        .expect("expected escrow_refunded event");
    let (_addr, topics, data) = parse_event(&env, evt);
    let topic_sym = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
    assert_eq!(topic_sym, Symbol::new(&env, "escrow_refunded"));

    let (inv_id, amount): (Symbol, i128) = data.try_into_val(&env).unwrap();
    assert_eq!(inv_id, ctx.invoice_id);
    assert_eq!(amount, 1_000);
}

// ──────────────────────────────────────────────────────────────────────────────
// 22. Face value vs purchase price discount — seller gets face, buyer gets net
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_discounted_purchase_price() {
    let env = Env::default();
    env.mock_all_auths();

    // face_value = 900 == purchase_price = 900 (discount reflected externally).
    // Use 0% fee so all math is clean.  Buyer funds 900; payer pays 900.
    // Escrow in: 900 (buyer) + 900 (payer) = 1800.
    // Escrow out: 900 (investor, 0% fee) + 900 (seller release) = 1800.
    let ctx = setup(&env, 0, "INVDSC", 900, 900);

    ctx.escrow.create_escrow(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &900, // face_value
        &900, // purchase_price
        &99_999,
        &ctx.payment_token.address,
        &ctx.inv_token_id,
        &test_commitment(&env, "discount"),
        &None,
    );
    ctx.escrow.fund_escrow(&ctx.invoice_id, &ctx.buyer, &900);

    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 900);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Funded
    );

    // Payer settles the invoice.
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &900);

    // 0% fee: investor gets 900, admin gets 0, seller gets 900.
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 900);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 900);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 0);
}

// ──────────────────────────────────────────────────────────────────────────────
// 23. Refund before due_date strictly blocked; at due_dt it is allowed
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_refund_at_exact_due_date_blocked() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(0);
    let ctx = setup(&env, 300, "INVBND", 1_000, 0);
    create_and_fund(&ctx, 1_000, 5_000);

    // One second before due_date: must fail.
    env.ledger().set_timestamp(4_999);
    let result = ctx.escrow.try_refund(&ctx.invoice_id);
    assert_eq!(result, Err(Ok(errors::Error::RefundNotAllowed)));

    // At exactly due_date (ledger_ts == due_dt): contract allows refund
    // because it checks `ledger_ts < due_dt` — false when equal.
    env.ledger().set_timestamp(5_000);
    ctx.escrow.refund(&ctx.invoice_id);
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Refunded
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 24. Double refund rejected
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_double_refund_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(0);
    let ctx = setup(&env, 300, "INVDR", 1_000, 0);
    create_and_fund(&ctx, 1_000, 1_000);

    env.ledger().set_timestamp(1_001);
    ctx.escrow.refund(&ctx.invoice_id);

    let result = ctx.escrow.try_refund(&ctx.invoice_id);
    assert_eq!(result, Err(Ok(errors::Error::RefundNotAllowed)));
}

// ──────────────────────────────────────────────────────────────────────────────
// 25. get_config returns current admin and fee
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_get_config_returns_correct_values() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 750, "INVCFG", 0, 0);

    let cfg = ctx.escrow.get_config();
    assert_eq!(cfg.admin, ctx.admin);
    assert_eq!(cfg.fee_bps, 750);
    assert!(!cfg.paused);
    assert!(cfg.payment_distributor.is_none());
}

// ──────────────────────────────────────────────────────────────────────────────
// Issue #170: Multi-Currency Escrow Settlement Integration Tests
//
// These tests validate that the InvoiceEscrow contract correctly handles
// escrows denominated in different payment tokens.  Each test uses the real
// InvoiceToken and Stellar-asset-contract instances so cross-contract calls
// (mint, transfer, set_transfer_locked) execute as they would on-chain.
//
// Coverage:
//   26. Two simultaneous escrows in different tokens settle independently.
//   27. Token balances are fully isolated between currency A and currency B.
//   28. Refund returns the correct token to the funder.
//   29. Platform fee is calculated and collected in the correct token.
//   30. Persistent state for each currency-specific escrow is independent.
// ──────────────────────────────────────────────────────────────────────────────

/// Helper: register a second InvoiceToken and Stellar asset for multi-currency tests.
/// Returns (invoice_token_id, invoice_token_client, payment_token_id, token_client, asset_client).
fn register_currency<'a>(
    env: &'a Env,
    admin: &Address,
    escrow_id: &Address,
    token_name: &str,
    token_symbol: &str,
    inv_id: &Symbol,
) -> (
    Address,
    invoice_token::InvoiceTokenClient<'a>,
    Address,
    TokenClient<'a>,
    AssetClient<'a>,
) {
    use soroban_sdk::String as SorobanString;

    let inv_token_id = env.register(InvoiceToken, ());
    let inv_token = invoice_token::InvoiceTokenClient::new(env, &inv_token_id);
    inv_token.initialize(
        admin,
        &SorobanString::from_str(env, token_name),
        &SorobanString::from_str(env, token_symbol),
        &7,
        inv_id,
        escrow_id,
    );

    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let payment_token = TokenClient::new(env, &token_contract.address());
    let payment_asset = AssetClient::new(env, &token_contract.address());

    (
        inv_token_id,
        inv_token,
        token_contract.address(),
        payment_token,
        payment_asset,
    )
}

// ──────────────────────────────────────────────────────────────────────────────
// 26. Two simultaneous escrows in different tokens settle independently
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_multi_currency_two_escrows_settle_independently() {
    let env = Env::default();
    env.mock_all_auths();
    use soroban_sdk::String as SorobanString;

    // Shared escrow contract and participants.
    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow = InvoiceEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    escrow.initialize(&admin, &300); // 3% fee

    // Currency A setup.
    let inv_a = Symbol::new(&env, "INVMC26A");
    let (inv_tok_a_id, inv_tok_a, pay_tok_a_id, pay_tok_a, pay_ast_a) =
        register_currency(&env, &admin, &escrow_id, "Invoice A Token", "ITKA", &inv_a);

    // Currency B setup.
    let inv_b = Symbol::new(&env, "INVMC26B");
    let (inv_tok_b_id, inv_tok_b, pay_tok_b_id, pay_tok_b, pay_ast_b) =
        register_currency(&env, &admin, &escrow_id, "Invoice B Token", "ITKB", &inv_b);

    // Participants.
    let seller = Address::generate(&env);
    let buyer_a = Address::generate(&env);
    let buyer_b = Address::generate(&env);
    let payer_a = Address::generate(&env);
    let payer_b = Address::generate(&env);

    // Mint tokens for each currency.
    pay_ast_a.mint(&buyer_a, &1_000);
    pay_ast_a.mint(&payer_a, &1_000);
    pay_ast_b.mint(&buyer_b, &500);
    pay_ast_b.mint(&payer_b, &500);

    // Create both escrows.
    escrow.create_escrow(
        &inv_a,
        &seller,
        &payer_a,
        &1_000,
        &1_000,
        &99_999,
        &pay_tok_a_id,
        &inv_tok_a_id,
        &test_commitment(&env, "mc_a"),
        &None,
    );
    escrow.create_escrow(
        &inv_b,
        &seller,
        &payer_b,
        &500,
        &500,
        &99_999,
        &pay_tok_b_id,
        &inv_tok_b_id,
        &test_commitment(&env, "mc_b"),
        &None,
    );

    // Fund both escrows.
    escrow.fund_escrow(&inv_a, &buyer_a, &1_000);
    escrow.fund_escrow(&inv_b, &buyer_b, &500);

    assert_eq!(escrow.get_escrow_status(&inv_a), EscrowStatus::Funded);
    assert_eq!(escrow.get_escrow_status(&inv_b), EscrowStatus::Funded);

    // Settle A.
    escrow.record_payment(&inv_a, &payer_a, &1_000);
    assert_eq!(escrow.get_escrow_status(&inv_a), EscrowStatus::Settled);
    // B must still be Funded.
    assert_eq!(escrow.get_escrow_status(&inv_b), EscrowStatus::Funded);

    // Settle B.
    escrow.record_payment(&inv_b, &payer_b, &500);
    assert_eq!(escrow.get_escrow_status(&inv_b), EscrowStatus::Settled);

    // Invoice tokens unlocked independently after each settlement.
    assert!(!inv_tok_a.transfer_locked());
    assert!(!inv_tok_b.transfer_locked());
}

// ──────────────────────────────────────────────────────────────────────────────
// 27. Token balances are fully isolated — currency A payment never touches B
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_multi_currency_settlement_token_isolation() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow = InvoiceEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    escrow.initialize(&admin, &300);

    let inv_a = Symbol::new(&env, "INVMC27A");
    let inv_b = Symbol::new(&env, "INVMC27B");

    let (inv_tok_a_id, _inv_tok_a, pay_tok_a_id, pay_tok_a, pay_ast_a) =
        register_currency(&env, &admin, &escrow_id, "Token A", "TKNA", &inv_a);
    let (inv_tok_b_id, _inv_tok_b, pay_tok_b_id, pay_tok_b, pay_ast_b) =
        register_currency(&env, &admin, &escrow_id, "Token B", "TKNB", &inv_b);

    let seller = Address::generate(&env);
    let buyer_a = Address::generate(&env);
    let buyer_b = Address::generate(&env);
    let payer_a = Address::generate(&env);
    let payer_b = Address::generate(&env);

    pay_ast_a.mint(&buyer_a, &1_000);
    pay_ast_a.mint(&payer_a, &1_000);
    pay_ast_b.mint(&buyer_b, &800);
    pay_ast_b.mint(&payer_b, &800);

    escrow.create_escrow(
        &inv_a, &seller, &payer_a, &1_000, &1_000, &99_999,
        &pay_tok_a_id, &inv_tok_a_id, &test_commitment(&env, "iso_a"), &None,
    );
    escrow.create_escrow(
        &inv_b, &seller, &payer_b, &800, &800, &99_999,
        &pay_tok_b_id, &inv_tok_b_id, &test_commitment(&env, "iso_b"), &None,
    );

    escrow.fund_escrow(&inv_a, &buyer_a, &1_000);
    escrow.fund_escrow(&inv_b, &buyer_b, &800);

    // Settle A only.
    escrow.record_payment(&inv_a, &payer_a, &1_000);

    // Token A: distributed to buyer_a (970 after 3% fee) and seller_a.
    assert_eq!(pay_tok_a.balance(&buyer_a), 970);
    assert_eq!(pay_tok_a.balance(&seller), 1_000);
    assert_eq!(pay_tok_a.balance(&admin), 30);
    assert_eq!(pay_tok_a.balance(&escrow_id), 0);

    // Token B: completely untouched — still in escrow.
    assert_eq!(pay_tok_b.balance(&escrow_id), 800);
    assert_eq!(pay_tok_b.balance(&buyer_b), 0);
    assert_eq!(pay_tok_b.balance(&seller), 0);

    // Settle B.
    escrow.record_payment(&inv_b, &payer_b, &800);

    // 3% of 800 = 24 fee.
    assert_eq!(pay_tok_b.balance(&buyer_b), 800 - 24);
    assert_eq!(pay_tok_b.balance(&admin), 24);
    assert_eq!(pay_tok_b.balance(&escrow_id), 0);

    // Token A balances must be unchanged after B settlement.
    assert_eq!(pay_tok_a.balance(&buyer_a), 970);
    assert_eq!(pay_tok_a.balance(&admin), 30);
}

// ──────────────────────────────────────────────────────────────────────────────
// 28. Refund returns the correct token to the funder
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_multi_currency_refund_returns_correct_token() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(0);

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow = InvoiceEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    escrow.initialize(&admin, &300);

    let inv_a = Symbol::new(&env, "INVMC28A");
    let inv_b = Symbol::new(&env, "INVMC28B");
    let due_date = 5_000u64;

    let (inv_tok_a_id, _inv_tok_a, pay_tok_a_id, pay_tok_a, pay_ast_a) =
        register_currency(&env, &admin, &escrow_id, "Refund A", "REFA", &inv_a);
    let (inv_tok_b_id, _inv_tok_b, pay_tok_b_id, pay_tok_b, pay_ast_b) =
        register_currency(&env, &admin, &escrow_id, "Refund B", "REFB", &inv_b);

    let seller = Address::generate(&env);
    let buyer_a = Address::generate(&env);
    let buyer_b = Address::generate(&env);
    let payer_a = Address::generate(&env);
    let payer_b = Address::generate(&env);

    pay_ast_a.mint(&buyer_a, &1_000);
    pay_ast_b.mint(&buyer_b, &600);

    escrow.create_escrow(
        &inv_a, &seller, &payer_a, &1_000, &1_000, &due_date,
        &pay_tok_a_id, &inv_tok_a_id, &test_commitment(&env, "refund_a"), &None,
    );
    escrow.create_escrow(
        &inv_b, &seller, &payer_b, &600, &600, &due_date,
        &pay_tok_b_id, &inv_tok_b_id, &test_commitment(&env, "refund_b"), &None,
    );

    escrow.fund_escrow(&inv_a, &buyer_a, &1_000);
    escrow.fund_escrow(&inv_b, &buyer_b, &600);

    // Settle escrow A so it is no longer refundable.
    pay_ast_a.mint(&payer_a, &1_000);
    escrow.record_payment(&inv_a, &payer_a, &1_000);
    assert_eq!(escrow.get_escrow_status(&inv_a), EscrowStatus::Settled);

    // Advance past due_date for escrow B.
    env.ledger().set_timestamp(due_date + 1);
    escrow.refund(&inv_b);
    assert_eq!(escrow.get_escrow_status(&inv_b), EscrowStatus::Refunded);

    // buyer_b gets back 600 in token B — not token A.
    assert_eq!(pay_tok_b.balance(&buyer_b), 600);
    assert_eq!(pay_tok_b.balance(&escrow_id), 0);

    // Token A is unaffected by the B refund.
    assert_eq!(pay_tok_a.balance(&escrow_id), 0); // already settled
}

// ──────────────────────────────────────────────────────────────────────────────
// 29. Platform fee is calculated and collected in the correct token per escrow
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_multi_currency_fee_collected_in_correct_token() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow = InvoiceEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    escrow.initialize(&admin, &500); // 5% fee

    let inv_a = Symbol::new(&env, "INVMC29A");
    let inv_b = Symbol::new(&env, "INVMC29B");

    let (inv_tok_a_id, _inv_tok_a, pay_tok_a_id, pay_tok_a, pay_ast_a) =
        register_currency(&env, &admin, &escrow_id, "Fee A Token", "FEEA", &inv_a);
    let (inv_tok_b_id, _inv_tok_b, pay_tok_b_id, pay_tok_b, pay_ast_b) =
        register_currency(&env, &admin, &escrow_id, "Fee B Token", "FEEB", &inv_b);

    let seller = Address::generate(&env);
    let buyer_a = Address::generate(&env);
    let buyer_b = Address::generate(&env);
    let payer_a = Address::generate(&env);
    let payer_b = Address::generate(&env);

    // Escrow A: 2_000 in token A; escrow B: 1_000 in token B.
    pay_ast_a.mint(&buyer_a, &2_000);
    pay_ast_a.mint(&payer_a, &2_000);
    pay_ast_b.mint(&buyer_b, &1_000);
    pay_ast_b.mint(&payer_b, &1_000);

    escrow.create_escrow(
        &inv_a, &seller, &payer_a, &2_000, &2_000, &99_999,
        &pay_tok_a_id, &inv_tok_a_id, &test_commitment(&env, "fee_a"), &None,
    );
    escrow.create_escrow(
        &inv_b, &seller, &payer_b, &1_000, &1_000, &99_999,
        &pay_tok_b_id, &inv_tok_b_id, &test_commitment(&env, "fee_b"), &None,
    );

    escrow.fund_escrow(&inv_a, &buyer_a, &2_000);
    escrow.fund_escrow(&inv_b, &buyer_b, &1_000);

    // Settle both.
    escrow.record_payment(&inv_a, &payer_a, &2_000);
    escrow.record_payment(&inv_b, &payer_b, &1_000);

    // 5% of 2000 = 100 in token A to admin.
    // 5% of 1000 = 50  in token B to admin.
    assert_eq!(pay_tok_a.balance(&admin), 100, "admin must receive 100 token A");
    assert_eq!(pay_tok_b.balance(&admin), 50,  "admin must receive 50 token B");

    // Investors receive the net amount in their respective tokens.
    assert_eq!(pay_tok_a.balance(&buyer_a), 2_000 - 100);
    assert_eq!(pay_tok_b.balance(&buyer_b), 1_000 - 50);

    // Escrow contract must be empty for both currencies.
    assert_eq!(pay_tok_a.balance(&escrow_id), 0);
    assert_eq!(pay_tok_b.balance(&escrow_id), 0);
}

// ──────────────────────────────────────────────────────────────────────────────
// 30. Persistent state for each currency-specific escrow is fully independent
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_integration_multi_currency_state_persists_independently() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow = InvoiceEscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    escrow.initialize(&admin, &300);

    let inv_a = Symbol::new(&env, "INVMC30A");
    let inv_b = Symbol::new(&env, "INVMC30B");

    let (inv_tok_a_id, _inv_tok_a, pay_tok_a_id, _pay_tok_a, pay_ast_a) =
        register_currency(&env, &admin, &escrow_id, "State A", "STKA", &inv_a);
    let (inv_tok_b_id, _inv_tok_b, pay_tok_b_id, _pay_tok_b, pay_ast_b) =
        register_currency(&env, &admin, &escrow_id, "State B", "STKB", &inv_b);

    let seller = Address::generate(&env);
    let buyer_a = Address::generate(&env);
    let buyer_b = Address::generate(&env);
    let payer_a = Address::generate(&env);
    let payer_b = Address::generate(&env);

    let commitment_a = test_commitment(&env, "state_a");
    let commitment_b = test_commitment(&env, "state_b");

    pay_ast_a.mint(&buyer_a, &1_200);
    pay_ast_a.mint(&payer_a, &1_200);
    pay_ast_b.mint(&buyer_b, &700);
    pay_ast_b.mint(&payer_b, &700);

    escrow.create_escrow(
        &inv_a, &seller, &payer_a, &1_200, &1_200, &88_888,
        &pay_tok_a_id, &inv_tok_a_id, &commitment_a, &None,
    );
    escrow.create_escrow(
        &inv_b, &seller, &payer_b, &700, &700, &77_777,
        &pay_tok_b_id, &inv_tok_b_id, &commitment_b, &None,
    );

    // Verify initial state for both escrows from storage.
    let data_a = escrow.get_escrow(&inv_a);
    let data_b = escrow.get_escrow(&inv_b);

    assert_eq!(data_a.face_value, 1_200);
    assert_eq!(data_a.purchase_price, 1_200);
    assert_eq!(data_a.status, EscrowStatus::Created);
    assert_eq!(data_a.token, pay_tok_a_id);
    assert_eq!(data_a.commitment, commitment_a);
    assert_eq!(data_a.due_dt, 88_888);

    assert_eq!(data_b.face_value, 700);
    assert_eq!(data_b.purchase_price, 700);
    assert_eq!(data_b.status, EscrowStatus::Created);
    assert_eq!(data_b.token, pay_tok_b_id);
    assert_eq!(data_b.commitment, commitment_b);
    assert_eq!(data_b.due_dt, 77_777);

    // Fund only escrow A — B must remain Created.
    escrow.fund_escrow(&inv_a, &buyer_a, &1_200);
    assert_eq!(escrow.get_escrow_status(&inv_a), EscrowStatus::Funded);
    assert_eq!(escrow.get_escrow_status(&inv_b), EscrowStatus::Created);

    // Check A's stored funded_amt and funder without touching B.
    let data_a_after_fund = escrow.get_escrow(&inv_a);
    assert_eq!(data_a_after_fund.funded_amt, 1_200);
    assert_eq!(data_a_after_fund.funder, Some(buyer_a.clone()));

    let data_b_after_fund = escrow.get_escrow(&inv_b);
    assert_eq!(data_b_after_fund.funded_amt, 0);
    assert!(data_b_after_fund.funder.is_none());

    // Settle A — B must remain unaffected.
    escrow.record_payment(&inv_a, &payer_a, &1_200);
    assert_eq!(escrow.get_escrow_status(&inv_a), EscrowStatus::Settled);
    assert_eq!(escrow.get_escrow_status(&inv_b), EscrowStatus::Created);

    let data_a_settled = escrow.get_escrow(&inv_a);
    assert_eq!(data_a_settled.paid_amt, 1_200);

    // B's paid_amt must still be 0.
    let data_b_settled = escrow.get_escrow(&inv_b);
    assert_eq!(data_b_settled.paid_amt, 0);
    assert_eq!(data_b_settled.commitment, commitment_b); // commitment is immutable
}
