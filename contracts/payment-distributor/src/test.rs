#![allow(deprecated)]

use super::*;
use invoice_escrow::{EscrowStatus, InvoiceEscrow, InvoiceEscrowClient};
use invoice_token::{InvoiceToken, InvoiceTokenClient};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as AssetClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, String as SorobanString, Symbol,
};

struct TestContext<'a> {
    env: Env,
    admin: Address,
    seller: Address,
    buyer: Address,
    payer: Address,
    escrow_id: Address,
    escrow: InvoiceEscrowClient<'a>,
    distributor_id: Address,
    distributor: PaymentDistributorClient<'a>,
    inv_token: InvoiceTokenClient<'a>,
    payment_token: TokenClient<'a>,
    payment_asset: AssetClient<'a>,
    invoice_id: Symbol,
}

fn setup(env: &Env, fee_bps: u32, configure_distributor: bool) -> TestContext<'_> {
    let admin = Address::generate(env);
    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let payer = Address::generate(env);

    let escrow_id = env.register(InvoiceEscrow, ());
    let escrow = InvoiceEscrowClient::new(env, &escrow_id);

    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(env, &distributor_id);

    let inv_token_id = env.register(InvoiceToken, ());
    let inv_token = InvoiceTokenClient::new(env, &inv_token_id);

    let token_admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let payment_token = TokenClient::new(env, &token_id.address());
    let payment_asset = AssetClient::new(env, &token_id.address());

    let invoice_id = Symbol::new(env, "INV_DIST");
    inv_token.initialize(
        &admin,
        &SorobanString::from_str(env, "Invoice Dist"),
        &SorobanString::from_str(env, "INVD"),
        &18,
        &invoice_id,
        &escrow_id,
    );

    escrow.initialize(&admin, &fee_bps);
    distributor.initialize(&admin);
    if configure_distributor {
        escrow.set_payment_distributor(&distributor_id);
    }

    TestContext {
        env: env.clone(),
        admin,
        seller,
        buyer,
        payer,
        escrow_id,
        escrow,
        distributor_id,
        distributor,
        inv_token,
        payment_token,
        payment_asset,
        invoice_id,
    }
}

fn create_and_fund(ctx: &TestContext<'_>, amount: i128, due_date: u64) {
    ctx.payment_asset.mint(&ctx.buyer, &amount);
    ctx.escrow.create_escrow(
        &ctx.invoice_id,
        &ctx.seller,
        &ctx.payer,
        &amount,
        &amount,
        &due_date,
        &ctx.payment_token.address,
        &ctx.inv_token.address,
        &soroban_sdk::BytesN::from_array(&ctx.env, &[0u8; 32]),
    );
    ctx.escrow.fund_escrow(&ctx.invoice_id, &ctx.buyer, &amount);
}

#[test]
fn test_double_initialize_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);
    let admin = Address::generate(&env);
    distributor.initialize(&admin);

    let result = distributor.try_initialize(&admin);
    assert_eq!(result, Err(Ok(Error::AlreadyInit)));
}

#[test]
fn test_get_distribution_state_defaults_to_zero() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);

    assert_eq!(state.paid_distributed, 0);
    assert!(!state.refund_distributed);
}

#[test]
fn test_distribute_payment_rejects_created_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);

    let result = ctx.distributor.try_distribute_payment(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![
            &env,
            ctx.payment_token.address.clone(),
            ctx.seller.clone(),
            ctx.buyer.clone(),
            ctx.admin.clone()
        ],
        &soroban_sdk::vec![&env, 0i128, 0i128, 0i128, 0i128],
        &0u32,
    );
    assert_eq!(result, Err(Ok(Error::InvalidEscrowStatus)));
}

#[test]
fn test_incremental_payment_distribution_tracks_paid_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true); // 5% fee
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &400);

    assert_eq!(ctx.payment_token.balance(&ctx.seller), 400);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 380);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 20);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 600);
    assert_eq!(
        ctx.distributor
            .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id)
            .paid_distributed,
        400
    );
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Funded
    );

    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &600);

    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 950);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 50);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);
    assert_eq!(ctx.payment_token.balance(&ctx.escrow_id), 0);
    assert_eq!(
        ctx.distributor
            .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id)
            .paid_distributed,
        1_000
    );
    assert_eq!(
        ctx.escrow.get_escrow_status(&ctx.invoice_id),
        EscrowStatus::Settled
    );
}

