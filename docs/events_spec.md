# Blockchain Indexer Event Log Specification

This specification documents the exact event topics and payload schemas emitted by StellarSettle contracts (`invoice-escrow`, `invoice-token`, `payment-distributor`) for off-chain indexer consumption.

## Invoice Escrow Contract Events

### `escrow_created`
- **Topics**: `(Symbol("escrow_created"), invoice_id: Symbol)`
- **Data Payload**: `(seller: Address, debtor: Address, face_value: i128, purchase_price: i128, due_date: u64, payment_token: Address, invoice_token: Address, commitment: BytesN<32>)`
- **Description**: Emitted when a new escrow contract is initialized by a seller.

### `escrow_funded`
- **Topics**: `(Symbol("escrow_funded"), invoice_id: Symbol)`
- **Data Payload**: `(buyer: Address, funded_amount: i128, total_funded: i128, purchase_price: i128)`
- **Description**: Emitted when an investor deposits funds toward an invoice purchase.

### `payment_settled`
- **Topics**: `(Symbol("payment_settled"), invoice_id: Symbol)`
- **Data Payload**: `(payment_amount: i128, platform_fee: i128, investor_payout: i128)`
- **Description**: Emitted when a debtor executes payment settlement.

### `escrow_refunded`
- **Topics**: `(Symbol("escrow_refunded"), invoice_id: Symbol)`
- **Data Payload**: `(refunded_amount: i128)`
- **Description**: Emitted when an unpaid invoice triggers an investor collateral refund after due date.

### `installment_schedule_set`
- **Topics**: `(Symbol("installment_schedule_set"),)`
- **Data Payload**: `(invoice_id: Symbol, milestone_count: u32, final_due_ts: u64, total_amount: i128)`
- **Description**: Emitted when a seller configures or replaces the installment repayment milestone schedule for an invoice.

### `installment_settled`
- **Topics**: `(Symbol("installment_settled"),)`
- **Data Payload**: `(invoice_id: Symbol, index: u32, cumulative_amount: i128, paid_amt: i128)`
- **Description**: Emitted each time cumulative repayments reach a milestone's cumulative target (or the escrow fully settles, closing any remaining milestones).

### `settlement_proposed`
- **Topics**: `(Symbol("settlement_proposed"),)`
- **Data Payload**: `(invoice_id: BytesN<32>, proposer: Address, repayment_amount: i128)`
- **Description**: Emitted when an authorized admin proposes a registered-invoice settlement.

### `settlement_approved`
- **Topics**: `(Symbol("settlement_approved"),)`
- **Data Payload**: `(invoice_id: BytesN<32>, approver: Address, repayment_amount: i128)`
- **Description**: Emitted after a different authorized admin approves and executes the proposal.

## Invoice Token Event Schema Coverage

Token event payloads are emitted by `contracts/invoice-token/src/events.rs`. Contract tests validate the emitted topic tuple and typed data payload for transfer, approval, mint, burn, lock changes, minter changes, pause changes, fee updates, freeze/unfreeze, nonce queries, and multi-event transfer flows. Keep one assertion for every event helper when changing a topic or payload; event topic names are part of the external integration contract.

| Event topic | Topics after contract id | Data |
| --- | --- | --- |
| `transfer` | `transfer, from, to` | `amount: i128` |
| `approve` | `approve, from, spender` | `(amount: i128, expiration_ledger: u32)` |
| `mint` | `mint, to` | `amount: i128` |
| `burn` | `burn, from` | `amount: i128` |
| `transfer_locked_updated` | `transfer_locked_updated` | `(old: bool, new: bool)` |
| `minter_updated` | `minter_updated` | `(old: Address, new: Address)` |
| `paused_updated` | `paused_updated` | `(old: bool, new: bool)` |
| `account_frozen` / `account_unfrozen` | event name | `account: Address` |
| `allow_extend` | `allow_extend, from, spender` | `new_expiration_ledger: u32` |
| `decimals_updated` | `decimals_updated` | `(old: u32, new: u32)` |
| `approval_revoked` | `approval_revoked, from, spender` | unit |
| `nonce_queried` | `nonce_queried` | `(account: Address, nonce: u64)` |
| `fee_deducted` | `fee_deducted` | `(from: Address, fee_amount: i128)` |
| `history_appended` | `history_appended` | `(from, from_clone, to: Address, amount: i128)` |
| `fee_updated` | `fee_updated` | `(old_bps: i128, new_bps: i128)` |
| `role_admin_updated` | `role_admin_updated, role` | `(old_admin: Address, new_admin: Address)` |
| `role_granted` | `role_granted, role, account` | `granted: bool` |

