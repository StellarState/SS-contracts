//! Invoice credit scoring contract for issuer risk tiering (issue #660).
//!
//! Assigns issuers a credit tier (C/B/A/AA/AAA) based on KYC tier,
//! settlement history, default count, and platform tenure.
//! Tier determines maximum invoice size and yield rate floor.

#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
pub use types::{CreditScore, CreditTier, IssuerFactors, PendingDowngrade, StorageKey};

use soroban_sdk::{contract, contractimpl, Address, Env};

/// Scoring weights (points per unit):
///  kyc_tier (0-3)            × 20 = up to  60 pts
///  settlement_history (0-N)  × 5  = up to  50 pts (capped at 10)
///  default_count             × -25 = penalty
///  platform_tenure_days      × 0.1 (1pt per 10 days, capped at 30 pts)
fn tier_from_score(score: u32) -> CreditTier {
    match score {
        0..=39 => CreditTier::C,
        40..=59 => CreditTier::B,
        60..=74 => CreditTier::A,
        75..=89 => CreditTier::AA,
        _ => CreditTier::AAA,
    }
}

fn compute_score_internal(factors: &IssuerFactors) -> u32 {
    let kyc_pts = factors.kyc_tier.min(3) * 20;

    let settlement_pts = factors.settlement_history.min(10) * 5;

    let default_penalty = factors.default_count.saturating_mul(25);

    let tenure_pts = (factors.platform_tenure_days / 10).min(30);

    let raw = kyc_pts + settlement_pts + tenure_pts;
    raw.saturating_sub(default_penalty)
}

fn require_authorised(env: &Env, caller: &Address) -> Result<(), Error> {
    // The caller must be either the admin or a registered authorised contract.
    let admin: Address = env
        .storage()
        .instance()
        .get(&StorageKey::Admin)
        .ok_or(Error::NotInitialized)?;

    let is_admin = *caller == admin;
    let is_authorised: bool = env
        .storage()
        .persistent()
        .get(&StorageKey::AuthorisedCaller(caller.clone()))
        .unwrap_or(false);

    if !is_admin && !is_authorised {
        return Err(Error::Unauthorized);
    }
    caller.require_auth();
    Ok(())
}

#[contract]
pub struct CreditScoring;

