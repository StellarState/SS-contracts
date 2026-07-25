# Escrow State-Machine Invariants & Transition Matrix

This document provides the formal state-machine specification for the StellarSettle `invoice-escrow` contract and its interactions with `invoice-token` and `payment-distributor`.

---

## 1. Escrow States Definition

The escrow lifecycle is governed by the `EscrowStatus` enum (`repr(u32)`):

| Status Enum | Value | Description |
|---|---|---|
| `Created` | `0` | Initial state. Invoice registered by Seller. Awaiting investor funding. |
| `Funded` | `1` | Purchase price fully or partially funded by Investor(s). Invoice tokens minted. |
| `Settled` | `2` | Face value fully paid by Debtor/Payer. Investor collateral released; tokens unlocked. |
| `Refunded` | `3` | Refund executed after due date expiry (`due_dt`). Unreleased collateral returned to Investor. |
| `Cancelled` | `4` | Cancelled by Seller before any funding occurred (`funded_amt == 0`). Terminal state. |

---

## 2. State Transition Matrix

The table below defines valid (`✅`) and invalid (`❌`) state transitions. Any attempt to execute an action on an invalid current state results in an immediate contract panic / rejection.

| Current State | `create_escrow` | `cancel_escrow` | `fund_escrow` | `record_payment` | `refund` | Next State |
|---|---|---|---|---|---|---|
| **Uninitialized** | ✅ | ❌ | ❌ | ❌ | ❌ | `Created` |
| `Created` | ❌ | ✅ | ✅ | ❌ | ❌ | `Cancelled` or `Funded` |
| `Funded` | ❌ | ❌ | ❌ | ✅ | ✅ | `Settled` or `Refunded` |
| `Settled` | ❌ | ❌ | ❌ | ❌ | ❌ | *Terminal State* |
| `Refunded` | ❌ | ❌ | ❌ | ❌ | ❌ | *Terminal State* |
| `Cancelled` | ❌ | ❌ | ❌ | ❌ | ❌ | *Terminal State* |

---

## 3. Entrypoint Pre-Conditions, Post-Conditions & Invariants

### 3.1 `create_escrow`
- **Caller / Authorization:** Seller (`seller.require_auth()`)
- **Pre-Conditions:**
  - Contract is not paused (`config.paused == false`).
  - Invoice ID (`inv_id`) does not already exist in persistent storage.
  - `face_value > 0`, `purchase_price > 0`, `purchase_price <= face_value`.
  - `due_dt > current_ledger_timestamp`.
  - Commitment hash (`commitment`) is valid 32-byte hash (`BytesN<32>`).
- **Post-Conditions:**
  - `EscrowData` record stored under `StorageKey::Escrow(inv_id)` with `status = Created`, `funded_amt = 0`, `paid_amt = 0`.
  - Event `escrow_created` emitted.
- **Invariants:**
  - No payment or invoice tokens transferred.
  - `invoice-token` transfer locks remain enabled (`transfer_locked = true`).

---

### 3.2 `cancel_escrow`
- **Caller / Authorization:** Seller (`seller.require_auth()`)
- **Pre-Conditions:**
  - Contract is not paused (`config.paused == false`).
  - `status == EscrowStatus::Created`.
  - `funded_amt == 0` (zero investor funds deposited).
- **Post-Conditions:**
  - `status` updated to `EscrowStatus::Cancelled`.
  - Event `escrow_cancelled` emitted.
- **Invariants:**
  - Terminal state; no funds transferred or locked.

---

### 3.3 `fund_escrow`
- **Caller / Authorization:** Funder / Investor (`funder.require_auth()`)
- **Pre-Conditions:**
  - Contract is not paused (`config.paused == false`).
  - `status == EscrowStatus::Created`.
  - `amount > 0` and `amount + funded_amt <= purchase_price`.
  - Funder has sufficient `payment_token` balance and approval.
- **Post-Conditions:**
  - `payment_token` transferred from Funder into `invoice-escrow` contract address.
  - `funded_amt` incremented by `amount`.
  - `funder` set to `Some(funder_address)`.
  - When `funded_amt == purchase_price`:
    - `status` transitions from `Created` to `Funded`.
    - Escrow calls `invoice-token.mint(funder, purchase_price)` to issue ownership tokens to investor.
  - Event `escrow_funded` emitted.
