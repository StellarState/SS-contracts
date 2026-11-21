#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Env};

fn setup() -> (Env, CreditScoringClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CreditScoring, ());
    let client = CreditScoringClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    (env, client, admin)
}

fn high_quality_factors(_env: &Env) -> IssuerFactors {
    IssuerFactors {
        kyc_tier: 3,
        settlement_history: 10,
        default_count: 0,
        platform_tenure_days: 300,
    }
}

fn low_quality_factors(_env: &Env) -> IssuerFactors {
    IssuerFactors {
        kyc_tier: 0,
        settlement_history: 0,
        default_count: 3,
        platform_tenure_days: 0,
    }
}

fn widened_reference_score_and_tier(factors: &IssuerFactors) -> (u32, CreditTier) {
    let kyc_pts = u128::from(factors.kyc_tier.min(3)) * 20;
    let settlement_pts = u128::from(factors.settlement_history.min(10)) * 5;
    let tenure_pts = u128::from((factors.platform_tenure_days / 10).min(30));
    let default_penalty = u128::from(factors.default_count) * 25;
    let raw = kyc_pts + settlement_pts + tenure_pts;
    let score = raw.saturating_sub(default_penalty) as u32;
    (score, tier_from_score(score))
}

#[test]
fn test_aaa_tier_for_high_quality_issuer() {
    let (env, client, admin) = setup();
    let issuer = Address::generate(&env);
    let score = client.compute_credit_score(&admin, &issuer, &high_quality_factors(&env));
    assert_eq!(score.tier, CreditTier::AAA);
    assert!(score.score >= 90);
}

#[test]
fn test_c_tier_for_defaulted_issuer() {
    let (env, client, admin) = setup();
    let issuer = Address::generate(&env);
    let score = client.compute_credit_score(&admin, &issuer, &low_quality_factors(&env));
    assert_eq!(score.tier, CreditTier::C);
}

#[test]
fn test_get_credit_score_returns_stored_value() {
    let (env, client, admin) = setup();
    let issuer = Address::generate(&env);
    let factors = high_quality_factors(&env);
    let computed = client.compute_credit_score(&admin, &issuer, &factors);
    let fetched = client.get_credit_score(&issuer);
    assert_eq!(computed.score, fetched.score);
    assert_eq!(computed.tier, fetched.tier);
}

