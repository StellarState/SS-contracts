use soroban_sdk::{BytesN, Env, Symbol};

pub fn rating_recorded(env: &Env, invoice_id: &BytesN<32>, average_x100: u32, total_ratings: u32) {
    let topics = (Symbol::new(env, "RatingRecorded"), invoice_id.clone());
    env.events().publish(topics, (average_x100, total_ratings));
}

pub fn yield_adjusted_at_settlement(
    env: &Env,
    invoice_id: &BytesN<32>,
    base_yield_bps: i128,
    adjustment_bps: i32,
    final_yield_bps: i128,
) {
    let topics = (
        Symbol::new(env, "YieldAdjustedAtSettlement"),
        invoice_id.clone(),
    );
    env.events().publish(
        topics,
        (base_yield_bps, adjustment_bps, final_yield_bps),
    );
}
