//! Event definitions for state changes (escrow_created, escrow_funded, payment_settled).

use soroban_sdk::{Address, Env, Symbol};

use crate::types::{EscrowStatus, InvoiceCategory};

/// Publish a lifecycle transition event carrying the new status and ledger
/// timestamp. Lets off-chain indexers reconstruct full escrow lifecycle
/// history from a single event stream instead of correlating multiple events.
pub fn escrow_status_changed(env: &Env, inv_id: Symbol, status: EscrowStatus, timestamp: u64) {
    env.events().publish(
        (Symbol::new(env, "escrow_status_changed"),),
        (inv_id, status as u32, timestamp),
    );
}

/// Publish escrow_created event.
pub fn escrow_created(
    env: &Env,
    inv_id: Symbol,
    seller: &Address,
    debtor: &Address,
    face_value: i128,
    purchase_price: i128,
    due_dt: u64,
    token: &Address,
    inv_token: &Address,
    commitment: &soroban_sdk::BytesN<32>,
    category: InvoiceCategory,
    effective_fee_bps: u32,
) {
    env.events().publish(
        (Symbol::new(env, "escrow_created"),),
        (
            inv_id.clone(),
            seller,
            debtor,
            face_value,
            purchase_price,
            due_dt,
            token,
            inv_token,
            commitment,
            category as u32,
            effective_fee_bps,
        ),
    );
}

/// Publish escrow_funded event with partial funding info.
pub fn escrow_funded(
    env: &Env,
    inv_id: Symbol,
    funder: &Address,
    amount: i128,
    funded_amt: i128,
    purchase_price: i128,
) {
    env.events().publish(
        (Symbol::new(env, "escrow_funded"),),
        (inv_id, funder, amount, funded_amt, purchase_price),
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
pub fn escrow_refunded(env: &Env, inv_id: Symbol, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "escrow_refunded"),), (inv_id, amount));
}

/// Publish escrow_cancelled event (invoice_id, seller).
pub fn escrow_cancelled(env: &Env, inv_id: Symbol, seller: &Address) {
    env.events()
        .publish((Symbol::new(env, "escrow_cancelled"),), (inv_id, seller));
}

/// Publish platform fee update event with old and new basis points.
pub fn platform_fee_updated(env: &Env, old_fee_bps: u32, new_fee_bps: u32) {
    env.events().publish(
        (Symbol::new(env, "platform_fee_updated"),),
        (old_fee_bps, new_fee_bps),
    );
}

/// Publish payment distributor update event.
pub fn payment_distributor_updated(
    env: &Env,
    had_previous_distributor: bool,
    new_distributor: &Address,
) {
    env.events().publish(
        (
            Symbol::new(env, "distributor_updated"),
            new_distributor.clone(),
        ),
        had_previous_distributor,
    );
}

/// Publish pause state updates.
pub fn paused_updated(env: &Env, old_paused: bool, new_paused: bool) {
    env.events().publish(
        (Symbol::new(env, "paused_updated"),),
        (old_paused, new_paused),
    );
}

/// Publish category fee schedule set/updated event.
///
/// `old_fee_bps` is `None` when this is the first time a schedule is set for
/// this category, `Some(old)` when overwriting an existing schedule.
pub fn category_fee_set(
    env: &Env,
    category: InvoiceCategory,
    old_fee_bps: Option<u32>,
    new_fee_bps: u32,
) {
    env.events().publish(
        (Symbol::new(env, "cat_fee_set"),),
        (category as u32, old_fee_bps, new_fee_bps),
    );
}
