use super::*;
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, vec, Address, BytesN, Env};

struct Fixture<'a> {
    contract_id: Address,
    client: DividendReinvestmentClient<'a>,
    wallet: Address,
    token: Address,
}

fn setup<'a>(env: &'a Env) -> Fixture<'a> {
    env.mock_all_auths();

    let admin = Address::generate(env);
    let wallet = Address::generate(env);

    let contract_id = env.register(DividendReinvestment, ());
    let client = DividendReinvestmentClient::new(env, &contract_id);
    client.initialize(&admin);

    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token = token_contract.address();
    let token_client = StellarAssetClient::new(env, &token);
    token_client.mint(&wallet, &100_000);

    Fixture {
        contract_id,
        client,
        wallet,
        token,
    }
}

fn get_invoices(env: &Env, contract_id: &Address) -> Vec<BytesN<32>> {
    env.as_contract(contract_id, || {
        env.storage()
            .instance()
            .get(&DataKey::Invoices)
            .unwrap_or_else(|| Vec::new(env))
    })
}

fn get_invoice(env: &Env, contract_id: &Address, invoice_id: &BytesN<32>) -> Option<TargetInvoice> {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::Invoice(invoice_id.clone()))
    })
}

#[test]
fn test_identical_retry_preserves_single_index_entry() {
    let env = Env::default();
    let f = setup(&env);

    let inv_id = BytesN::from_array(&env, &[1u8; 32]);
    let target = TargetInvoice {
        token: f.token.clone(),
        yield_bps: 500,
        capacity: 10_000,
    };

    // First registration
    f.client.register_invoice(&inv_id, &target);
    assert_eq!(get_invoices(&env, &f.contract_id).len(), 1);
    assert_eq!(
        get_invoice(&env, &f.contract_id, &inv_id),
        Some(target.clone())
    );

    // Identical retry
    let res = f.client.try_register_invoice(&inv_id, &target);
    assert_eq!(res, Ok(Ok(())));

    // Still exactly one searchable index entry
    let invoices = get_invoices(&env, &f.contract_id);
    assert_eq!(invoices.len(), 1);
    assert_eq!(invoices.get(0), Some(inv_id.clone()));
    assert_eq!(get_invoice(&env, &f.contract_id, &inv_id), Some(target));
}

#[test]
fn test_replaying_registration_after_partial_reinvestment_preserves_capacity() {
    let env = Env::default();
    let f = setup(&env);

    // Configure reinvestment
    f.client.set_reinvestment_config(
        &f.wallet,
        &ReinvestmentConfig {
            token: f.token.clone(),
            min_yield_bps: 500,
            max_amount: 10_000,
            enabled: true,
        },
    );

    let inv_id = BytesN::from_array(&env, &[1u8; 32]);
    let initial_target = TargetInvoice {
        token: f.token.clone(),
        yield_bps: 500,
        capacity: 10_000,
    };

    f.client.register_invoice(&inv_id, &initial_target);

    // Consume partial capacity
    let matched = f.client.execute_reinvestment(&f.wallet, &4_000);
    assert_eq!(matched, Some(inv_id.clone()));

    let current = get_invoice(&env, &f.contract_id, &inv_id).unwrap();
    assert_eq!(current.capacity, 6_000);

    // Replay initial registration (with original capacity 10_000)
    let retry_res = f.client.try_register_invoice(&inv_id, &initial_target);
    assert_eq!(retry_res, Ok(Ok(())));

    // Capacity must remain 6,000, NOT restored to 10,000
    let after_replay = get_invoice(&env, &f.contract_id, &inv_id).unwrap();
    assert_eq!(after_replay.capacity, 6_000);
    assert_eq!(get_invoices(&env, &f.contract_id).len(), 1);
}

#[test]
fn test_replaying_registration_after_exhausted_target_preserves_zero_capacity() {
    let env = Env::default();
    let f = setup(&env);

    f.client.set_reinvestment_config(
        &f.wallet,
        &ReinvestmentConfig {
            token: f.token.clone(),
            min_yield_bps: 500,
            max_amount: 10_000,
            enabled: true,
        },
    );

    let inv_id = BytesN::from_array(&env, &[1u8; 32]);
    let initial_target = TargetInvoice {
        token: f.token.clone(),
        yield_bps: 500,
        capacity: 10_000,
    };

    f.client.register_invoice(&inv_id, &initial_target);

    // Consume entire capacity
    let matched = f.client.execute_reinvestment(&f.wallet, &10_000);
    assert_eq!(matched, Some(inv_id.clone()));

    let current = get_invoice(&env, &f.contract_id, &inv_id).unwrap();
    assert_eq!(current.capacity, 0);

    // Replay registration on exhausted target
    let retry_res = f.client.try_register_invoice(&inv_id, &initial_target);
    assert_eq!(retry_res, Ok(Ok(())));

    // Capacity must remain 0
    let after_replay = get_invoice(&env, &f.contract_id, &inv_id).unwrap();
    assert_eq!(after_replay.capacity, 0);

    // Subsequent reinvestment cannot match exhausted invoice
    let no_match = f.client.execute_reinvestment(&f.wallet, &1_000);
    assert_eq!(no_match, None);
}