#[test]
fn test_refund_distribution_can_only_happen_once() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    env.ledger().set_timestamp(1_000);
    create_and_fund(&ctx, 1_000, 2_000);

    ctx.payment_asset.mint(&ctx.payer, &400);
    ctx.escrow.record_payment(&ctx.invoice_id, &ctx.payer, &400);

    env.ledger().set_timestamp(2_001);
    ctx.escrow.refund(&ctx.invoice_id);

    assert_eq!(ctx.payment_token.balance(&ctx.seller), 400);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 988);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 12);
    assert_eq!(ctx.payment_token.balance(&ctx.distributor_id), 0);

    let state = ctx
        .distributor
        .get_distribution_state(&ctx.escrow_id, &ctx.invoice_id);
    assert_eq!(state.paid_distributed, 400);
    assert!(state.refund_distributed);

    let second_refund = ctx.distributor.try_distribute_refund(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![&env, ctx.payment_token.address.clone(), ctx.buyer.clone()],
        &soroban_sdk::vec![&env, 600i128],
        &3u32,
    );
    assert_eq!(second_refund, Err(Ok(Error::RefundAlreadyDistributed)));
}

fn tier(min: i128, max: i128, bps: u32) -> FeeTier {
    FeeTier {
        min_amount: min,
        max_amount: max,
        fee_bps: bps,
    }
}

#[test]
fn test_set_platform_fee_success_and_lookup() {
    let env = Env::default();
    env.mock_all_auths();

    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);
    let admin = Address::generate(&env);
    distributor.initialize(&admin);

    let tiers = soroban_sdk::vec![
        &env,
        tier(0, 999, 500),
        tier(1_000, 9_999, 300),
        tier(10_000, i128::MAX, 100),
    ];
    distributor.set_platform_fee(&admin, &tiers);

    assert_eq!(distributor.get_platform_fee_bps(&500), 500);
    assert_eq!(distributor.get_platform_fee_bps(&1_000), 300);
    assert_eq!(distributor.get_platform_fee_bps(&9_999), 300);
    assert_eq!(distributor.get_platform_fee_bps(&1_000_000), 100);
}

#[test]
fn test_set_platform_fee_rejects_non_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);
    let admin = Address::generate(&env);
    let stranger = Address::generate(&env);
    distributor.initialize(&admin);

    let tiers = soroban_sdk::vec![&env, tier(0, i128::MAX, 500)];
    let result = distributor.try_set_platform_fee(&stranger, &tiers);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_set_platform_fee_rejects_empty_tiers() {
    let env = Env::default();
    env.mock_all_auths();

    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);
    let admin = Address::generate(&env);
    distributor.initialize(&admin);

    let tiers: soroban_sdk::Vec<FeeTier> = soroban_sdk::vec![&env];
    let result = distributor.try_set_platform_fee(&admin, &tiers);
    assert_eq!(result, Err(Ok(Error::EmptyFeeTiers)));
}

#[test]
fn test_set_platform_fee_rejects_gap_between_tiers() {
    let env = Env::default();
    env.mock_all_auths();

    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);
    let admin = Address::generate(&env);
    distributor.initialize(&admin);

    // Gap: first tier ends at 999, second starts at 2000 instead of 1000.
    let tiers = soroban_sdk::vec![&env, tier(0, 999, 500), tier(2_000, i128::MAX, 100)];
    let result = distributor.try_set_platform_fee(&admin, &tiers);
    assert_eq!(result, Err(Ok(Error::InvalidFeeTier)));
}

#[test]
fn test_set_platform_fee_rejects_tier_not_starting_at_zero() {
    let env = Env::default();
    env.mock_all_auths();

    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);
    let admin = Address::generate(&env);
    distributor.initialize(&admin);

    let tiers = soroban_sdk::vec![&env, tier(1, i128::MAX, 500)];
    let result = distributor.try_set_platform_fee(&admin, &tiers);
    assert_eq!(result, Err(Ok(Error::InvalidFeeTier)));
}

#[test]
fn test_set_platform_fee_rejects_bps_over_max() {
    let env = Env::default();
    env.mock_all_auths();

    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);
    let admin = Address::generate(&env);
    distributor.initialize(&admin);

    let tiers = soroban_sdk::vec![&env, tier(0, i128::MAX, 10_001)];
    let result = distributor.try_set_platform_fee(&admin, &tiers);
    assert_eq!(result, Err(Ok(Error::InvalidFeeTier)));
}

#[test]
fn test_get_platform_fee_bps_requires_configured_tiers() {
    let env = Env::default();
    env.mock_all_auths();

    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);
    let admin = Address::generate(&env);
    distributor.initialize(&admin);

    let result = distributor.try_get_platform_fee_bps(&100);
    assert_eq!(result, Err(Ok(Error::EmptyFeeTiers)));
}