#[contractimpl]
impl CreditScoring {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&StorageKey::Admin, &admin);
        Ok(())
    }

    /// Admin-only: register a contract address as authorised to call
    /// `compute_credit_score`.
    pub fn set_authorised_caller(env: Env, caller: Address, authorised: bool) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&StorageKey::AuthorisedCaller(caller), &authorised);
        Ok(())
    }

    /// Authorised-caller only: compute and persist an issuer's credit score.
    pub fn compute_credit_score(
        env: Env,
        caller: Address,
        issuer: Address,
        factors: IssuerFactors,
    ) -> Result<CreditScore, Error> {
        require_authorised(&env, &caller)?;

        let score = compute_score_internal(&factors);
        let tier = tier_from_score(score);

        let old_tier: CreditTier = env
            .storage()
            .persistent()
            .get(&StorageKey::Score(issuer.clone()))
            .map(|s: CreditScore| s.tier)
            .unwrap_or(CreditTier::C);

        let credit = CreditScore {
            score,
            tier,
            factors: factors.clone(),
        };

        env.storage()
            .persistent()
            .set(&StorageKey::Score(issuer.clone()), &credit);

        events::credit_score_updated(&env, &issuer, old_tier, tier);

        Ok(credit)
    }

    /// Returns the current credit score and factor breakdown for an issuer.
    pub fn get_credit_score(env: Env, issuer: Address) -> Result<CreditScore, Error> {
        env.storage()
            .persistent()
            .get(&StorageKey::Score(issuer))
            .ok_or(Error::IssuerNotFound)
    }

    /// Admin-only: configure the grace ledger window before a risk downgrade takes effect.
    pub fn set_downgrade_grace_ledgers(
        env: Env,
        admin: Address,
        grace_ledgers: u32,
    ) -> Result<(), Error> {
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();
        env.storage()
            .instance()
            .set(&StorageKey::GracePeriodLedgers, &grace_ledgers);
        Ok(())
    }

    pub fn get_downgrade_grace_ledgers(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&StorageKey::GracePeriodLedgers)
            .unwrap_or(0)
    }

    /// Propose a risk downgrade. If a grace period is configured and the new tier is
    /// lower than the active tier, stores a pending downgrade instead of mutating
    /// the active rating immediately.
    pub fn propose_downgrade(
        env: Env,
        caller: Address,
        issuer: Address,
        factors: IssuerFactors,
    ) -> Result<Option<PendingDowngrade>, Error> {
        require_authorised(&env, &caller)?;

        let score = compute_score_internal(&factors);
        let tier = tier_from_score(score);
        let proposed_score = CreditScore {
            score,
            tier,
            factors,
        };

        let current = env
            .storage()
            .persistent()
            .get::<StorageKey, CreditScore>(&StorageKey::Score(issuer.clone()));
        let grace = Self::get_downgrade_grace_ledgers(env.clone());

        if let Some(active) = current {
            if tier < active.tier && grace > 0 {
                let now = env.ledger().sequence();
                let pending = PendingDowngrade {
                    proposed_score,
                    proposed_at_ledger: now,
                    execute_after_ledger: now.saturating_add(grace),
                };
                env.storage()
                    .persistent()
                    .set(&StorageKey::PendingDowngrade(issuer.clone()), &pending);
                events::downgrade_proposed(
                    &env,
                    &issuer,
                    active.tier,
                    tier,
                    pending.execute_after_ledger,
                );
                return Ok(Some(pending));
            }
        }

        let old_tier = env
            .storage()
            .persistent()
            .get(&StorageKey::Score(issuer.clone()))
            .map(|s: CreditScore| s.tier)
            .unwrap_or(CreditTier::C);
        env.storage()
            .persistent()
            .set(&StorageKey::Score(issuer.clone()), &proposed_score);
        events::credit_score_updated(&env, &issuer, old_tier, tier);
        Ok(None)
    }

    /// Authorised-caller only: renew rating and cancel any pending downgrade.
    pub fn renew_and_cancel_downgrade(
        env: Env,
        caller: Address,
        issuer: Address,
    ) -> Result<(), Error> {
        require_authorised(&env, &caller)?;
        let key = StorageKey::PendingDowngrade(issuer.clone());
        if !env.storage().persistent().has(&key) {
            return Err(Error::NoPendingDowngrade);
        }
        env.storage().persistent().remove(&key);
        events::downgrade_cancelled(&env, &issuer, &caller);
        Ok(())
    }

    /// Execute a pending downgrade once its grace period has elapsed.
    pub fn execute_pending_downgrade(
        env: Env,
        caller: Address,
        issuer: Address,
    ) -> Result<CreditScore, Error> {
        require_authorised(&env, &caller)?;
        let key = StorageKey::PendingDowngrade(issuer.clone());
        let pending: PendingDowngrade = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::NoPendingDowngrade)?;

        if env.ledger().sequence() < pending.execute_after_ledger {
            return Err(Error::GracePeriodNotExpired);
        }

        let old_tier = env
            .storage()
            .persistent()
            .get(&StorageKey::Score(issuer.clone()))
            .map(|s: CreditScore| s.tier)
            .unwrap_or(CreditTier::C);
        env.storage()
            .persistent()
            .set(&StorageKey::Score(issuer.clone()), &pending.proposed_score);
        env.storage().persistent().remove(&key);
        events::credit_score_updated(&env, &issuer, old_tier, pending.proposed_score.tier);
        Ok(pending.proposed_score)
    }

    pub fn get_pending_downgrade(env: Env, issuer: Address) -> Option<PendingDowngrade> {
        env.storage()
            .persistent()
            .get(&StorageKey::PendingDowngrade(issuer))
    }
}
