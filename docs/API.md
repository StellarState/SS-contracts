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

## Invoice Token Contract

### Events
- `transfer_locked_updated(old_locked: bool, new_locked: bool)`
- `minter_updated(old_minter: Address, new_minter: Address)`

## Invoice Escrow Contract

(Legacy documentation for reference)

### `initialize(admin: Address, platform_fee_bps: u32)`
### `create_escrow(invoice_id: Symbol, seller: Address, debtor: Address, face_value: i128, purchase_price: i128, due_date: u64, payment_token: Address, invoice_token: Address, commitment: BytesN<32>)`
Creates an escrow for an invoice with the specified parameters.
- **invoice_id**: Unique identifier for the invoice.
- **seller**: Address of the invoice seller (creator of the escrow).
- **debtor**: Address of the party responsible for paying the invoice.
- **face_value**: Total amount owed by the debtor (must be > 0).
- **purchase_price**: Amount the investor will pay to fund the escrow (must be > 0).
- **due_date**: Unix timestamp when the invoice is due (must be > 0 and > current ledger timestamp).
- **payment_token**: Address of the token used for payments.
- **invoice_token**: Address of the invoice token contract.
- **commitment**: SHA-256 hash of off-chain invoice data (immutable anchor).

**Constraints:**
- face_value and purchase_price must be positive (> 0)
- due_date must be non-zero and strictly greater than the current ledger timestamp
- Each invoice_id can only be used once
### `fund_escrow(invoice_id: Symbol, buyer: Address)`
### `record_payment(invoice_id: Symbol, payer: Address, amount: i128)`
Records a full or partial payment for a funded invoice.
- **amount**: Must be $> 0$ and $\le$ (initial amount - total already paid).
- Partial payments distribute the platform fee to the admin, the remainder to the investor, and release a proportional amount of the investor's initial funding to the seller.
- The invoice status transitions to `Settled` only when the total paid matches the invoice amount.

### `refund(invoice_id: Symbol)`
Returns the remaining (un-released) investor funding if the invoice is not paid by its due date.

### Events
- `platform_fee_updated(old_fee_bps: u32, new_fee_bps: u32)`
