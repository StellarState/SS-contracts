use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env, Vec};
use types::RatingStatsV2;

fn setup<'a>(env: &'a Env) -> (InvoiceRatingClient<'a>, Address, Address) {
    env.mock_all_auths();
    let client = InvoiceRatingClient::new(env, &env.register(InvoiceRating, ()));
    let admin = Address::generate(env);
    let operator = Address::generate(env);
    client.initialize(&admin, &operator);
    (client, operator, Address::generate(env))
}

fn settled_invoice(
    env: &Env,
    client: &InvoiceRatingClient,
    operator: &Address,
    issuer: &Address,
    id: u8,
    holders: &[&Address],
) -> BytesN<32> {
    let invoice_id = BytesN::from_array(env, &[id; 32]);
    client.register_invoice(operator, &invoice_id, issuer);
    for holder in holders {
        client.set_fraction_holder(operator, &invoice_id, holder, &true);
    }
    client.mark_settled(operator, &invoice_id);
    invoice_id
}

#[test]
fn rejects_non_holder_and_pre_settlement() {
    let env = Env::default();
    let (client, operator, issuer) = setup(&env);
    let holder = Address::generate(&env);
    let stranger = Address::generate(&env);

    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    client.register_invoice(&operator, &invoice_id, &issuer);
    client.set_fraction_holder(&operator, &invoice_id, &holder, &true);

    assert_eq!(
        client.try_submit_rating(&holder, &invoice_id, &5),
        Err(Ok(Error::InvoiceNotSettled))
    );

    client.mark_settled(&operator, &invoice_id);
    assert_eq!(
        client.try_submit_rating(&stranger, &invoice_id, &5),
        Err(Ok(Error::NotFractionHolder))
    );
    assert_eq!(
        client.try_submit_rating(&holder, &invoice_id, &6),
        Err(Ok(Error::InvalidScore))
    );
    assert_eq!(
        client.try_submit_rating(&holder, &invoice_id, &0),
        Err(Ok(Error::InvalidScore))
    );
}

#[test]
fn rejects_duplicate_rating() {
    let env = Env::default();
    let (client, operator, issuer) = setup(&env);
    let holder = Address::generate(&env);
    let invoice_id = settled_invoice(&env, &client, &operator, &issuer, 1, &[&holder]);

    client.submit_rating(&holder, &invoice_id, &4);
    assert_eq!(
        client.try_submit_rating(&holder, &invoice_id, &5),
        Err(Ok(Error::AlreadyRated))
    );
}

#[test]
fn aggregates_invoice_ratings() {
    let env = Env::default();
    let (client, operator, issuer) = setup(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    let invoice_id = settled_invoice(&env, &client, &operator, &issuer, 1, &[&a, &b, &c]);

    client.submit_rating(&a, &invoice_id, &5);
    client.submit_rating(&b, &invoice_id, &4);

    let partial = client.get_aggregate_rating(&invoice_id);
    assert_eq!(partial.total_ratings, 2);
    assert!(!partial.valid);

    client.submit_rating(&c, &invoice_id, &4);
    let agg = client.get_aggregate_rating(&invoice_id);
    assert_eq!(agg.total_ratings, 3);
    assert_eq!(agg.average_x100, 433);
    assert!((100..=500).contains(&agg.average_x100));
    assert!(agg.valid);
    assert_eq!(agg.distribution.get(3).unwrap(), 2);
    assert_eq!(agg.distribution.get(4).unwrap(), 1);
    assert_eq!(agg.distribution.get(0).unwrap(), 0);
}

#[test]
fn aggregates_issuer_rating_across_invoices() {
    let env = Env::default();
    let (client, operator, issuer) = setup(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let first = settled_invoice(&env, &client, &operator, &issuer, 1, &[&a, &b]);
    let second = settled_invoice(&env, &client, &operator, &issuer, 2, &[&a]);

    client.submit_rating(&a, &first, &5);
    client.submit_rating(&b, &first, &3);
    client.submit_rating(&a, &second, &1);

    let agg = client.get_issuer_rating(&issuer);
    assert_eq!(agg.total_ratings, 3);
    assert_eq!(agg.average_x100, 300);
    assert!(agg.valid);

    // Per-invoice aggregates stay separate.
    assert_eq!(client.get_aggregate_rating(&second).total_ratings, 1);
}

#[test]
fn calculates_large_issuer_average_without_overflow() {
    let env = Env::default();
    let (client, _, issuer) = setup(&env);
    let mut distribution = Vec::new(&env);
    for _ in 0..MAX_SCORE {
        distribution.push_back(0u32);
    }
    distribution.set(4, 10_000_000);
    let stats = RatingStatsV2 {
        total_ratings: 10_000_000,
        score_sum: 50_000_000,
        distribution,
    };
    assert!(stats.score_sum > u64::from(u32::MAX / 100));
    let expected_average_x100 = (stats.score_sum * 100 / u64::from(stats.total_ratings)) as u32;

    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&StorageKey::IssuerStatsV2(issuer.clone()), &stats);
    });

    let aggregate = client.get_issuer_rating(&issuer);
    assert_eq!(aggregate.total_ratings, 10_000_000);
    assert_eq!(aggregate.average_x100, expected_average_x100);
    assert_eq!(aggregate.distribution.get(4).unwrap(), 10_000_000);
    let distribution_total = (0..MAX_SCORE)
        .map(|index| u64::from(aggregate.distribution.get(index).unwrap()))
        .sum::<u64>();
    let distribution_score_sum = (0..MAX_SCORE)
        .map(|index| u64::from(index + 1) * u64::from(aggregate.distribution.get(index).unwrap()))
        .sum::<u64>();
    assert_eq!(distribution_total, u64::from(aggregate.total_ratings));
    assert_eq!(distribution_score_sum, stats.score_sum);
}