## Payment Distributor Contract Events

The payment-distributor contract orchestrates lifecycle-driven payouts to sellers, investors, and platform fee recipients. It emits events for distribution tracking, configuration updates, and emergency operations.

### `initialized`
- **Topics**: `(Symbol("initialized"),)`
- **Data Payload**: `admin: Address`
- **Description**: Emitted once during contract initialization with the admin address.

### `PaymentDistributed`
- **Topics**: `(Symbol("PaymentDistributed"), escrow: Address, invoice_id: Symbol)`
- **Data Payload**: `(recipients: Vec<Address>, amounts: Vec<i128>, escrow_status: u32, timestamp: u64)`
- **Description**: Comprehensive distribution audit event emitted when payment is distributed. Includes:
  - `recipients`: Vector of recipient addresses `[seller, funder, fee_recipient]`
  - `amounts`: Vector of amounts `[seller_amount, investor_amount, platform_fee, total_paid]`
  - `escrow_status`: Current escrow status code
  - `timestamp`: Ledger timestamp for compliance tracking

### `refund_distributed`
- **Topics**: `(Symbol("refund_distributed"), escrow: Address, invoice_id: Symbol)`
- **Data Payload**: `(recipients: Vec<Address>, amounts: Vec<i128>)`
- **Description**: Emitted when refund distribution is executed. Recipients vector contains funder addresses, amounts vector contains corresponding refund amounts.

### `fee_recipient_updated`
- **Topics**: `(Symbol("fee_recipient_updated"),)`
- **Data Payload**: `(old_recipient: Option<Address>, new_recipient: Address)`
- **Description**: Emitted when the platform fee recipient address is updated by admin. `old_recipient` is None on first configuration.

### `escrow_contract_updated`
- **Topics**: `(Symbol("escrow_contract_updated"),)`
- **Data Payload**: `(old_escrow: Option<Address>, new_escrow: Address)`
- **Description**: Emitted when the authorized escrow contract binding is updated. `old_escrow` is None on first configuration.

### `platform_fee_updated`
- **Topics**: `(Symbol("platform_fee_updated"),)`
- **Data Payload**: `(admin: Address, tiers: Vec<FeeTier>)`
- **Description**: Emitted when dynamic platform fee tier structure is updated. Contains the admin address and complete fee tier configuration.

### `investor_bonus_rate_updated`
- **Topics**: `(Symbol("investor_bonus_rate_updated"),)`
- **Data Payload**: `(admin: Address, bonus_bps: u32)`
- **Description**: Emitted when investor bonus rate (in basis points) is updated by admin.

### `referral_paid`
- **Topics**: `(Symbol("referral_paid"), token: Address)`
- **Data Payload**: `(referral_recipient: Address, amount: i128)`
- **Description**: Per-referral payout event emitted when a referral fee cut is distributed. Token address is in topics for multi-currency indexing.

### `AssetDistributed`
- **Topics**: `(Symbol("AssetDistributed"), token: Address)`
- **Data Payload**: `(recipients: Vec<Address>, amounts: Vec<i128>, total: i128)`
- **Description**: Per-asset multi-currency distribution audit event. Emitted for each asset type distributed, with token address in topics for efficient indexing.

### `EmergencyWithdrawal`
- **Topics**: `(Symbol("EmergencyWithdrawal"), token: Address)`
- **Data Payload**: `(admin: Address, to: Address, amount: i128)`
- **Description**: Emergency withdrawal audit event emitted when admin executes emergency token recovery. Includes admin, destination address, and withdrawn amount.

