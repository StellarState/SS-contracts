#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient as AssetClient,
    Address, BytesN, Env, Vec,
};

fn setup_token(env: &Env) -> (Address, AssetClient<'_>) {
    let token_admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let asset = AssetClient::new(env, &token_id.address());
    (token_id.address(), asset)
}

#[test]
fn test_initialize_and_create_vest() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(YieldVesting, ());
    let client = YieldVestingClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let (token, _asset) = setup_token(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin);

    let mut tranches = Vec::new(&env);
    tranches.push_back(Tranche {
        amount: 500,
        release_timestamp: 1000,
        claimed: false,
    });
    tranches.push_back(Tranche {
        amount: 500,
        release_timestamp: 2000,
        claimed: false,
    });

    let vest_id = client.create_yield_vest(&invoice_id, &beneficiary, &token, &tranches);
    assert_eq!(vest_id, 1);

    let schedule = client.get_vest_schedule(&vest_id);
    assert_eq!(schedule.total_amount, 1000);
    assert_eq!(schedule.claimed_amount, 0);
    assert_eq!(schedule.tranches.len(), 2);
    assert_eq!(schedule.tranches.get(0).unwrap().claimed, false);
    assert_eq!(schedule.tranches.get(1).unwrap().claimed, false);
    assert_eq!(
        schedule.tranches.get(0).unwrap().amount + schedule.tranches.get(1).unwrap().amount,
        schedule.total_amount
    );
}

#[test]
fn test_create_vest_rejects_preclaimed_tranches_atomically() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(YieldVesting, ());
    let client = YieldVestingClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let (token, _asset) = setup_token(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin);

    let mut all_preclaimed = Vec::new(&env);
    all_preclaimed.push_back(Tranche {
        amount: 400,
        release_timestamp: 1000,
        claimed: true,
    });
    all_preclaimed.push_back(Tranche {
        amount: 600,
        release_timestamp: 2000,
        claimed: true,
    });
    assert!(client
        .try_create_yield_vest(&invoice_id, &beneficiary, &token, &all_preclaimed)
        .is_err());

    let mut mixed = Vec::new(&env);
    mixed.push_back(Tranche {
        amount: 400,
        release_timestamp: 1000,
        claimed: false,
    });
    mixed.push_back(Tranche {
        amount: 600,
        release_timestamp: 2000,
        claimed: true,
    });
    assert!(client
        .try_create_yield_vest(&invoice_id, &beneficiary, &token, &mixed)
        .is_err());
    assert_eq!(env.events().all().events().len(), 0);

    let mut pending = Vec::new(&env);
    pending.push_back(Tranche {
        amount: 400,
        release_timestamp: 1000,
        claimed: false,
    });
    let vest_id = client.create_yield_vest(&invoice_id, &beneficiary, &token, &pending);
    assert_eq!(vest_id, 1);
    assert_eq!(env.events().all().events().len(), 1);

    let schedule = client.get_vest_schedule(&vest_id);
    assert_eq!(schedule.total_amount, 400);
    assert_eq!(schedule.claimed_amount, 0);
    assert!(!schedule.tranches.get(0).unwrap().claimed);
}

#[test]
fn test_claim_tranche_blocked_before_release_time() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(500);

    let contract_id = env.register(YieldVesting, ());
    let client = YieldVestingClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let (token, _asset) = setup_token(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin);

    let mut tranches = Vec::new(&env);
    tranches.push_back(Tranche {
        amount: 500,
        release_timestamp: 1000,
        claimed: false,
    });

    let vest_id = client.create_yield_vest(&invoice_id, &beneficiary, &token, &tranches);

    // Should fail with TimeNotReached
    let res = client.try_claim_tranche(&vest_id, &0);
    assert!(res.is_err());
}

#[test]
fn test_claim_tranche_success_after_release_time() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(YieldVesting, ());
    let client = YieldVestingClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let (token, asset) = setup_token(&env);
    asset.mint(&contract_id, &10_000);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin);

    let mut tranches = Vec::new(&env);
    tranches.push_back(Tranche {
        amount: 600,
        release_timestamp: 1000,
        claimed: false,
    });

    let vest_id = client.create_yield_vest(&invoice_id, &beneficiary, &token, &tranches);

    // Fast-forward timestamp past release
    env.ledger().set_timestamp(1500);

    let claimed = client.claim_tranche(&vest_id, &0);
    assert_eq!(claimed, 600);

    let schedule = client.get_vest_schedule(&vest_id);
    assert_eq!(schedule.claimed_amount, 600);
    assert_eq!(schedule.tranches.get(0).unwrap().claimed, true);

    // Attempting to claim again should fail
    let res2 = client.try_claim_tranche(&vest_id, &0);
    assert!(res2.is_err());
}