#[test]
fn reads_legacy_stats_and_upgrades_on_the_next_rating() {
    let env = Env::default();
    let (client, operator, issuer) = setup(&env);
    let holder = Address::generate(&env);
    let invoice_id = settled_invoice(&env, &client, &operator, &issuer, 1, &[&holder]);
    let mut distribution = Vec::new(&env);
    for _ in 0..MAX_SCORE {
        distribution.push_back(0u32);
    }
    distribution.set(3, 3);
    let legacy_stats = RatingStats {
        total_ratings: 3,
        score_sum: 12,
        distribution,
    };

    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&StorageKey::IssuerStats(issuer.clone()), &legacy_stats);
    });

    let legacy_aggregate = client.get_issuer_rating(&issuer);
    assert_eq!(legacy_aggregate.average_x100, 400);
    assert_eq!(legacy_aggregate.total_ratings, 3);

    client.submit_rating(&holder, &invoice_id, &5);

    let upgraded_aggregate = client.get_issuer_rating(&issuer);
    assert_eq!(upgraded_aggregate.average_x100, 425);
    assert_eq!(upgraded_aggregate.total_ratings, 4);
    assert_eq!(upgraded_aggregate.distribution.get(3).unwrap(), 3);
    assert_eq!(upgraded_aggregate.distribution.get(4).unwrap(), 1);
}

#[test]
fn counter_overflow_rejects_rating_without_changing_related_records() {
    let env = Env::default();
    let (client, operator, issuer) = setup(&env);
    let holder = Address::generate(&env);
    let invoice_id = settled_invoice(&env, &client, &operator, &issuer, 1, &[&holder]);
    let mut distribution = Vec::new(&env);
    for _ in 0..MAX_SCORE {
        distribution.push_back(0u32);
    }
    distribution.set(0, u32::MAX);
    let stats = RatingStatsV2 {
        total_ratings: u32::MAX,
        score_sum: u64::from(u32::MAX),
        distribution,
    };

    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&StorageKey::IssuerStatsV2(issuer.clone()), &stats);
    });

    assert_eq!(
        client.try_submit_rating(&holder, &invoice_id, &1),
        Err(Ok(Error::CounterOverflow))
    );
    assert_eq!(client.get_aggregate_rating(&invoice_id).total_ratings, 0);

    let issuer_aggregate = client.get_issuer_rating(&issuer);
    assert_eq!(issuer_aggregate.total_ratings, u32::MAX);
    assert_eq!(issuer_aggregate.distribution.get(0).unwrap(), u32::MAX);

    // A persisted wallet rating would fail as AlreadyRated instead.
    assert_eq!(
        client.try_submit_rating(&holder, &invoice_id, &1),
        Err(Ok(Error::CounterOverflow))
    );

    let mut distribution = Vec::new(&env);
    for _ in 0..MAX_SCORE {
        distribution.push_back(0u32);
    }
    distribution.set(0, 1);
    let overflowing_sum = RatingStatsV2 {
        total_ratings: 1,
        score_sum: u64::MAX,
        distribution,
    };
    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&StorageKey::IssuerStatsV2(issuer.clone()), &overflowing_sum);
    });
    assert_eq!(
        client.try_submit_rating(&holder, &invoice_id, &1),
        Err(Ok(Error::CounterOverflow))
    );
    assert_eq!(client.get_aggregate_rating(&invoice_id).total_ratings, 0);
}
