# StellarSettle API Reference

This document summarizes the public functions exposed by the StellarSettle Soroban contracts.

## Payment Distributor Contract

The `payment-distributor` contract now implements the settlement/refund fan-out flow used by `invoice-escrow` when a distributor is configured.

### `initialize(admin: Address)`
- Sets the distributor admin.

### `distribute_payment(escrow_contract: Address, invoice_id: Symbol, addresses: Vec<Address>, amounts: Vec<i128>, escrow_status: u32)`
- Callable only by the authenticated escrow contract.
- `addresses` must be `[token, seller, funder, admin]`.
- `amounts` must be `[paid_amount, seller_amount, investor_amount, platform_fee]`.
- `escrow_status` must represent `Funded` or `Settled`.
- Uses `paid_amount` plus internal distribution state to prevent replay or double distribution.
- Transfers funds held by `payment-distributor` to the seller, funder, and admin.

### `distribute_refund(escrow_contract: Address, invoice_id: Symbol, addresses: Vec<Address>, amounts: Vec<i128>, escrow_status: u32)`
- Callable only by the authenticated escrow contract.
- `addresses` must be `[token, funder]`.
- `amounts` must be `[refund_amount]`.
- `escrow_status` must represent `Refunded`.
- Refund distribution is one-time per `(escrow_contract, invoice_id)`.

### `get_distribution_state(escrow_contract: Address, invoice_id: Symbol) -> DistributionState`
- Returns the tracked payout progress for an escrow invoice.
- `paid_distributed` is the cumulative `paid_amount` already fanned out.
- `refund_distributed` indicates whether the refund leg has already been processed.

### `get_admin() -> Address`
- Returns the distributor admin.

### Events
- `payment_distributed`
- `refund_distributed`
- `initialized`

## Invoice Token Contract

`invoice-token` is the invoice ownership token contract with transfer-lock and pause controls.

### Core Functions
- `initialize(admin, name, symbol, decimals, invoice_id, minter)`
- `transfer(from, to, amount)`
- `approve(from, spender, amount, expiration_ledger)`
- `transfer_from(spender, from, to, amount)`
- `burn(from, amount)`
- `burn_from(spender, from, amount)`
- `mint(to, amount, by)`
- `set_transfer_locked(caller, locked)`
- `set_minter(new_minter)`
- `set_paused(paused)`
- `paused() -> bool`
- `transfer_locked() -> bool`

### Pause Policy
- When paused, the contract rejects `transfer`, `approve`, `transfer_from`, `burn`, `burn_from`, and `mint`.
- Admin-only config operations such as `set_transfer_locked`, `set_minter`, and `set_paused` remain available.

### Events
- `transfer`
- `approve`
- `mint`
- `burn`
- `transfer_locked_updated`
- `minter_updated`
- `paused_updated`

## Invoice Escrow Contract

`invoice-escrow` remains the lifecycle orchestrator and now optionally routes settlement/refund payouts through `payment-distributor`.

### Core Functions
- `initialize(admin: Address, platform_fee_bps: u32)`
- `create_escrow(invoice_id, seller, amount, due_date, payment_token, invoice_token)`
- `cancel_escrow(invoice_id, seller)`
- `fund_escrow(invoice_id, buyer)`
- `record_payment(invoice_id, payer, amount)`
- `refund(invoice_id)`
- `update_platform_fee_bps(new_fee_bps)`
- `set_payment_distributor(payment_distributor)`
- `set_paused(paused)`
- `get_escrow(invoice_id) -> EscrowData`
- `get_config() -> Config`
- `get_escrow_status(invoice_id) -> EscrowStatus`
- `paused() -> bool`

### Distribution Behavior
- If `payment_distributor` is configured, `record_payment` transfers the current settlement funds into the distributor and invokes `distribute_payment`.
- If `payment_distributor` is configured, `refund` transfers the remaining collateral into the distributor and invokes `distribute_refund`.
- If no distributor is configured, escrow falls back to the legacy direct-transfer path.

### Pause Policy
- When paused, the contract rejects lifecycle-changing operations:
  `create_escrow`, `cancel_escrow`, `fund_escrow`, `record_payment`, and `refund`.
- View functions and admin-only configuration updates remain available.

### Events
- `escrow_created`
- `escrow_funded`
- `payment_settled`
- `escrow_refunded`
- `escrow_cancelled`
- `platform_fee_updated`
- `distributor_updated`
- `paused_updated`
