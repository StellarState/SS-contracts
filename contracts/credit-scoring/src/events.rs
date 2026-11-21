use crate::types::CreditTier;
use soroban_sdk::{Address, Env, Symbol};

#[allow(deprecated)]
pub fn credit_score_updated(
    env: &Env,
    issuer: &Address,
    old_tier: CreditTier,
    new_tier: CreditTier,
) {
    let topics = (Symbol::new(env, "CreditScoreUpdated"), issuer.clone());
    env.events()
        .publish(topics, (old_tier as u32, new_tier as u32));
}

#[allow(deprecated)]
pub fn downgrade_proposed(
    env: &Env,
    issuer: &Address,
    current_tier: CreditTier,
    proposed_tier: CreditTier,
    execute_after_ledger: u32,
) {
    let topics = (Symbol::new(env, "proposed"), issuer.clone());
    env.events().publish(
        topics,
        (
            current_tier as u32,
            proposed_tier as u32,
            execute_after_ledger,
        ),
    );
}

#[allow(deprecated)]
pub fn downgrade_cancelled(env: &Env, issuer: &Address, renewed_by: &Address) {
    let topics = (Symbol::new(env, "cancelled"), issuer.clone());
    env.events().publish(topics, renewed_by.clone());
}
