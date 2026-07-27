use soroban_sdk::{Address, Env, Symbol, Vec};

use crate::types::FeeTier;

pub fn initialized(env: &Env, admin: &Address) {
    let topics = (Symbol::new(env, "initialized"),);
    env.events().publish(topics, admin.clone());
}

pub fn payment_distributed(
    env: &Env,
    escrow: &Address,
    invoice_id: &Symbol,
    recipients: &Vec<Address>,
    amounts: &Vec<i128>,
) {
    let topics = (
        Symbol::new(env, "payment_distributed"),
        escrow.clone(),
        invoice_id.clone(),
    );
    env.events()
        .publish(topics, (recipients.clone(), amounts.clone()));
}

pub fn refund_distributed(
    env: &Env,
    escrow: &Address,
    invoice_id: &Symbol,
    recipients: &Vec<Address>,
    amounts: &Vec<i128>,
) {
    let topics = (
        Symbol::new(env, "refund_distributed"),
        escrow.clone(),
        invoice_id.clone(),
    );
    env.events()
        .publish(topics, (recipients.clone(), amounts.clone()));
}

pub fn platform_fee_updated(env: &Env, admin: &Address, tiers: &Vec<FeeTier>) {
    let topics = (Symbol::new(env, "platform_fee_updated"),);
    env.events().publish(topics, (admin.clone(), tiers.clone()));
}

pub fn investor_bonus_rate_updated(env: &Env, admin: &Address, bonus_bps: u32) {
    let topics = (Symbol::new(env, "investor_bonus_rate_updated"),);
    env.events().publish(topics, (admin.clone(), bonus_bps));
}