### `DustSwept`
- **Topics**: `(Symbol("DustSwept"), token: Address)`
- **Data Payload**: `(admin: Address, to: Address, amount: i128)`
- **Description**: Emitted when admin sweeps residual dust amounts remaining in the distributor contract to a specified address.

### `ExcessRefunded`
- **Topics**: `(Symbol("ExcessRefunded"), token: Address, invoice_id: Symbol)`
- **Data Payload**: `(escrow: Address, amount: i128)`
- **Description**: Emitted when excess funds are returned to the originating escrow contract that deposited them.

### `role_grant_updated`
- **Topics**: `(Symbol("role_grant_updated"), role: Symbol, account: Address)`
- **Data Payload**: `granted: bool`
- **Description**: Emitted when role-based access control grants or revokes a role for an account.

---

## Indexing Guide for Payment Distributor Events

### High-Priority Events for Indexers

1. **`PaymentDistributed`**: Primary distribution tracking event. Index by `escrow` and `invoice_id` topics for efficient lookup. Extract `recipients`, `amounts`, `escrow_status`, and `timestamp` from data payload.

2. **`refund_distributed`**: Critical for tracking refund operations. Index by `escrow` and `invoice_id` topics.

3. **`AssetDistributed`**: Essential for multi-currency support. Index by `token` address in topics for per-asset filtering.

### Configuration Events

- **`fee_recipient_updated`**, **`escrow_contract_updated`**, **`platform_fee_updated`**, **`investor_bonus_rate_updated`**: Track distributor configuration changes over time.

### Operational Events

- **`referral_paid`**: Track referral payouts per token.
- **`EmergencyWithdrawal`**, **`DustSwept`**: Monitor admin interventions and dust cleanup operations.
- **`ExcessRefunded`**: Track excess fund returns to escrow contracts.

### Event Schema Patterns

1. **PascalCase Events**: `PaymentDistributed`, `AssetDistributed`, `EmergencyWithdrawal`, `DustSwept`, `ExcessRefunded` use PascalCase for consistency with structured audit events.

2. **snake_case Events**: Configuration and initialization events use snake_case: `initialized`, `fee_recipient_updated`, `escrow_contract_updated`, `platform_fee_updated`, `investor_bonus_rate_updated`, `refund_distributed`, `referral_paid`, `role_grant_updated`.

3. **Topic Structure**: High-cardinality identifiers (`escrow`, `invoice_id`, `token`) are placed in topics for efficient filtering. Low-cardinality data is in the payload.

4. **Timestamp Inclusion**: `PaymentDistributed` includes ledger timestamp for compliance and audit trail requirements.

---

## Indexed vs Unindexed Fields Reference

This section explicitly identifies which fields are indexed (appear in event topics) vs unindexed (appear only in data payload) for each contract. Indexed fields enable efficient event filtering by indexers.

### Invoice Escrow Contract

| Event | Indexed Fields (Topics) | Unindexed Fields (Data Payload) |
|-------|------------------------|--------------------------------|
| `escrow_created` | `invoice_id` | `seller`, `debtor`, `face_value`, `purchase_price`, `due_date`, `payment_token`, `invoice_token`, `commitment`, `funding_milestone` |
| `escrow_funded` | `invoice_id` | `buyer`, `funded_amount`, `total_funded`, `purchase_price` |
| `payment_settled` | `invoice_id` | `payment_amount`, `platform_fee`, `investor_payout` |
| `escrow_refunded` | `invoice_id` | `refunded_amount` |
| `escrow_status_changed` | — | `invoice_id`, `status`, `timestamp` |
| `installment_schedule_set` | — | `invoice_id`, `milestone_count`, `final_due_ts`, `total_amount` |
| `installment_settled` | — | `invoice_id`, `index`, `cumulative_amount`, `paid_amt` |
| `settlement_proposed` | — | `invoice_id`, `proposer`, `repayment_amount` |
| `settlement_approved` | — | `invoice_id`, `approver`, `repayment_amount` |
| `early_settlement_applied` | — | `invoice_id`, `discount_bps`, `original_face`, `effective_face` |
| `grace_period_expired` | — | `invoice_id`, `due_dt`, `grace_period_seconds` |
| `category_fee_updated` | — | `category`, `fee_bps` |
| `DisputeRaised` | — | `invoice_id`, `raiser`, `raised_at` |
| `DisputeResolved` | — | `invoice_id`, `favour`, `timed_out` |
| `platform_fee_updated` | — | `old_fee_bps`, `new_fee_bps` |
| `distributor_updated` | `new_distributor` | `had_previous_distributor` |
| `paused_updated` | — | `old_paused`, `new_paused` |
| `fee_collected` | — | `invoice_id`, `fee_amount`, `treasury` |
| `max_investors_updated` | — | `count` |
| `PartialFillExecuted` | `invoice_id` | `investor`, `requested`, `accepted`, `returned` |
| `param_change_proposed` | — | `param`, `new_value`, `proposed_at`, `timelock_secs` |
| `param_change_executed` | — | `param`, `new_value` |
| `currency_invested` | `invoice_id` | `investor`, `currency`, `amount`, `base_amount` |
| `currency_max_age_set` | `currency` | `max_age_secs` |

