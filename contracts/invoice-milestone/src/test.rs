#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::{Client as TokenClient, StellarAssetClient},
    BytesN, Env, Vec,
};

fn setup() -> (
    Env,
    InvoiceMilestoneClient<'static>,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(InvoiceMilestone, ());
    let client = InvoiceMilestoneClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let issuer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let token_client = TokenClient::new(&env, &token_id.address());
    StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &10_000);
    client.initialize(&admin, &token_client.address);

    (env, client, admin, issuer, token_client.address)
}

fn invoice_id(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[1u8; 32])
}

fn make_milestones(env: &Env) -> Vec<Milestone> {
    let mut milestones = Vec::new(env);
    milestones.push_back(Milestone {
        amount: 500,
        description_hash: BytesN::from_array(env, &[10u8; 32]),
        evidence_expiry_ledger: 100,
        verified: false,
    });
    milestones.push_back(Milestone {
        amount: 300,
        description_hash: BytesN::from_array(env, &[11u8; 32]),
        evidence_expiry_ledger: 100,
        verified: false,
    });
    milestones
}

#[test]
fn test_create_and_verify_milestone() {
    let (env, client, _admin, issuer, _token) = setup();
    let inv = invoice_id(&env);
    client.create_milestones(&inv, &issuer, &make_milestones(&env));

    let statuses = client.get_milestone_status(&inv);
    assert_eq!(statuses.len(), 2);
    assert!(!statuses.get(0).unwrap().verified);

    client.verify_milestone(&inv, &0);
    let statuses = client.get_milestone_status(&inv);
    assert!(statuses.get(0).unwrap().verified);
    assert!(!statuses.get(1).unwrap().verified);
}

#[test]
fn test_double_verify_rejected() {
    let (env, client, _admin, issuer, _token) = setup();
    let inv = invoice_id(&env);
    client.create_milestones(&inv, &issuer, &make_milestones(&env));
    client.verify_milestone(&inv, &0);
    assert_eq!(
        client.try_verify_milestone(&inv, &0),
        Err(Ok(Error::AlreadyVerified))
    );
}

#[test]
fn test_invalid_index_rejected() {
    let (env, client, _admin, issuer, _token) = setup();
    let inv = invoice_id(&env);
    client.create_milestones(&inv, &issuer, &make_milestones(&env));
    assert_eq!(
        client.try_verify_milestone(&inv, &99),
        Err(Ok(Error::InvalidIndex))
    );
}

#[test]
fn test_duplicate_milestones_rejected() {
    let (env, client, _admin, issuer, _token) = setup();
    let inv = invoice_id(&env);
    let milestones = make_milestones(&env);
    client.create_milestones(&inv, &issuer, &milestones);
    assert_eq!(
        client.try_create_milestones(&inv, &issuer, &milestones),
        Err(Ok(Error::MilestonesAlreadySet))
    );
}

#[test]
fn expired_evidence_blocks_payment_until_renewed() {
    let (env, client, admin, issuer, _token) = setup();
    let inv = invoice_id(&env);
    client.create_milestones(&inv, &issuer, &make_milestones(&env));
    let at_expiry = client.get_milestone_status(&inv).get(0).unwrap();
    assert!(at_expiry.evidence_valid);
    assert_eq!(at_expiry.remaining_ledger_life, 100);
    env.ledger().set_sequence_number(101);

    let status = client.get_milestone_status(&inv).get(0).unwrap();
    assert!(!status.evidence_valid);
    assert_eq!(status.remaining_ledger_life, 0);
    assert_eq!(
        client.try_verify_milestone(&inv, &0),
        Err(Ok(Error::EvidenceExpired))
    );
    client.report_expired_evidence(&admin, &inv, &0);
    assert_eq!(
        client.try_report_expired_evidence(&admin, &inv, &0),
        Err(Ok(Error::EvidenceAlreadyReported))
    );

    let renewed_hash = BytesN::from_array(&env, &[20; 32]);
    client.renew_evidence(&admin, &inv, &0, &renewed_hash, &120);
    let status = client.get_milestone_status(&inv).get(0).unwrap();
    assert!(status.evidence_valid);
    assert_eq!(status.remaining_ledger_life, 19);
    client.verify_milestone(&inv, &0);
}

#[test]
fn renewal_requires_admin_and_non_expired_ledger_boundary() {
    let (env, client, admin, issuer, _token) = setup();
    let inv = invoice_id(&env);
    client.create_milestones(&inv, &issuer, &make_milestones(&env));
    env.ledger().set_sequence_number(1);
    let outsider = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[21; 32]);
    assert_eq!(
        client.try_renew_evidence(&outsider, &inv, &0, &hash, &110),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        client.try_renew_evidence(&admin, &inv, &0, &hash, &0),
        Err(Ok(Error::InvalidExpiry))
    );
}
