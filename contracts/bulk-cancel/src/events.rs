use soroban_sdk::{symbol_short, Address, Env};

pub fn emit_bulk_orders_cancelled(
    env: &Env,
    wallet: &Address,
    count: u32,
    total_refunded: i128,
) {
    env.events().publish(
        (symbol_short!("bulk_can"), wallet.clone()),
        (count, total_refunded),
    );
}
