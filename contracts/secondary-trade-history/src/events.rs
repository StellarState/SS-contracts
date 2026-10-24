#![allow(deprecated)]
//! Events emitted by the secondary market trade history contract.

use soroban_sdk::{BytesN, Env, Symbol};

/// Emitted when the contract is initialized.
pub fn initialized(env: &Env, admin: &soroban_sdk::Address) {
    let topics = (Symbol::new(env, "initialized"),);
    env.events().publish(topics, admin.clone());
}

/// Emitted each time a trade is recorded.
/// Topics: (TradeRecorded, invoice_id); Data: (price, amount, timestamp).
pub fn trade_recorded(
    env: &Env,
    invoice_id: &BytesN<32>,
    price: i128,
    amount: i128,
    timestamp: u64,
) {
    let topics = (Symbol::new(env, "TradeRecorded"), invoice_id.clone());
    env.events().publish(topics, (price, amount, timestamp));
}
