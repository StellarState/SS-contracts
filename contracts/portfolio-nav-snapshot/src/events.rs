use soroban_sdk::{symbol_short, Address, Env, Symbol};

pub fn portfolio_snapshot_taken(env: &Env, wallet: &Address, date: u64, nav_amount: i128) {
    env.events().publish(
        (symbol_short!("nav_snap"), wallet.clone()),
        (date, nav_amount),
    );
}

/// Emitted when a NAV snapshot passes verification against the outstanding
/// supply and reserve oracle inputs (#817).
pub fn nav_snapshot_verified(
    env: &Env,
    wallet: &Address,
    date: u64,
    nav_amount: i128,
    outstanding_supply: i128,
    reserve: i128,
) {
    env.events().publish(
        (Symbol::new(env, "nav_snapshot_verified"), wallet.clone()),
        (date, nav_amount, outstanding_supply, reserve),
    );
}

/// Emitted when a NAV snapshot fails verification (#817). `reason` is a
/// short symbol such as `oracle_not_set`, `stale_oracle` or
/// `nav_mismatch`.
pub fn nav_snapshot_rejected(
    env: &Env,
    wallet: &Address,
    date: u64,
    nav_amount: i128,
    outstanding_supply: i128,
    reserve: i128,
    reason: &Symbol,
) {
    env.events().publish(
        (Symbol::new(env, "nav_snapshot_rejected"), wallet.clone()),
        (date, nav_amount, outstanding_supply, reserve, reason.clone()),
    );
}
