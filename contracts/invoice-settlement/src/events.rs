#![allow(deprecated)]

use soroban_sdk::{Address, BytesN, Env, Symbol};

pub fn repayment_received(env: &Env, invoice_id: &BytesN<32>, issuer: &Address, amount: i128) {
    let topics = (Symbol::new(env, "RepaymentReceived"), invoice_id.clone());
    env.events()
        .publish(topics, (issuer.clone(), amount, env.ledger().timestamp()));
}

pub fn settlement_complete(
    env: &Env,
    invoice_id: &BytesN<32>,
    distributed: i128,
    fee: i128,
    total: i128,
) {
    let topics = (Symbol::new(env, "SettlementComplete"), invoice_id.clone());
    env.events()
        .publish(topics, (distributed, fee, total, env.ledger().timestamp()));
}

pub fn settlement_evidenced(
    env: &Env,
    invoice_id: &BytesN<32>,
    payer: &Address,
    evidence_hash: &BytesN<32>,
) {
    let topics = (Symbol::new(env, "settlement_evidenced"), invoice_id.clone());
    env.events()
        .publish(topics, (payer.clone(), evidence_hash.clone()));
}

pub fn settle_with_evidence(
    env: &Env,
    invoice_id: &BytesN<32>,
    payer: &Address,
    evidence_hash: &BytesN<32>,
    amount: i128,
) {
    let topics = (Symbol::new(env, "settle_with_evidence"), invoice_id.clone());
    env.events().publish(
        topics,
        (
            payer.clone(),
            evidence_hash.clone(),
            amount,
            env.ledger().timestamp(),
        ),
    );
}