- **Invariants:**
  - Escrow contract balance of `payment_token` increases by exact funded `amount`.
  - `invoice-token` transfers remain locked (`transfer_locked = true`).

---

### 3.4 `record_payment`
- **Caller / Authorization:** Debtor / Payer (`payer.require_auth()`)
- **Pre-Conditions:**
  - Contract is not paused (`config.paused == false`).
  - `status == EscrowStatus::Funded`.
  - `amount > 0` and `paid_amt + amount <= face_value`.
  - Payer has sufficient `payment_token` balance and approval.
- **Post-Conditions:**
  - `payment_token` transferred from Payer into `invoice-escrow` (or `payment-distributor`).
  - `paid_amt` incremented by `amount`.
  - **Distribution:**
    - If `payment_distributor` configured: invokes `distribute_payment` fan-out to Seller, Investor, and Platform Fee account.
    - If no distributor: direct transfer of `payment amount` to Seller/Investor per fee schedule.
  - When `paid_amt == face_value`:
    - `status` transitions from `Funded` to `Settled`.
    - Escrow invokes `invoice-token.set_transfer_locked(false)` to unlock secondary market trading of invoice tokens.
  - Event `payment_recorded` emitted.
- **Invariants:**
  - Total payment tokens received equals total tokens distributed.
  - `paid_amt <= face_value` strictly enforced.

---

### 3.5 `refund`
- **Caller / Authorization:** Anyone (typically Funder or Admin)
- **Pre-Conditions:**
  - Contract is not paused (`config.paused == false`).
  - `status == EscrowStatus::Funded`.
  - Current ledger timestamp > `due_dt` (invoice is past due).
  - Unreleased collateral remains: `unreleased = purchase_price - paid_amt > 0`.
- **Post-Conditions:**
  - Remaining unreleased payment token balance transferred back to Investor (`funder`).
  - `status` transitions from `Funded` to `Refunded`.
  - Escrow invokes `invoice-token.set_transfer_locked(false)`.
  - Event `escrow_refunded` emitted.
- **Invariants:**
  - Refund amount + `paid_amt` == `purchase_price`.

---

## 4. Invoice Token Governance & Lifecycle

The `invoice-token` contract implements restricted minting and transfer locking tied to the escrow lifecycle:

| Escrow State | `mint` Authorized | `transfer_locked` | Secondary Market Trading |
|---|---|---|---|
| `Created` | Escrow Contract | `true` | Disabled (Admin only) |
| `Funded` | Escrow Contract | `true` | Disabled (Admin only) |
| `Settled` | No | `false` | **Enabled** (All holders) |
| `Refunded` | No | `false` | **Enabled** (All holders) |
| `Cancelled` | No | `true` | Disabled |

---

## 5. Storage Policies & Key Isolation

The contract uses Soroban's storage primitives respecting the 10-character `contracttype` symbol limit:

### 5.1 Storage Key Schemas
```rust
pub enum StorageKey {
    Config,                              // Instance: Global configuration struct
    Escrow(Symbol),                      // Persistent: Per-invoice EscrowData
    FunderAmount(Symbol, Address),       // Persistent: Individual funder contribution
}
```

### 5.2 Storage TTL & Extension Policies
- **Instance Storage (`Config`):** Extended on every admin write or state transition to ensure global configuration remains active.
- **Persistent Storage (`Escrow`, `FunderAmount`):** Extended on `create_escrow`, `fund_escrow`, `record_payment`, `refund`, and `cancel_escrow` by `STORAGE_EXTEND_AMOUNT` ledger sequence threshold.

---

## 6. Emergency Pause Interaction Matrix

When `config.paused == true`, all state-mutating entrypoints panic with error `ContractPaused`:

| Entrypoint | Execution when `paused == true` | Error Code |
|---|---|---|
| `set_paused(bool)` | Allowed (Admin only) | N/A |
| `create_escrow` | **Rejected** | `ContractPaused` |
| `cancel_escrow` | **Rejected** | `ContractPaused` |
| `fund_escrow` | **Rejected** | `ContractPaused` |
| `record_payment` | **Rejected** | `ContractPaused` |
| `refund` | **Rejected** | `ContractPaused` |
| Read-only getters (`get_escrow`, `get_config`) | **Allowed** | N/A |
