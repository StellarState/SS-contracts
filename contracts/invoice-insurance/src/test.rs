use super::*;
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype,
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient as AssetClient},
    Address, BytesN, Env,
};

#[contracttype]
#[derive(Clone)]
enum MockKey {
    Eligible(Address, BytesN<32>),
    Fail,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
enum MockError {
    Unavailable = 1,
}

#[contract]
struct MockPositionRegistry;

#[contractimpl]
impl MockPositionRegistry {
    pub fn set_eligible(env: Env, investor: Address, invoice_id: BytesN<32>, amount: i128) {
        env.storage()
            .instance()
            .set(&MockKey::Eligible(investor, invoice_id), &amount);
    }

    pub fn transfer_eligibility(
        env: Env,
        from: Address,
        to: Address,
        invoice_id: BytesN<32>,
        amount: i128,
    ) {
        let from_key = MockKey::Eligible(from, invoice_id.clone());
        let current: i128 = env.storage().instance().get(&from_key).unwrap_or(0);
        env.storage()
            .instance()
            .set(&from_key, &current.saturating_sub(amount));
        let to_key = MockKey::Eligible(to, invoice_id);
        let received: i128 = env.storage().instance().get(&to_key).unwrap_or(0);
        env.storage()
            .instance()
            .set(&to_key, &received.saturating_add(amount));
    }

    pub fn set_failing(env: Env, failing: bool) {
        env.storage().instance().set(&MockKey::Fail, &failing);
    }

    pub fn eligible_principal(
        env: Env,
        investor: Address,
        invoice_id: BytesN<32>,
    ) -> Result<i128, MockError> {
        if env
            .storage()
            .instance()
            .get(&MockKey::Fail)
            .unwrap_or(false)
        {
            return Err(MockError::Unavailable);
        }
        Ok(env
            .storage()
            .instance()
            .get(&MockKey::Eligible(investor, invoice_id))
            .unwrap_or(0))
    }
}

fn setup<'a>(
    env: &'a Env,
) -> (
    InvoiceInsuranceClient<'a>,
    Address,
    TokenClient<'a>,
    AssetClient<'a>,
    MockPositionRegistryClient<'a>,
) {
    env.mock_all_auths();
    let contract_id = env.register(InvoiceInsurance, ());
    let client = InvoiceInsuranceClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract_v2(Address::generate(env));
    let token = TokenClient::new(env, &token_id.address());
    let asset = AssetClient::new(env, &token_id.address());
    let registry_id = env.register(MockPositionRegistry, ());
    let registry = MockPositionRegistryClient::new(env, &registry_id);
    client.initialize(&admin, &token_id.address(), &registry_id);
    (client, admin, token, asset, registry)
}

#[test]
fn premium_is_two_percent_of_coverage_amount() {
    let env = Env::default();
    let (client, _admin, token, asset, registry) = setup(&env);
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    asset.mint(&investor, &1_000);
    registry.set_eligible(&investor, &invoice_id, &10_000);

    // Coverage amount = 10_000 * 50% = 5_000; premium = 2% of 5_000 = 100.
    client.purchase_insurance(&investor, &invoice_id, &10_000, &5_000);

    assert_eq!(token.balance(&investor), 900);
    assert_eq!(token.balance(&client.address), 100);
    let policy = client.get_policy(&invoice_id, &investor).unwrap();
    assert_eq!(policy.principal, 10_000);
    assert_eq!(policy.eligible_principal, 10_000);
    assert_eq!(policy.coverage_amount, 5_000);
    assert_eq!(policy.premium, 100);
}

#[test]
fn rejects_invalid_and_duplicate_policies() {
    let env = Env::default();
    let (client, _admin, _token, asset, registry) = setup(&env);
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    asset.mint(&investor, &1_000);
    registry.set_eligible(&investor, &invoice_id, &10_000);

    assert_eq!(
        client.try_purchase_insurance(&investor, &invoice_id, &10_000, &0),
        Err(Ok(Error::InvalidCoverage))
    );
    assert_eq!(
        client.try_purchase_insurance(&investor, &invoice_id, &10_000, &10_001),
        Err(Ok(Error::InvalidCoverage))
    );
    assert_eq!(
        client.try_purchase_insurance(&investor, &invoice_id, &0, &5_000),
        Err(Ok(Error::InvalidAmount))
    );

    client.purchase_insurance(&investor, &invoice_id, &10_000, &5_000);
    assert_eq!(
        client.try_purchase_insurance(&investor, &invoice_id, &10_000, &5_000),
        Err(Ok(Error::PolicyAlreadyExists))
    );
}

#[test]
fn payout_covers_coverage_bps_of_principal_loss() {
    let env = Env::default();
    let (client, admin, token, asset, registry) = setup(&env);
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    asset.mint(&investor, &1_000);
    asset.mint(&client.address, &10_000);
    registry.set_eligible(&investor, &invoice_id, &10_000);

    client.purchase_insurance(&investor, &invoice_id, &10_000, &5_000);

    // 40% principal loss = 4_000; 50% of it is covered = 2_000.
    let paid = client.trigger_insurance_payout(&invoice_id, &4_000);
    assert_eq!(env.auths()[0].0, admin);
    assert_eq!(paid, 2_000);
    assert_eq!(token.balance(&investor), 900 + 2_000);

    assert_eq!(
        client.try_trigger_insurance_payout(&invoice_id, &4_000),
        Err(Ok(Error::PayoutAlreadyTriggered))
    );
}

