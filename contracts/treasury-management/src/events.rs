#![allow(deprecated)]
//! Events emitted by the platform treasury management contract.

use soroban_sdk::{Address, Env, String, Symbol};

/// Emitted when the contract is initialized.
pub fn initialized(env: &Env, admin: &Address) {
    let topics = (Symbol::new(env, "initialized"),);
    env.events().publish(topics, admin.clone());
}

/// Emitted each time a fee source contributes funds.
/// Topics: (FeeReceived, source); Data: amount.
pub fn fee_received(env: &Env, source: &Address, amount: i128) {
    let topics = (Symbol::new(env, "FeeReceived"), source.clone());
    env.events().publish(topics, amount);
}

/// Emitted each time the admin allocates funds out of the treasury.
/// Topics: (FundsAllocated, destination); Data: (amount, purpose).
pub fn funds_allocated(env: &Env, destination: &Address, amount: i128, purpose: &String) {
    let topics = (Symbol::new(env, "FundsAllocated"), destination.clone());
    env.events().publish(topics, (amount, purpose.clone()));
}

/// Emitted when an allocation starts a cooldown window.
pub fn allocation_cooldown_started(
    env: &Env,
    current_ledger: u32,
    cooldown_ledgers: u32,
    available_at_ledger: u32,
) {
    let topics = (Symbol::new(env, "AllocationCooldownStarted"),);
    env.events().publish(
        topics,
        (current_ledger, cooldown_ledgers, available_at_ledger),
    );
}

/// Emitted for every allocation alongside `FundsAllocated` (#825).
/// Topics: (treasury_allocation_receipt,); Data: the full receipt fields.
pub fn treasury_allocation_receipt(
    env: &Env,
    receipt_id: u64,
    allocator: &Address,
    destination: &Address,
    amount: i128,
    purpose: &String,
    ledger: u32,
    ledger_timestamp: u64,
    treasury_revision: u64,
) {
    let topics = (Symbol::new(env, "treasury_allocation_receipt"),);
    env.events().publish(
        topics,
        (
            receipt_id,
            allocator.clone(),
            destination.clone(),
            amount,
            purpose.clone(),
            ledger,
            ledger_timestamp,
            treasury_revision,
        ),
    );
}

/// Emitted once per ledger day when cumulative allocations first reach the
/// configured daily limit (#824). Fired from the committed allocation path —
/// a rejected call rolls back its events — so it marks the start of the
/// day's breach window rather than every rejected attempt.
pub fn daily_treasury_limit_reached(
    env: &Env,
    ledger_day: u64,
    limit: i128,
    attempted_total: i128,
) {
    let topics = (Symbol::new(env, "daily_treasury_limit_reached"),);
    env.events()
        .publish(topics, (ledger_day, limit, attempted_total));
}

/// Emitted when a fee receipt is rejected due to allowlist or cap controls.
pub fn source_rejected(env: &Env, source: &Address, amount: i128, reason: &Symbol) {
    let topics = (Symbol::new(env, "source_rejected"), source.clone());
    env.events().publish(topics, (amount, reason.clone()));
}

/// Emitted when a fee receipt passes source allowlist and cap checks.
pub fn source_fee_received(
    env: &Env,
    source: &Address,
    amount: i128,
    daily_total: i128,
    cumulative_total: i128,
) {
    let topics = (Symbol::new(env, "source_fee_received"), source.clone());
    env.events()
        .publish(topics, (amount, daily_total, cumulative_total));
}