#[test]
fn test_unauthorised_caller_rejected() {
    let (env, client, _admin) = setup();
    let issuer = Address::generate(&env);
    let stranger = Address::generate(&env);
    assert_eq!(
        client.try_compute_credit_score(&stranger, &issuer, &high_quality_factors(&env)),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn test_authorised_contract_can_compute() {
    let (env, client, _admin) = setup();
    let authorised = Address::generate(&env);
    let issuer = Address::generate(&env);
    client.set_authorised_caller(&authorised, &true);
    let score = client.compute_credit_score(&authorised, &issuer, &high_quality_factors(&env));
    assert!(score.score > 0);
}

#[test]
fn test_score_factors_breakdown_accurate() {
    let (env, client, admin) = setup();
    let issuer = Address::generate(&env);
    let factors = IssuerFactors {
        kyc_tier: 2,
        settlement_history: 5,
        default_count: 1,
        platform_tenure_days: 100,
    };
    // kyc=40, settlement=25, tenure=10, default_penalty=25 -> 50
    let score = client.compute_credit_score(&admin, &issuer, &factors);
    assert_eq!(score.score, 50);
    assert_eq!(score.tier, CreditTier::B);
}

#[test]
fn test_downgrade_grace_period_and_renewal_cancellation() {
    use soroban_sdk::testutils::Ledger as _;

    let (env, client, admin) = setup();
    let issuer = Address::generate(&env);

    client.set_downgrade_grace_ledgers(&admin, &10);
    client.compute_credit_score(&admin, &issuer, &high_quality_factors(&env));

    // Propose downgrade -> stored as pending, active score remains AAA
    let pending = client.propose_downgrade(&admin, &issuer, &low_quality_factors(&env));
    assert!(pending.is_some());
    assert_eq!(client.get_credit_score(&issuer).tier, CreditTier::AAA);

    // Before grace expires, execution fails
    assert_eq!(
        client.try_execute_pending_downgrade(&admin, &issuer),
        Err(Ok(Error::GracePeriodNotExpired))
    );

    // Authorized renewal cancels pending downgrade
    client.renew_and_cancel_downgrade(&admin, &issuer);
    assert!(client.get_pending_downgrade(&issuer).is_none());

    // Propose again and advance ledger past grace boundary -> executes once
    client.propose_downgrade(&admin, &issuer, &low_quality_factors(&env));
    env.ledger()
        .set_sequence_number(env.ledger().sequence() + 10);
    let updated = client.execute_pending_downgrade(&admin, &issuer);
    assert_eq!(updated.tier, CreditTier::C);
    assert_eq!(client.get_credit_score(&issuer).tier, CreditTier::C);
}

#[test]
fn test_propose_downgrade_with_extreme_default_count_safe() {
    use soroban_sdk::testutils::Ledger as _;

    let (env, client, admin) = setup();
    let issuer = Address::generate(&env);

    client.set_downgrade_grace_ledgers(&admin, &5);
    client.compute_credit_score(&admin, &issuer, &high_quality_factors(&env));

    let extreme_defaults = IssuerFactors {
        kyc_tier: 3,
        settlement_history: 10,
        default_count: u32::MAX,
        platform_tenure_days: 300,
    };
    let pending = client
        .propose_downgrade(&admin, &issuer, &extreme_defaults)
        .expect("extreme default_count must propose downgrade to Tier C without wrapping");
    assert_eq!(pending.proposed_score.score, 0);
    assert_eq!(pending.proposed_score.tier, CreditTier::C);

    env.ledger()
        .set_sequence_number(env.ledger().sequence() + 5);
    let updated = client.execute_pending_downgrade(&admin, &issuer);
    assert_eq!(updated.score, 0);
    assert_eq!(updated.tier, CreditTier::C);
}

#[test]
fn test_default_count_u32_max_produces_zero_and_tier_c() {
    let (env, client, admin) = setup();
    let issuer = Address::generate(&env);
    let factors = IssuerFactors {
        kyc_tier: 3,
        settlement_history: 10,
        default_count: u32::MAX,
        platform_tenure_days: 300,
    };
    let score = client.compute_credit_score(&admin, &issuer, &factors);
    assert_eq!(score.score, 0);
    assert_eq!(score.tier, CreditTier::C);

    let stored = client.get_credit_score(&issuer);
    assert_eq!(stored.score, 0);
    assert_eq!(stored.tier, CreditTier::C);
    assert_eq!(stored.factors.default_count, u32::MAX);
}

#[test]
fn test_values_around_u32_max_div_25_cannot_wrap_into_better_scores() {
    let (env, client, admin) = setup();
    let boundary = u32::MAX / 25;
    let candidates = [
        boundary - 2,
        boundary - 1,
        boundary,
        boundary + 1,
        boundary + 2,
        boundary.saturating_mul(2) + 1,
        u32::MAX - 1,
        u32::MAX,
    ];

    for default_count in candidates {
        let issuer = Address::generate(&env);
        let factors = IssuerFactors {
            kyc_tier: 3,
            settlement_history: 10,
            default_count,
            platform_tenure_days: 300,
        };
        let computed = client.compute_credit_score(&admin, &issuer, &factors);
        assert_eq!(computed.score, 0);
        assert_eq!(computed.tier, CreditTier::C);
    }
}

#[test]
fn test_increasing_default_count_never_increases_score_or_tier() {
    let (env, client, admin) = setup();
    let boundary = u32::MAX / 25;
    let default_steps = [
        0,
        1,
        2,
        3,
        4,
        5,
        6,
        10,
        1_000,
        boundary - 1,
        boundary,
        boundary + 1,
        u32::MAX - 1,
        u32::MAX,
    ];
    let profiles = [(3, 10, 300), (2, 5, 100), (1, 2, 20), (0, 0, 0)];

    for (kyc_tier, settlement_history, platform_tenure_days) in profiles {
        let issuer = Address::generate(&env);
        let mut prev_score = u32::MAX;
        let mut prev_tier = CreditTier::AAA;

        for default_count in default_steps {
            let factors = IssuerFactors {
                kyc_tier,
                settlement_history,
                default_count,
                platform_tenure_days,
            };
            let current = client.compute_credit_score(&admin, &issuer, &factors);
            assert!(current.score <= prev_score);
            assert!(current.tier <= prev_tier);
            prev_score = current.score;
            prev_tier = current.tier;
        }
    }
}

#[test]
fn test_unauthorised_extreme_default_call_preserves_stored_score() {
    let (env, client, admin) = setup();
    let issuer = Address::generate(&env);
    let stranger = Address::generate(&env);

    let initial = client.compute_credit_score(&admin, &issuer, &high_quality_factors(&env));
    assert_eq!(initial.score, 140);
    assert_eq!(initial.tier, CreditTier::AAA);

    let extreme_defaults = IssuerFactors {
        kyc_tier: 3,
        settlement_history: 10,
        default_count: u32::MAX,
        platform_tenure_days: 300,
    };
    assert_eq!(
        client.try_compute_credit_score(&stranger, &issuer, &extreme_defaults),
        Err(Ok(Error::Unauthorized))
    );

    let persisted = client.get_credit_score(&issuer);
    assert_eq!(persisted.score, initial.score);
    assert_eq!(persisted.tier, initial.tier);
    assert_eq!(persisted.factors.default_count, 0);
}

#[test]
fn test_boundary_and_property_samples_match_widened_arithmetic() {
    let (env, client, admin) = setup();
    let issuer = Address::generate(&env);
    let boundary = u32::MAX / 25;

    let kyc_vals = [0, 1, 2, 3, 7, u32::MAX];
    let settlement_vals = [0, 1, 5, 10, 25, u32::MAX];
    let tenure_vals = [0, 9, 10, 155, 300, 999, u32::MAX];
    let default_vals = [
        0,
        1,
        2,
        3,
        4,
        5,
        6,
        boundary - 1,
        boundary,
        boundary + 1,
        boundary.saturating_mul(3) + 7,
        u32::MAX - 1,
        u32::MAX,
    ];

    for kyc_tier in kyc_vals {
        for settlement_history in settlement_vals {
            for &platform_tenure_days in &tenure_vals[..3] {
                for default_count in default_vals {
                    let factors = IssuerFactors {
                        kyc_tier,
                        settlement_history,
                        default_count,
                        platform_tenure_days,
                    };
                    let (expected_score, expected_tier) =
                        widened_reference_score_and_tier(&factors);
                    let actual_score = compute_score_internal(&factors);
                    assert_eq!(actual_score, expected_score);
                    assert_eq!(tier_from_score(actual_score), expected_tier);
                }
            }
        }
    }

    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..64 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let kyc_tier = (state & 0x7) as u32;
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let settlement_history = (state & 0x1F) as u32;
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let default_count = (state >> 32) as u32;
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let platform_tenure_days = (state & 0x3FF) as u32;

        let factors = IssuerFactors {
            kyc_tier,
            settlement_history,
            default_count,
            platform_tenure_days,
        };
        let (expected_score, expected_tier) = widened_reference_score_and_tier(&factors);
        let res = client.compute_credit_score(&admin, &issuer, &factors);
        assert_eq!(res.score, expected_score);
        assert_eq!(res.tier, expected_tier);
    }
}