#[test]
fn test_conflicting_duplicate_registrations_produce_typed_error() {
    let env = Env::default();
    let f = setup(&env);

    let inv_id = BytesN::from_array(&env, &[1u8; 32]);
    let target = TargetInvoice {
        token: f.token.clone(),
        yield_bps: 500,
        capacity: 10_000,
    };
    f.client.register_invoice(&inv_id, &target);

    // Conflicting token
    let other_token = Address::generate(&env);
    let conflict_token = TargetInvoice {
        token: other_token,
        yield_bps: 500,
        capacity: 10_000,
    };
    let res_token = f.client.try_register_invoice(&inv_id, &conflict_token);
    assert_eq!(res_token, Err(Ok(Error::InvoiceConflict)));

    // Conflicting yield
    let conflict_yield = TargetInvoice {
        token: f.token.clone(),
        yield_bps: 600,
        capacity: 10_000,
    };
    let res_yield = f.client.try_register_invoice(&inv_id, &conflict_yield);
    assert_eq!(res_yield, Err(Ok(Error::InvoiceConflict)));

    // Conflicting capacity
    let conflict_capacity = TargetInvoice {
        token: f.token.clone(),
        yield_bps: 500,
        capacity: 15_000,
    };
    let res_cap = f.client.try_register_invoice(&inv_id, &conflict_capacity);
    assert_eq!(res_cap, Err(Ok(Error::InvoiceConflict)));

    // Ensure original record and index are completely untouched
    assert_eq!(get_invoice(&env, &f.contract_id, &inv_id), Some(target));
    assert_eq!(get_invoices(&env, &f.contract_id).len(), 1);
}

#[test]
fn test_retries_preserve_matching_order_for_unrelated_invoices() {
    let env = Env::default();
    let f = setup(&env);

    f.client.set_reinvestment_config(
        &f.wallet,
        &ReinvestmentConfig {
            token: f.token.clone(),
            min_yield_bps: 500,
            max_amount: 10_000,
            enabled: true,
        },
    );

    let inv_a = BytesN::from_array(&env, &[1u8; 32]);
    let target_a = TargetInvoice {
        token: f.token.clone(),
        yield_bps: 500,
        capacity: 10_000,
    };
    let inv_b = BytesN::from_array(&env, &[2u8; 32]);
    let target_b = TargetInvoice {
        token: f.token.clone(),
        yield_bps: 500,
        capacity: 10_000,
    };

    f.client.register_invoice(&inv_a, &target_a);
    f.client.register_invoice(&inv_b, &target_b);

    assert_eq!(
        get_invoices(&env, &f.contract_id),
        vec![&env, inv_a.clone(), inv_b.clone()]
    );

    // Retry invoice A
    let retry_res = f.client.try_register_invoice(&inv_a, &target_a);
    assert_eq!(retry_res, Ok(Ok(())));

    // Order must remain [inv_a, inv_b]
    assert_eq!(
        get_invoices(&env, &f.contract_id),
        vec![&env, inv_a.clone(), inv_b.clone()]
    );

    // Reinvestment matches invoice A first
    let matched = f.client.execute_reinvestment(&f.wallet, &3_000);
    assert_eq!(matched, Some(inv_a));
}

#[test]
fn test_failed_registration_changes_neither_target_records_nor_index() {
    let env = Env::default();
    env.mock_all_auths();

    // Not initialized
    let contract_id = env.register(DividendReinvestment, ());
    let client = DividendReinvestmentClient::new(&env, &contract_id);
    let inv_id = BytesN::from_array(&env, &[1u8; 32]);
    let token = Address::generate(&env);

    let target = TargetInvoice {
        token: token.clone(),
        yield_bps: 500,
        capacity: 10_000,
    };
    let res = client.try_register_invoice(&inv_id, &target);
    assert_eq!(res, Err(Ok(Error::NotInitialized)));
    assert_eq!(get_invoices(&env, &contract_id).len(), 0);
    assert_eq!(get_invoice(&env, &contract_id, &inv_id), None);

    // Initialize and test invalid amount (capacity <= 0)
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let invalid_target = TargetInvoice {
        token,
        yield_bps: 500,
        capacity: 0,
    };
    let res_inv = client.try_register_invoice(&inv_id, &invalid_target);
    assert_eq!(res_inv, Err(Ok(Error::InvalidAmount)));
    assert_eq!(get_invoices(&env, &contract_id).len(), 0);
    assert_eq!(get_invoice(&env, &contract_id, &inv_id), None);
}