#[test]
fn test_distribute_payment_fanout_splits_platform_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true); // 5% fee
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    let referral = Address::generate(&env);

    // Manually invoke distribute_payment with a fanout recipient splitting
    // the 5% (50) platform fee: referral gets 20, admin keeps the remaining 30.
    // Distributor needs seller(1000) + funder(950) + admin(30) + referral(20) = 2000.
    ctx.payment_asset.mint(&ctx.escrow_id, &2_000);
    ctx.payment_token
        .transfer(&ctx.escrow_id, &ctx.distributor_id, &2_000);
    let result = ctx.distributor.try_distribute_payment(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![
            &env,
            ctx.payment_token.address.clone(),
            ctx.seller.clone(),
            ctx.buyer.clone(),
            ctx.admin.clone(),
            referral.clone()
        ],
        &soroban_sdk::vec![&env, 1_000i128, 1_000i128, 950i128, 50i128, 20i128],
        &1u32,
    );
    assert_eq!(result, Ok(Ok(())));

    assert_eq!(ctx.payment_token.balance(&referral), 20);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 30);
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 950);
}

#[test]
fn test_distribute_payment_rejects_fanout_amount_exceeding_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true);
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    let referral = Address::generate(&env);
    ctx.payment_asset.mint(&ctx.escrow_id, &1_000);
    ctx.payment_token
        .transfer(&ctx.escrow_id, &ctx.distributor_id, &1_000);

    let result = ctx.distributor.try_distribute_payment(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![
            &env,
            ctx.payment_token.address.clone(),
            ctx.seller.clone(),
            ctx.buyer.clone(),
            ctx.admin.clone(),
            referral.clone()
        ],
        // Fanout amount (60) exceeds the total platform fee (50).
        &soroban_sdk::vec![&env, 1_000i128, 1_000i128, 950i128, 50i128, 60i128],
        &1u32,
    );
    assert_eq!(result, Err(Ok(Error::InvalidFeeSplit)));
}

#[test]
fn test_distribute_payment_rejects_mismatched_array_lengths() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);

    let result = ctx.distributor.try_distribute_payment(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![
            &env,
            ctx.payment_token.address.clone(),
            ctx.seller.clone(),
            ctx.buyer.clone(),
            ctx.admin.clone()
        ],
        &soroban_sdk::vec![&env, 0i128, 0i128, 0i128],
        &1u32,
    );
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_distribute_payment_rejects_too_many_fanout_recipients() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);

    let mut addresses = soroban_sdk::vec![
        &env,
        ctx.payment_token.address.clone(),
        ctx.seller.clone(),
        ctx.buyer.clone(),
        ctx.admin.clone()
    ];
    let mut amounts = soroban_sdk::vec![&env, 0i128, 0i128, 0i128, 0i128];
    for _ in 0..11 {
        addresses.push_back(Address::generate(&env));
        amounts.push_back(0i128);
    }

    let result = ctx.distributor.try_distribute_payment(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &addresses,
        &amounts,
        &1u32,
    );
    assert_eq!(result, Err(Ok(Error::TooManyFeeRecipients)));
}

#[test]
fn test_distribute_refund_legacy_single_funder_gets_full_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    ctx.payment_asset.mint(&ctx.distributor_id, &1_000);

    let result = ctx.distributor.try_distribute_refund(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![&env, ctx.payment_token.address.clone(), ctx.buyer.clone()],
        &soroban_sdk::vec![&env, 1_000i128],
        &3u32,
    );
    assert_eq!(result, Ok(Ok(())));
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 1_000);
}

#[test]
fn test_distribute_refund_splits_pro_rata_across_funders() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    let funder_a = Address::generate(&env);
    let funder_b = Address::generate(&env);
    let funder_c = Address::generate(&env);
    ctx.payment_asset.mint(&ctx.distributor_id, &1_000);

    // Weights 700/200/100 of a 1_000 refund => 700/200/100 exactly.
    let result = ctx.distributor.try_distribute_refund(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![
            &env,
            ctx.payment_token.address.clone(),
            funder_a.clone(),
            funder_b.clone(),
            funder_c.clone()
        ],
        &soroban_sdk::vec![&env, 1_000i128, 700i128, 200i128, 100i128],
        &3u32,
    );
    assert_eq!(result, Ok(Ok(())));
    assert_eq!(ctx.payment_token.balance(&funder_a), 700);
    assert_eq!(ctx.payment_token.balance(&funder_b), 200);
    assert_eq!(ctx.payment_token.balance(&funder_c), 100);
}

#[test]
fn test_distribute_refund_pro_rata_assigns_rounding_dust_to_last_funder() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    let funder_a = Address::generate(&env);
    let funder_b = Address::generate(&env);
    ctx.payment_asset.mint(&ctx.distributor_id, &100);

    // Equal weights on an odd total: 100 * 1/2 = 50 (floor), remainder 50 to funder_b.
    let result = ctx.distributor.try_distribute_refund(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![
            &env,
            ctx.payment_token.address.clone(),
            funder_a.clone(),
            funder_b.clone()
        ],
        &soroban_sdk::vec![&env, 99i128, 1i128, 1i128],
        &3u32,
    );
    assert_eq!(result, Ok(Ok(())));
    let total = ctx.payment_token.balance(&funder_a) + ctx.payment_token.balance(&funder_b);
    assert_eq!(total, 99);
    assert_eq!(ctx.payment_token.balance(&funder_a), 49);
    assert_eq!(ctx.payment_token.balance(&funder_b), 50);
}