### Invoice Token Contract

| Event | Indexed Fields (Topics) | Unindexed Fields (Data Payload) |
|-------|------------------------|--------------------------------|
| `transfer` | `from`, `to` | `amount` |
| `approve` | `from`, `spender` | `amount`, `expiration_ledger` |
| `mint` | `to` | `amount` |
| `burn` | `from` | `amount` |
| `transfer_locked_updated` | — | `old`, `new` |
| `minter_updated` | — | `old_minter`, `new_minter` |
| `paused_updated` | — | `old`, `new` |
| `account_frozen` / `account_unfrozen` | — | `account` |
| `allow_extend` | `from`, `spender` | `new_expiration_ledger` |
| `decimals_updated` | — | `old`, `new` |
| `approval_revoked` | `from`, `spender` | — |
| `nonce_queried` | — | `account`, `nonce` |
| `fee_deducted` | — | `from`, `fee_amount` |
| `history_appended` | — | `from`, `from_clone`, `to`, `amount` |
| `fee_updated` | — | `old_bps`, `new_bps` |
| `role_admin_updated` | `role` | `old_admin`, `new_admin` |
| `role_granted` | `role`, `account` | `granted` |
| `clawback` | `from` | `amount` |

### Payment Distributor Contract

| Event | Indexed Fields (Topics) | Unindexed Fields (Data Payload) |
|-------|------------------------|--------------------------------|
| `initialized` | — | `admin` |
| `PaymentDistributed` | `escrow`, `invoice_id` | `recipients`, `amounts`, `escrow_status`, `timestamp` |
| `refund_distributed` | `escrow`, `invoice_id` | `recipients`, `amounts` |
| `fee_recipient_updated` | — | `old_recipient`, `new_recipient` |
| `escrow_contract_updated` | — | `old_escrow`, `new_escrow` |
| `platform_fee_updated` | — | `admin`, `tiers` |
| `investor_bonus_rate_updated` | — | `admin`, `bonus_bps` |
| `referral_paid` | `token` | `referral_recipient`, `amount` |
| `AssetDistributed` | `token` | `recipients`, `amounts`, `total` |
| `EmergencyWithdrawal` | `token` | `admin`, `to`, `amount` |
| `DustSwept` | `token` | `admin`, `to`, `amount` |
| `ExcessRefunded` | `token`, `invoice_id` | `escrow`, `amount` |
| `role_grant_updated` | `role`, `account` | `granted` |

---

## Snapshot Testing

Event schema snapshots are captured in contract test suites:

- `contracts/invoice-escrow/src/event_schema_tests.rs`
- `contracts/payment-distributor/src/event_schema_tests.rs`
- `contracts/invoice-token/src/event_schema_tests.rs`

These tests validate topic names, topic count, data field count, and field types. They fail on:
- Topic renames
- Field removals or additions
- Field type changes
- Field order changes

To update snapshots after intentional schema changes, run the tests and update the assertions to match the new schema.