#[test]
fn payout_is_capped_at_coverage_amount() {
    let env = Env::default();
    let (client, _admin, token, asset, registry) = setup(&env);
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    asset.mint(&investor, &1_000);
    asset.mint(&client.address, &10_000);
    registry.set_eligible(&investor, &invoice_id, &10_000);

    client.purchase_insurance(&investor, &invoice_id, &10_000, &10_000);

    // Total loss with full coverage pays exactly the coverage amount.
    let paid = client.trigger_insurance_payout(&invoice_id, &10_000);
    assert_eq!(paid, 10_000);
    assert_eq!(token.balance(&investor), 1_000 - 200 + 10_000);
}

#[test]
fn payout_fails_when_pool_is_insufficient() {
    let env = Env::default();
    let (client, _admin, _token, asset, registry) = setup(&env);
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    asset.mint(&investor, &1_000);
    registry.set_eligible(&investor, &invoice_id, &10_000);

    client.purchase_insurance(&investor, &invoice_id, &10_000, &5_000);
    assert_eq!(
        client.try_trigger_insurance_payout(&invoice_id, &10_000),
        Err(Ok(Error::InsufficientPool))
    );
}

#[test]
fn rejects_inflated_principal_before_charging_premium() {
    let env = Env::default();
    let (client, _admin, token, asset, registry) = setup(&env);
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[2u8; 32]);
    asset.mint(&investor, &1_000);
    registry.set_eligible(&investor, &invoice_id, &5_000);

    assert_eq!(
        client.try_purchase_insurance(&investor, &invoice_id, &5_001, &5_000),
        Err(Ok(Error::InvalidPosition))
    );
    assert_eq!(token.balance(&investor), 1_000);
    assert_eq!(token.balance(&client.address), 0);
    assert!(client.get_policy(&invoice_id, &investor).is_none());
    assert_eq!(client.get_policyholders(&invoice_id).len(), 0);
}

#[test]
fn rejects_unknown_and_unfunded_positions() {
    let env = Env::default();
    let (client, _admin, _token, _asset, registry) = setup(&env);
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[3u8; 32]);

    // No registry entry: unknown position.
    assert_eq!(
        client.try_purchase_insurance(&investor, &invoice_id, &100, &5_000),
        Err(Ok(Error::InvalidPosition))
    );

    // Explicitly unfunded position.
    registry.set_eligible(&investor, &invoice_id, &0);
    assert_eq!(
        client.try_purchase_insurance(&investor, &invoice_id, &100, &5_000),
        Err(Ok(Error::InvalidPosition))
    );
    assert_eq!(client.get_policyholders(&invoice_id).len(), 0);
}

#[test]
fn registry_failure_rejects_purchase_without_side_effects() {
    let env = Env::default();
    let (client, _admin, token, asset, registry) = setup(&env);
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[4u8; 32]);
    asset.mint(&investor, &1_000);
    registry.set_eligible(&investor, &invoice_id, &10_000);
    registry.set_failing(&true);

    assert_eq!(
        client.try_purchase_insurance(&investor, &invoice_id, &10_000, &5_000),
        Err(Ok(Error::RegistryCallFailed))
    );
    assert_eq!(token.balance(&investor), 1_000);
    assert_eq!(token.balance(&client.address), 0);
    assert!(client.get_policy(&invoice_id, &investor).is_none());
    assert_eq!(client.get_policyholders(&invoice_id).len(), 0);
}

#[test]
fn transfer_of_eligibility_follows_purchase_snapshot_policy() {
    let env = Env::default();
    let (client, _admin, _token, asset, registry) = setup(&env);
    let investor = Address::generate(&env);
    let transferee = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[5u8; 32]);

    asset.mint(&investor, &1_000);
    asset.mint(&transferee, &1_000);
    asset.mint(&client.address, &20_000);
    registry.set_eligible(&investor, &invoice_id, &10_000);

    client.purchase_insurance(&investor, &invoice_id, &10_000, &10_000);

    // Move the underlying eligible principal to another wallet.
    registry.transfer_eligibility(&investor, &transferee, &invoice_id, &10_000);

    // The original policy is a snapshot and still pays on its validated
    // principal; the receiving wallet may insure its own new position.
    client.purchase_insurance(&transferee, &invoice_id, &10_000, &10_000);

    let original = client.get_policy(&invoice_id, &investor).unwrap();
    assert_eq!(original.principal, 10_000);
    assert_eq!(original.eligible_principal, 10_000);
    let transferred = client.get_policy(&invoice_id, &transferee).unwrap();
    assert_eq!(transferred.principal, 10_000);
    assert_eq!(transferred.eligible_principal, 10_000);

    let paid = client.trigger_insurance_payout(&invoice_id, &10_000);
    assert_eq!(paid, 20_000);
    assert_eq!(client.get_policyholders(&invoice_id).len(), 2);
}

#[test]
fn post_default_purchase_is_rejected() {
    let env = Env::default();
    let (client, _admin, _token, asset, registry) = setup(&env);
    let investor = Address::generate(&env);
    let newcomer = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[6u8; 32]);

    asset.mint(&investor, &1_000);
    asset.mint(&newcomer, &1_000);
    asset.mint(&client.address, &10_000);
    registry.set_eligible(&investor, &invoice_id, &10_000);

    client.purchase_insurance(&investor, &invoice_id, &10_000, &10_000);
    client.trigger_insurance_payout(&invoice_id, &10_000);

    // Principal acquired after default cannot create a new policy.
    registry.set_eligible(&newcomer, &invoice_id, &10_000);
    assert_eq!(
        client.try_purchase_insurance(&newcomer, &invoice_id, &10_000, &10_000),
        Err(Ok(Error::PayoutAlreadyTriggered))
    );
    assert_eq!(client.get_policyholders(&invoice_id).len(), 1);
}
