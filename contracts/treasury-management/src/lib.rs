//! Platform treasury management contract for StellarSettle.
//!
//! Consolidates protocol fee inflows from every fee-collecting contract,
//! tracks balances per fee source, and lets the admin allocate funds to
//! platform operations, grants, and reserves with a paginated audit log.

#![no_std]

mod errors;
mod events;
mod storage;
mod types;

pub use types::{AllocationReceipt, AllocationRecord, DailyAllocation, ReceiptPage, TreasuryBalance};

use soroban_sdk::{contract, contractimpl, Address, Env, String, Symbol, Vec};

use errors::Error;

/// Reject the zero address (all-zero 32-byte Ed25519 key).
fn ensure_non_zero_address(env: &Env, address: &Address) -> Result<(), Error> {
    let zero_str = String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    );
    let zero = Address::from_string(&zero_str);
    if *address == zero {
        return Err(Error::InvalidAddress);
    }
    Ok(())
}

#[contract]
pub struct TreasuryManagement;

#[contractimpl]
impl TreasuryManagement {
    /// Initialize the contract with the admin allowed to call `allocate`.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        ensure_non_zero_address(&env, &admin)?;
        if storage::get_admin(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        storage::set_admin(&env, &admin);
        storage::set_total_balance(&env, 0);
        events::initialized(&env, &admin);
        Ok(())
    }

    /// Called by any fee-collecting contract to attribute `amount` to
    /// `source`. This contract does not itself move tokens: the caller is
    /// expected to have already transferred `amount` of the protocol's
    /// fee token to this contract's address before calling, matching how
    /// the other fee-collecting contracts in this workspace separate
    /// token movement from balance bookkeeping.
    pub fn receive_fee(env: Env, source: Address, amount: i128) -> Result<(), Error> {
        source.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        storage::get_admin(&env).ok_or(Error::NotInit)?;

        if storage::is_allowlist_enforced(&env) && !storage::is_source_approved(&env, &source) {
            events::source_rejected(&env, &source, amount, &Symbol::new(&env, "unapproved"));
            return Err(Error::SourceNotApproved);
        }

        let prev_cum = storage::get_source_cumulative_total(&env, &source);
        let new_cum = prev_cum.checked_add(amount).ok_or(Error::Overflow)?;
        if let Some(cap) = storage::get_source_cap(&env, &source) {
            if new_cum > cap {
                events::source_rejected(&env, &source, amount, &Symbol::new(&env, "cap_exceeded"));
                return Err(Error::SourceCapExceeded);
            }
        }

        let ledger_day = env.ledger().timestamp() / 86_400;
        let prev_daily = storage::get_source_daily_total(&env, &source, ledger_day);
        let new_daily = prev_daily.checked_add(amount).ok_or(Error::Overflow)?;

        storage::credit_source(&env, &source, amount);
        storage::set_source_cumulative_total(&env, &source, new_cum);
        storage::set_source_daily_total(&env, &source, ledger_day, new_daily);
        let total = storage::get_total_balance(&env);
        storage::set_total_balance(&env, total.checked_add(amount).ok_or(Error::Overflow)?);
        storage::bump_treasury_revision(&env);

        events::source_fee_received(&env, &source, amount, new_daily, new_cum);
        events::fee_received(&env, &source, amount);
        Ok(())
    }