#[test]
fn test_batch_claim_and_claim_all_eligible() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(YieldVesting, ());
    let client = YieldVestingClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let (token, asset) = setup_token(&env);
    asset.mint(&contract_id, &10_000);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin);

    let mut tranches = Vec::new(&env);
    tranches.push_back(Tranche {
        amount: 200,
        release_timestamp: 1000,
        claimed: false,
    });
    tranches.push_back(Tranche {
        amount: 300,
        release_timestamp: 2000,
        claimed: false,
    });
    tranches.push_back(Tranche {
        amount: 500,
        release_timestamp: 5000,
        claimed: false,
    });

    let vest_id = client.create_yield_vest(&invoice_id, &beneficiary, &token, &tranches);

    // Advance to 2500 -> first 2 tranches eligible, third is not
    env.ledger().set_timestamp(2500);

    let claimed_total = client.claim_all_eligible_tranches(&vest_id);
    assert_eq!(claimed_total, 500); // 200 + 300

    let schedule = client.get_vest_schedule(&vest_id);
    assert_eq!(schedule.claimed_amount, 500);
    assert_eq!(schedule.tranches.get(0).unwrap().claimed, true);
    assert_eq!(schedule.tranches.get(1).unwrap().claimed, true);
    assert_eq!(schedule.tranches.get(2).unwrap().claimed, false);
}

#[test]
fn test_underfunded_claim_paths_preserve_entitlements_until_funded() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1000);

    let contract_id = env.register(YieldVesting, ());
    let client = YieldVestingClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let (token, asset) = setup_token(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin);

    let mut single_tranche = Vec::new(&env);
    single_tranche.push_back(Tranche {
        amount: 100,
        release_timestamp: 1000,
        claimed: false,
    });
    let single_vest = client.create_yield_vest(&invoice_id, &beneficiary, &token, &single_tranche);

    let mut batch_tranches = Vec::new(&env);
    batch_tranches.push_back(Tranche {
        amount: 40,
        release_timestamp: 1000,
        claimed: false,
    });
    batch_tranches.push_back(Tranche {
        amount: 60,
        release_timestamp: 1000,
        claimed: false,
    });
    let batch_vest = client.create_yield_vest(&invoice_id, &beneficiary, &token, &batch_tranches);

    let mut all_tranches = Vec::new(&env);
    all_tranches.push_back(Tranche {
        amount: 100,
        release_timestamp: 1000,
        claimed: false,
    });
    let all_vest = client.create_yield_vest(&invoice_id, &beneficiary, &token, &all_tranches);

    let contract_address = env.current_contract_address();
    asset.mint(&contract_address, &99);

    let mut tranche_indices = Vec::new(&env);
    tranche_indices.push_back(0);
    tranche_indices.push_back(1);

    let events_before = env.events().all().events().len();
    assert_eq!(
        client.try_claim_tranche(&single_vest, &0),
        Err(Ok(Error::InsufficientFunds))
    );
    assert_eq!(client.get_vest_schedule(&single_vest).claimed_amount, 0);
    assert!(
        !client
            .get_vest_schedule(&single_vest)
            .tranches
            .get(0)
            .unwrap()
            .claimed
    );
    assert_eq!(asset.balance(&beneficiary), 0);
    assert_eq!(asset.balance(&contract_address), 99);
    assert_eq!(env.events().all().events().len(), events_before);

    asset.mint(&contract_address, &1);
    assert_eq!(client.claim_tranche(&single_vest, &0), 100);
    assert_eq!(asset.balance(&beneficiary), 100);
    assert_eq!(asset.balance(&contract_address), 0);
    assert_eq!(client.get_vest_schedule(&single_vest).claimed_amount, 100);

    asset.mint(&contract_address, &99);
    let events_before = env.events().all().events().len();
    assert_eq!(
        client.try_claim_batch_tranches(&batch_vest, &tranche_indices),
        Err(Ok(Error::InsufficientFunds))
    );
    let batch_schedule = client.get_vest_schedule(&batch_vest);
    assert_eq!(batch_schedule.claimed_amount, 0);
    assert!(!batch_schedule.tranches.get(0).unwrap().claimed);
    assert!(!batch_schedule.tranches.get(1).unwrap().claimed);
    assert_eq!(asset.balance(&beneficiary), 100);
    assert_eq!(asset.balance(&contract_address), 99);
    assert_eq!(env.events().all().events().len(), events_before);

    asset.mint(&contract_address, &1);
    assert_eq!(
        client.claim_batch_tranches(&batch_vest, &tranche_indices),
        100
    );
    assert_eq!(asset.balance(&beneficiary), 200);
    assert_eq!(asset.balance(&contract_address), 0);
    assert_eq!(client.get_vest_schedule(&batch_vest).claimed_amount, 100);

    asset.mint(&contract_address, &99);
    let events_before = env.events().all().events().len();
    assert_eq!(
        client.try_claim_all_eligible_tranches(&all_vest),
        Err(Ok(Error::InsufficientFunds))
    );
    let all_schedule = client.get_vest_schedule(&all_vest);
    assert_eq!(all_schedule.claimed_amount, 0);
    assert!(!all_schedule.tranches.get(0).unwrap().claimed);
    assert_eq!(asset.balance(&beneficiary), 200);
    assert_eq!(asset.balance(&contract_address), 99);
    assert_eq!(env.events().all().events().len(), events_before);

    asset.mint(&contract_address, &1);
    assert_eq!(client.claim_all_eligible_tranches(&all_vest), 100);
    assert_eq!(asset.balance(&beneficiary), 300);
    assert_eq!(asset.balance(&contract_address), 0);
    assert_eq!(client.get_vest_schedule(&all_vest).claimed_amount, 100);
}