#[test]
fn test_distribute_refund_rejects_zero_total_weight() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    let funder_a = Address::generate(&env);
    let funder_b = Address::generate(&env);

    let result = ctx.distributor.try_distribute_refund(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![
            &env,
            ctx.payment_token.address.clone(),
            funder_a.clone(),
            funder_b.clone()
        ],
        &soroban_sdk::vec![&env, 100i128, 0i128, 0i128],
        &3u32,
    );
    assert_eq!(result, Err(Ok(Error::InvalidRefundWeight)));
}

#[test]
fn test_distribute_refund_rejects_negative_weight() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    let funder_a = Address::generate(&env);
    let funder_b = Address::generate(&env);

    let result = ctx.distributor.try_distribute_refund(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![
            &env,
            ctx.payment_token.address.clone(),
            funder_a.clone(),
            funder_b.clone()
        ],
        &soroban_sdk::vec![&env, 100i128, -5i128, 105i128],
        &3u32,
    );
    assert_eq!(result, Err(Ok(Error::InvalidRefundWeight)));
}

#[test]
fn test_distribute_refund_rejects_mismatched_array_lengths() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);
    let funder_a = Address::generate(&env);
    let funder_b = Address::generate(&env);

    let result = ctx.distributor.try_distribute_refund(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &soroban_sdk::vec![
            &env,
            ctx.payment_token.address.clone(),
            funder_a.clone(),
            funder_b.clone()
        ],
        &soroban_sdk::vec![&env, 100i128, 50i128],
        &3u32,
    );
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_distribute_refund_rejects_too_many_recipients() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 300, true);

    let mut addresses = soroban_sdk::vec![&env, ctx.payment_token.address.clone()];
    let mut amounts = soroban_sdk::vec![&env, 100i128];
    for _ in 0..11 {
        addresses.push_back(Address::generate(&env));
        amounts.push_back(1i128);
    }

    let result = ctx.distributor.try_distribute_refund(
        &ctx.escrow_id,
        &ctx.invoice_id,
        &addresses,
        &amounts,
        &3u32,
    );
    assert_eq!(result, Err(Ok(Error::TooManyRefundRecipients)));
}

#[test]
fn test_get_investor_bonus_bps_defaults_to_zero() {
    let env = Env::default();
    env.mock_all_auths();

    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);
    let admin = Address::generate(&env);
    distributor.initialize(&admin);

    assert_eq!(distributor.get_investor_bonus_bps(), 0);
}

#[test]
fn test_set_investor_bonus_bps_rejects_non_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);
    let admin = Address::generate(&env);
    let stranger = Address::generate(&env);
    distributor.initialize(&admin);

    let result = distributor.try_set_investor_bonus_bps(&stranger, &500);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_set_investor_bonus_bps_rejects_over_max() {
    let env = Env::default();
    env.mock_all_auths();

    let distributor_id = env.register(PaymentDistributor, ());
    let distributor = PaymentDistributorClient::new(&env, &distributor_id);
    let admin = Address::generate(&env);
    distributor.initialize(&admin);

    let result = distributor.try_set_investor_bonus_bps(&admin, &10_001);
    assert_eq!(result, Err(Ok(Error::InvalidBonusRate)));
}

#[test]
fn test_distribute_payment_applies_investor_bonus_from_platform_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true); // 5% platform fee
    ctx.distributor.set_investor_bonus_bps(&ctx.admin, &400); // 4% investor bonus
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    // investor_amount=950, bonus=950*4%=38 (from the 50 fee), admin keeps 12.
    assert_eq!(ctx.payment_token.balance(&ctx.seller), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 988);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 12);
}

#[test]
fn test_distribute_payment_caps_investor_bonus_at_available_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let ctx = setup(&env, 500, true); // 5% platform fee = 50
    ctx.distributor.set_investor_bonus_bps(&ctx.admin, &10_000); // 100% bonus request
    create_and_fund(&ctx, 1_000, 50_000);
    ctx.payment_asset.mint(&ctx.payer, &1_000);

    ctx.escrow
        .record_payment(&ctx.invoice_id, &ctx.payer, &1_000);

    // Uncapped bonus would be 950, but only 50 is available from the fee.
    assert_eq!(ctx.payment_token.balance(&ctx.buyer), 1_000);
    assert_eq!(ctx.payment_token.balance(&ctx.admin), 0);
}