#[test]
fn test_initialize_and_already_initialized() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DividendReinvestment, ());
    let client = DividendReinvestmentClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    // Initialize succeeds
    client.initialize(&admin);

    // Initializing again fails with AlreadyInitialized
    let res_already = client.try_initialize(&admin);
    assert_eq!(res_already, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
fn test_reinvestment_config_validation_and_disable() {
    let env = Env::default();
    let f = setup(&env);

    // Disabling non-existent config does not fail
    let unknown_wallet = Address::generate(&env);
    let res_disable_empty = f.client.try_disable_reinvestment(&unknown_wallet);
    assert_eq!(res_disable_empty, Ok(Ok(())));

    // Setting config with invalid max_amount (<= 0) fails with InvalidAmount
    let invalid_config = ReinvestmentConfig {
        token: f.token.clone(),
        min_yield_bps: 500,
        max_amount: 0,
        enabled: true,
    };
    let res_inv = f
        .client
        .try_set_reinvestment_config(&f.wallet, &invalid_config);
    assert_eq!(res_inv, Err(Ok(Error::InvalidAmount)));

    // Set valid config
    let valid_config = ReinvestmentConfig {
        token: f.token.clone(),
        min_yield_bps: 500,
        max_amount: 5_000,
        enabled: true,
    };
    f.client.set_reinvestment_config(&f.wallet, &valid_config);
    assert_eq!(
        f.client.get_reinvestment_config(&f.wallet),
        Some(valid_config)
    );

    // Disable reinvestment sets enabled = false
    f.client.disable_reinvestment(&f.wallet);
    let disabled_cfg = f.client.get_reinvestment_config(&f.wallet).unwrap();
    assert!(!disabled_cfg.enabled);
}

#[test]
fn test_execute_reinvestment_filtering_and_edge_cases() {
    let env = Env::default();
    let f = setup(&env);

    // Payout amount <= 0 fails with InvalidAmount
    let res_zero = f.client.try_execute_reinvestment(&f.wallet, &0);
    assert_eq!(res_zero, Err(Ok(Error::InvalidAmount)));

    let res_neg = f.client.try_execute_reinvestment(&f.wallet, &-100);
    assert_eq!(res_neg, Err(Ok(Error::InvalidAmount)));

    // No config configured returns Ok(None)
    let no_cfg_res = f.client.execute_reinvestment(&f.wallet, &1_000);
    assert_eq!(no_cfg_res, None);

    // Set config
    f.client.set_reinvestment_config(
        &f.wallet,
        &ReinvestmentConfig {
            token: f.token.clone(),
            min_yield_bps: 500,
            max_amount: 5_000,
            enabled: true,
        },
    );

    // Payout exceeds max_amount returns None
    let over_max = f.client.execute_reinvestment(&f.wallet, &6_000);
    assert_eq!(over_max, None);

    // Disabled config returns None
    f.client.disable_reinvestment(&f.wallet);
    let disabled_res = f.client.execute_reinvestment(&f.wallet, &1_000);
    assert_eq!(disabled_res, None);

    // Re-enable config
    f.client.set_reinvestment_config(
        &f.wallet,
        &ReinvestmentConfig {
            token: f.token.clone(),
            min_yield_bps: 500,
            max_amount: 5_000,
            enabled: true,
        },
    );

    // Register invoice with different token
    let inv_diff_token = BytesN::from_array(&env, &[10u8; 32]);
    let other_token = Address::generate(&env);
    f.client.register_invoice(
        &inv_diff_token,
        &TargetInvoice {
            token: other_token,
            yield_bps: 600,
            capacity: 10_000,
        },
    );

    // Does not match different token
    assert_eq!(f.client.execute_reinvestment(&f.wallet, &1_000), None);

    // Register invoice with yield below min_yield_bps (400 < 500)
    let inv_low_yield = BytesN::from_array(&env, &[11u8; 32]);
    f.client.register_invoice(
        &inv_low_yield,
        &TargetInvoice {
            token: f.token.clone(),
            yield_bps: 400,
            capacity: 10_000,
        },
    );

    // Does not match lower yield
    assert_eq!(f.client.execute_reinvestment(&f.wallet, &1_000), None);
}