    /// Admin-only: toggle whether `receive_fee` enforces the approved source allowlist.
    pub fn set_source_allowlist_enforced(
        env: Env,
        admin: Address,
        enforced: bool,
    ) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();
        storage::set_allowlist_enforced(&env, enforced);
        Ok(())
    }

    /// Admin-only: approve or revoke a fee source and optionally set its aggregate cap.
    pub fn set_approved_source(
        env: Env,
        admin: Address,
        source: Address,
        approved: bool,
        cap: Option<i128>,
    ) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();
        ensure_non_zero_address(&env, &source)?;
        if let Some(c) = cap {
            if c <= 0 {
                return Err(Error::InvalidCap);
            }
        }
        storage::set_source_approved(&env, &source, approved);
        storage::set_source_cap(&env, &source, cap);
        Ok(())
    }

    /// View: whether `source` is approved.
    pub fn is_source_approved(env: Env, source: Address) -> bool {
        storage::is_source_approved(&env, &source)
    }

    /// View: optional aggregate cap for `source`.
    pub fn get_source_cap(env: Env, source: Address) -> Option<i128> {
        storage::get_source_cap(&env, &source)
    }

    /// View: `(daily_total, cumulative_total)` for `source` on `ledger_day`.
    pub fn get_source_totals(env: Env, source: Address, ledger_day: u64) -> (i128, i128) {
        (
            storage::get_source_daily_total(&env, &source, ledger_day),
            storage::get_source_cumulative_total(&env, &source),
        )
    }

    /// Admin-only: route `amount` of treasury funds to `destination` for
    /// `purpose` (e.g. "operations", "grant:Q3-community-fund",
    /// `reserve`). Records the allocation in the paginated history log and
    /// writes an immutable audit receipt (#825).
    ///
    /// `expected_revision` must equal the current treasury balance revision
    /// (see `get_treasury_revision`): it binds the allocation to the balance
    /// the caller observed, so a concurrent `receive_fee`/`allocate` that
    /// lands first rejects this call with `StaleRevision` instead of
    /// applying it against a stale balance.
    ///
    /// When a daily allocation limit is configured (#824), the allocation
    /// also fails with `DailyLimitExceeded` if it would push the current
    /// ledger day's cumulative total above the limit.
    ///
    /// Like `receive_fee`, this contract tracks the allocation as
    /// bookkeeping; the admin (or a caller the admin authorizes) is
    /// responsible for the actual token transfer to `destination`.
    pub fn allocate(
        env: Env,
        admin: Address,
        destination: Address,
        amount: i128,
        purpose: String,
        expected_revision: u64,
    ) -> Result<u32, Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();
        ensure_non_zero_address(&env, &destination)?;
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if purpose.is_empty() {
            return Err(Error::InvalidPurpose);
        }

        // Reject allocations computed against a stale treasury balance.
        if expected_revision != storage::get_treasury_revision(&env) {
            return Err(Error::StaleRevision);
        }

        let total = storage::get_total_balance(&env);
        if amount > total {
            return Err(Error::InsufficientBalance);
        }

        // Per-ledger-day allocation ceiling (#824). The day's bucket is
        // keyed by `timestamp / 86_400`, so capacity resets each new day.
        // `daily_treasury_limit_reached` is emitted from the committed
        // success path (a rejected call rolls back its events), once per
        // ledger day when cumulative allocations first reach the limit.
        let ledger_day = env.ledger().timestamp() / 86_400;
        let daily_limit = storage::get_daily_allocation_limit(&env);
        let mut day = storage::get_daily_allocation(&env, ledger_day);
        if let Some(limit) = daily_limit {
            let attempted = day.total.checked_add(amount).ok_or(Error::Overflow)?;
            if attempted > limit {
                return Err(Error::DailyLimitExceeded);
            }
            day.total = attempted;
            if day.total == limit && !day.limit_notified {
                day.limit_notified = true;
                events::daily_treasury_limit_reached(&env, ledger_day, limit, day.total);
            }
            storage::set_daily_allocation(&env, ledger_day, &day);
        }

        let current_ledger = env.ledger().sequence();
        let cooldown = storage::get_allocation_cooldown(&env);
        if cooldown > 0 {
            if let Some(last_ledger) = storage::get_last_allocation_ledger(&env) {
                if current_ledger < last_ledger.saturating_add(cooldown) {
                    return Err(Error::AllocationCooldownActive);
                }
            }
        }

        storage::set_total_balance(&env, total - amount);
        storage::set_last_allocation_ledger(&env, current_ledger);
        let new_revision = storage::bump_treasury_revision(&env);
        let ledger_timestamp = env.ledger().timestamp();

        let record = AllocationRecord {
            destination: destination.clone(),
            amount,
            purpose: purpose.clone(),
            timestamp: ledger_timestamp,
        };
        let index = storage::push_allocation(&env, &record);

        // Immutable audit receipt bound to the post-allocation treasury
        // revision (#825). Receipt ids advance in lock-step with the
        // allocation log, so `receipt_id` also indexes the history.
        let receipt = AllocationReceipt {
            receipt_id: storage::get_receipt_count(&env),
            allocator: admin.clone(),
            destination: destination.clone(),
            amount,
            purpose: purpose.clone(),
            ledger: current_ledger,
            ledger_timestamp,
            treasury_revision: new_revision,
        };
        let receipt_id = storage::push_receipt(&env, &receipt);
        events::treasury_allocation_receipt(
            &env,
            receipt_id,
            &admin,
            &destination,
            amount,
            &purpose,
            current_ledger,
            ledger_timestamp,
            new_revision,
        );

        if cooldown > 0 {
            events::allocation_cooldown_started(
                &env,
                current_ledger,
                cooldown,
                current_ledger.saturating_add(cooldown),
            );
        }
        events::funds_allocated(&env, &destination, amount, &purpose);
        Ok(index)
    }

    pub fn set_allocation_cooldown(
        env: Env,
        admin: Address,
        cooldown_ledgers: u32,
    ) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();
        storage::set_allocation_cooldown(&env, cooldown_ledgers);
        Ok(())
    }

    pub fn get_allocation_cooldown(env: Env) -> u32 {
        storage::get_allocation_cooldown(&env)
    }

    pub fn get_last_allocation_ledger(env: Env) -> Option<u32> {
        storage::get_last_allocation_ledger(&env)
    }

    pub fn get_remaining_cooldown(env: Env) -> u32 {
        let cooldown = storage::get_allocation_cooldown(&env);
        if cooldown == 0 {
            return 0;
        }
        match storage::get_last_allocation_ledger(&env) {
            Some(last) => last
                .saturating_add(cooldown)
                .saturating_sub(env.ledger().sequence()),
            None => 0,
        }
    }

    /// Admin-only: set or clear the per-ledger-day allocation ceiling (#824).
    /// `None` disables the limit; `Some(limit)` requires `limit > 0`.
    pub fn set_daily_allocation_limit(
        env: Env,
        admin: Address,
        limit: Option<i128>,
    ) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();
        if let Some(l) = limit {
            if l <= 0 {
                return Err(Error::InvalidDailyLimit);
            }
        }
        storage::set_daily_allocation_limit(&env, limit);
        Ok(())
    }

    /// View: the configured per-ledger-day allocation limit, or `None` when
    /// no limit is set.
    pub fn get_daily_allocation_limit(env: Env) -> Option<i128> {
        storage::get_daily_allocation_limit(&env)
    }

    /// View: remaining allocation capacity for the current ledger day.
    /// Returns `None` when no daily limit is configured.
    pub fn get_daily_allocation_remaining(env: Env) -> Option<i128> {
        let limit = storage::get_daily_allocation_limit(&env)?;
        let ledger_day = env.ledger().timestamp() / 86_400;
        let used = storage::get_daily_allocation(&env, ledger_day).total;
        Some(limit.saturating_sub(used).max(0))
    }

    /// View: total allocated so far on the current ledger day.
    pub fn get_daily_allocation_total(env: Env) -> i128 {
        let ledger_day = env.ledger().timestamp() / 86_400;
        storage::get_daily_allocation(&env, ledger_day).total
    }

    /// View: current treasury balance revision. Pass this as
    /// `expected_revision` to `allocate` to reject stale applications.
    pub fn get_treasury_revision(env: Env) -> u64 {
        storage::get_treasury_revision(&env)
    }

    /// View: fetch one allocation receipt by id.
    pub fn get_receipt(env: Env, receipt_id: u64) -> Option<AllocationReceipt> {
        storage::get_receipt(&env, receipt_id)
    }

    /// View: total number of allocation receipts ever written.
    pub fn get_receipt_count(env: Env) -> u64 {
        storage::get_receipt_count(&env)
    }

    /// View: cursor-paginated allocation receipt history (#825), oldest
    /// first. `cursor` is the receipt id to start from (`0` for the first
    /// page, or the `next_cursor` of the previous page); `limit` caps the
    /// page size. The history is append-only: receipts are never updated
    /// or removed.
    pub fn get_receipt_history(env: Env, cursor: u64, limit: u32) -> ReceiptPage {
        let count = storage::get_receipt_count(&env);
        let mut receipts = Vec::new(&env);
        if cursor >= count || limit == 0 {
            return ReceiptPage {
                receipts,
                next_cursor: None,
            };
        }
        let end = core::cmp::min(cursor.saturating_add(u64::from(limit)), count);
        let mut id = cursor;
        while id < end {
            if let Some(receipt) = storage::get_receipt(&env, id) {
                receipts.push_back(receipt);
            }
            id += 1;
        }
        let next_cursor = if end < count { Some(end) } else { None };
        ReceiptPage {
            receipts,
            next_cursor,
        }
    }

    /// View: total treasury balance and a per-source breakdown.
    pub fn get_treasury_balance(env: Env) -> TreasuryBalance {
        let total = storage::get_total_balance(&env);
        let known_sources = storage::get_known_sources(&env);
        let mut sources = Vec::new(&env);
        let mut amounts = Vec::new(&env);
        for source in known_sources.iter() {
            sources.push_back(source.clone());
            amounts.push_back(storage::get_source_balance(&env, &source));
        }
        TreasuryBalance {
            total,
            sources,
            amounts,
        }
    }

    /// View: paginated allocation log, oldest-first within the page.
    /// `offset` is the index of the first record to return; `limit` caps
    /// how many are returned. Returns fewer than `limit` records once the
    /// end of the log is reached, and an empty Vec once `offset` is past
    /// the end.
    pub fn get_allocation_history(env: Env, offset: u32, limit: u32) -> Vec<AllocationRecord> {
        let count = storage::get_allocation_count(&env);
        let mut result = Vec::new(&env);
        if offset >= count {
            return result;
        }
        let end = core::cmp::min(offset.saturating_add(limit), count);
        for index in offset..end {
            if let Some(record) = storage::get_allocation(&env, index) {
                result.push_back(record);
            }
        }
        result
    }

    /// View: total number of allocation records ever written.
    pub fn get_allocation_count(env: Env) -> u32 {
        storage::get_allocation_count(&env)
    }

    /// View: return the current admin.
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)
    }
}

#[cfg(test)]
mod test;
