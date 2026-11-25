use super::*;
use soroban_sdk::testutils::Address as _;

fn setup<'a>(
    env: &'a Env,
    target_amount: i128,
    senior_pct: u32,
) -> (InvoiceTrancheContractClient<'a>, Address, BytesN<32>) {
    let contract_id = env.register(InvoiceTrancheContract, ());
    let client = InvoiceTrancheContractClient::new(env, &contract_id);
    let issuer = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[7; 32]);
    client.create_tranches(&issuer, &invoice_id, &target_amount, &senior_pct, &0);
    (client, issuer, invoice_id)
}

#[test]
fn enforces_class_quotas_and_allows_the_other_class_to_finish() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, invoice_id) = setup(&env, 100, 70);
    let senior_one = Address::generate(&env);
    let senior_two = Address::generate(&env);
    let junior = Address::generate(&env);

    client.invest_tranche(&senior_one, &invoice_id, &(TrancheType::Senior as u32), &40);
    client.invest_tranche(&senior_two, &invoice_id, &(TrancheType::Senior as u32), &30);
    assert_eq!(
        client.try_invest_tranche(&senior_one, &invoice_id, &(TrancheType::Senior as u32), &1),
        Err(Ok(Error::QuotaExceeded))
    );

    client.invest_tranche(&junior, &invoice_id, &(TrancheType::Junior as u32), &30);
    let status = client.get_tranche_status(&invoice_id);
    assert_eq!(status.target_amount, 100);
    assert_eq!(status.senior_capacity, 70);
    assert_eq!(status.junior_capacity, 30);
    assert_eq!(status.senior_filled, 70);
    assert_eq!(status.junior_filled, 30);
    assert_eq!(
        client.get_investor_tranche_amount(&invoice_id, &senior_one, &(TrancheType::Senior as u32)),
        40
    );
}

#[test]
fn applies_cumulative_capacity_across_repeated_investments() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, invoice_id) = setup(&env, 100, 70);
    let investor = Address::generate(&env);
    let another_investor = Address::generate(&env);

    client.invest_tranche(&investor, &invoice_id, &(TrancheType::Senior as u32), &35);
    client.invest_tranche(
        &another_investor,
        &invoice_id,
        &(TrancheType::Senior as u32),
        &34,
    );
    assert_eq!(
        client.try_invest_tranche(&investor, &invoice_id, &(TrancheType::Senior as u32), &2),
        Err(Ok(Error::QuotaExceeded))
    );

    let status = client.get_tranche_status(&invoice_id);
    assert_eq!(status.senior_filled, 69);
    assert_eq!(
        client.get_investor_tranche_amount(&invoice_id, &investor, &(TrancheType::Senior as u32)),
        35
    );
}

#[test]
fn rounds_senior_capacity_down_and_assigns_remainder_to_junior() {
    for (target, senior_capacity, junior_capacity) in [(1, 0, 1), (2, 1, 1), (3, 2, 1)] {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, invoice_id) = setup(&env, target, 70);
        let status = client.get_tranche_status(&invoice_id);
        assert_eq!(status.senior_capacity, senior_capacity);
        assert_eq!(status.junior_capacity, junior_capacity);
        assert_eq!(status.senior_capacity + status.junior_capacity, target);
    }
}

#[test]
fn rejects_nonpositive_target() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(InvoiceTrancheContract, ());
    let client = InvoiceTrancheContractClient::new(&env, &contract_id);
    let issuer = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[9; 32]);

    assert_eq!(
        client.try_create_tranches(&issuer, &invoice_id, &0, &70, &0),
        Err(Ok(Error::InvalidTrancheConfig))
    );
}
