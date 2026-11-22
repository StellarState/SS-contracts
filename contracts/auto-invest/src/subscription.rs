#![allow(dead_code)]
//! Core interval-subscription math for issue #647. Not wired into
//! contractimpl yet - storage, create/execute/cancel entry points, balance
//! checks and events are left for follow-up.
use soroban_sdk::Address;

pub struct Subscription {
    pub investor: Address,
    pub pool_id: u64,
    pub amount_per_period: i128,
    pub interval_seconds: u64,
    pub next_execution: u64,
    pub total_invested: i128,
    pub active: bool,
}

pub fn is_due(sub: &Subscription, now: u64) -> bool {
    sub.active && now >= sub.next_execution
}

pub fn advance(sub: &mut Subscription, now: u64) {
    sub.total_invested += sub.amount_per_period;
    sub.next_execution = now + sub.interval_seconds;
}
