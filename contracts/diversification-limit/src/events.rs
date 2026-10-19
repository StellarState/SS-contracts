use soroban_sdk::{Address, BytesN, Env, Symbol};

pub fn diversification_limit_set(env: &Env, wallet: &Address, limit_bps: u32) {
    let topics = (Symbol::new(env, "DiversificationLimitSet"), wallet.clone());
    env.events().publish(topics, limit_bps);
}

pub fn invested(
    env: &Env,
    wallet: &Address,
    invoice_id: &BytesN<32>,
    amount: i128,
    projected_bps: u32,
) {
    let topics = (
        Symbol::new(env, "Invested"),
        wallet.clone(),
        invoice_id.clone(),
    );
    env.events().publish(topics, (amount, projected_bps));
}

pub fn diversification_limit_exceeded(
    env: &Env,
    wallet: &Address,
    invoice_id: &BytesN<32>,
    current_bps: u32,
    projected_bps: u32,
    max_bps: u32,
) {
    let topics = (
        Symbol::new(env, "DiversificationLimitExceeded"),
        wallet.clone(),
        invoice_id.clone(),
    );
    env.events()
        .publish(topics, (current_bps, projected_bps, max_bps));
}
