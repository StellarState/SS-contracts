#![cfg(test)]
#![allow(deprecated)]

use super::*;
use soroban_sdk::{
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient},
    Address, BytesN, Env, Vec,
};

struct Fixture {
    env: Env,
    client: InvoiceSettlementClient<'static>,
    issuer: Address,
    payer: Address,
    invoice_id: BytesN<32>,
    evidence_hash: BytesN<32>,
    payment_token: TokenClient<'static>,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(InvoiceSettlement, ());
    let client = InvoiceSettlementClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.initialize(&admin, &treasury, &0);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let payment_token = TokenClient::new(&env, &token_id.address());
    let payer = Address::generate(&env);
    StellarAssetClient::new(&env, &token_id.address()).mint(&payer, &10_000);
    let issuer = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let evidence_hash = BytesN::from_array(&env, &[2; 32]);

    Fixture {
        env,
        client,
        issuer,
        payer,
        invoice_id,
        evidence_hash,
        payment_token,
    }
}

fn register(f: &Fixture, invoice_id: &BytesN<32>, outstanding: i128) {
    let mut holders = Vec::new(&f.env);
    holders.push_back(HolderShare {
        wallet: Address::generate(&f.env),
        fractions: 1,
    });
    f.client.register_invoice(
        invoice_id,
        &f.issuer,
        &f.payment_token.address,
        &outstanding,
        &holders,
    );
}

#[test]
fn settlement_requires_matching_evidence_payer_and_full_outstanding_amount() {
    let f = setup();
    register(&f, &f.invoice_id, 1_000);
    f.client
        .record_settlement_evidence(&f.invoice_id, &f.payer, &f.evidence_hash);

    let wrong_hash = BytesN::from_array(&f.env, &[3; 32]);
    assert_eq!(
        f.client
            .try_repay(&f.invoice_id, &1_000, &f.payer, &wrong_hash),
        Err(Ok(Error::EvidenceMismatch))
    );
    assert_eq!(
        f.client
            .try_repay(&f.invoice_id, &999, &f.payer, &f.evidence_hash),
        Err(Ok(Error::InvalidAmount))
    );
    let other_payer = Address::generate(&f.env);
    assert_eq!(
        f.client
            .try_repay(&f.invoice_id, &1_000, &other_payer, &f.evidence_hash),
        Err(Ok(Error::EvidenceMismatch))
    );
    assert_eq!(f.payment_token.balance(&f.payer), 10_000);

    f.client
        .repay(&f.invoice_id, &1_000, &f.payer, &f.evidence_hash);
    let invoice = f.client.get_invoice(&f.invoice_id);
    assert_eq!(invoice.payer, Some(f.payer.clone()));
    assert_eq!(invoice.settlement_evidence_hash, Some(f.evidence_hash));
    assert_eq!(invoice.outstanding_balance, 0);
    assert_eq!(invoice.repaid_amount, 1_000);
    assert_eq!(f.payment_token.balance(&f.payer), 9_000);
}

#[test]
fn settlement_evidence_must_be_nonzero_and_cannot_be_reused() {
    let f = setup();
    register(&f, &f.invoice_id, 1_000);
    let zero_hash = BytesN::from_array(&f.env, &[0; 32]);
    assert_eq!(
        f.client
            .try_record_settlement_evidence(&f.invoice_id, &f.payer, &zero_hash),
        Err(Ok(Error::InvalidEvidence))
    );

    f.client
        .record_settlement_evidence(&f.invoice_id, &f.payer, &f.evidence_hash);
    let other_invoice = BytesN::from_array(&f.env, &[4; 32]);
    register(&f, &other_invoice, 500);
    assert_eq!(
        f.client
            .try_record_settlement_evidence(&other_invoice, &f.payer, &f.evidence_hash),
        Err(Ok(Error::EvidenceAlreadyUsed))
    );
}
