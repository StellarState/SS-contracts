# StellarSettle API Reference

This document provides a reference for the public functions available in the StellarSettle smart contracts.

## Payment Distributor Contract

The `Payment Distributor` contract handles the secure distribution of funds from the contract to specified recipients. This is used for settling payments to investors and platform fees.

### `initialize(admin: Address)`
Initializes the contract with an administrative address.
- **admin**: The address that will have permission to call distribution functions.

### `distribute(token: Address, recipient: Address, amount: i128)`
Distributes a specified amount of a token to a recipient address. This function requires authorization from the contract admin.
- **token**: The address of the SEP-41 compliant token to distribute.
- **recipient**: The address of the recipient receiving the funds.
- **amount**: The amount of tokens to transfer.

### `get_admin() -> Address`
Returns the current administrative address of the contract.

## Invoice Escrow Contract

(Legacy documentation for reference)

### `initialize(admin: Address, platform_fee_bps: u32)`
### `create_escrow(invoice_id: Symbol, seller: Address, amount: i128, due_date: u64, payment_token: Address, invoice_token: Address)`
### `fund_escrow(invoice_id: Symbol, buyer: Address)`
### `record_payment(invoice_id: Symbol, payer: Address, amount: i128)`
### `refund(invoice_id: Symbol)`
