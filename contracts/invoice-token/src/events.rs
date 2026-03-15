//! Event definitions for SEP-41 token (transfer, approve, mint, burn).

use soroban_sdk::{Address, Env, Symbol};

/// Emit transfer event (SEP-41: topics ["transfer", from, to], data amount).
pub fn transfer_event(env: &Env, from: &Address, to: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "transfer"), from, to), amount);
}

/// Emit approve event (SEP-41: topics ["approve", from, spender], data (amount, expiration_ledger)).
pub fn approve_event(
    env: &Env,
    from: &Address,
    spender: &Address,
    amount: i128,
    expiration_ledger: u32,
) {
    env.events().publish(
        (Symbol::new(env, "approve"), from, spender),
        (amount, expiration_ledger),
    );
}

/// Emit mint event.
pub fn mint_event(env: &Env, to: &Address, amount: i128) {
    env.events().publish((Symbol::new(env, "mint"), to), amount);
}

/// Emit burn event (SEP-41: topics ["burn", from], data amount).
pub fn burn_event(env: &Env, from: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "burn"), from), amount);
}
