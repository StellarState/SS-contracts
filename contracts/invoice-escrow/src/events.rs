//! Event definitions for state changes (escrow_created, escrow_funded, payment_settled).

use soroban_sdk::{Address, Env, Symbol};

/// Publish escrow_created event.
pub fn escrow_created(
    env: &Env,
    inv_id: Symbol,
    seller: &Address,
    amount: i128,
    due_dt: u64,
    token: &Address,
    inv_token: &Address,
) {
    env.events().publish(
        (Symbol::new(env, "escrow_created"),),
        (inv_id.clone(), seller, amount, due_dt, token, inv_token),
    );
}

/// Publish escrow_funded event.
pub fn escrow_funded(env: &Env, inv_id: Symbol, funder: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "escrow_funded"),),
        (inv_id, funder, amount),
    );
}

/// Publish payment_settled event (amount, platform_fee, investor_amount).
pub fn payment_settled(
    env: &Env,
    inv_id: Symbol,
    amount: i128,
    platform_fee: i128,
    investor_amount: i128,
) {
    env.events().publish(
        (Symbol::new(env, "payment_settled"),),
        (inv_id, amount, platform_fee, investor_amount),
    );
}

/// Publish refund event.
pub fn escrow_refunded(env: &Env, inv_id: Symbol, funder: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "escrow_refunded"),),
        (inv_id, funder, amount),
    );
}
