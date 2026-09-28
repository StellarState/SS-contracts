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
