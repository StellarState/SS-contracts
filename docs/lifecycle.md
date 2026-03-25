# Invoice Escrow Lifecycle

This document describes the intended end-to-end lifecycle of an invoice and how it flows through the `invoice-escrow` and `invoice-token` contracts.

## 1. Creation (`create_escrow`)
- **Actor:** Seller (invoice owner)
- **Action:** The seller registers a new invoice in the `invoice-escrow` contract.
- **State Changes:** Escrow status is set to `Created`. The contract tracks the `payment_token` (stablecoin used for payment) and `invoice_token` (SEP-41 token used to represent ownership).
- **Tokenization:** No tokens are minted at this stage.

## 2. Funding (`fund_escrow`)
- **Actor:** Buyer / Investor
- **Action:** The investor funds the invoice by transferring the required `payment_token` amount to the escrow contract.
- **State Changes:** Escrow status is updated to `Funded`. The investor is recorded as the `funder`.
- **Tokenization:** The `invoice-escrow` contract uses its minter authorization to call `mint` on the `invoice-token` contract. The investor receives the full `amount` of invoice tokens in their wallet to represent fractional or complete ownership.

## 3. Settlement (`record_payment`)
- **Actor:** Payer
- **Action:** The payer settles the invoice by sending an `amount` in `payment_token` to the escrow contract.
- **Distribution:**
    - The expected `platform_fee` (based on `amount`) is distributed to the admin.
    - The rest of the `amount` is distributed to the investor.
    - **Proportional Release:** An amount equal to the payer's `amount` is released from the contract's initial funding balance (investor's collateral) to the **Seller**.
- **State Changes:** Escrow status becomes `Settled` only if the total amount paid matches the original `data.amount`. Otherwise, status remains `Funded` to allow for further payments.
- **Tokenization:** After **complete** settlement, the escrow contract calls `set_transfer_locked(false)` on the `invoice-token` contract.

## 4. Refund (`refund`)
- **Actor:** Anyone (typically Investor or Admin)
- **Action:** Triggered if a funded invoice is past its `due_dt` and was not fully paid.
- **Action:** The **remaining** initial funded amount (collateral not yet released to the seller) is refunded to the investor.
- **State Changes:** Escrow status becomes `Refunded`.
- **Tokenization:** After returning remaining funds to the investor, the escrow contract calls `set_transfer_locked(false)` on the `invoice-token` contract.

## Transfer Lock Policy

The `invoice-token` transfer lock follows the escrow lifecycle:

| Escrow State | `transfer_locked` | Who Can Transfer |
|---|---|---|
| `Created` | `true` | Admin only |
| `Funded` | `true` | Admin only |
| `Settled` | `false` | All holders |
| `Refunded` | `false` | All holders |

**Access control:** Only the token `admin` or the designated `minter` (the escrow contract address) may call `set_transfer_locked`. This ensures only the escrow lifecycle — not arbitrary parties — can change the lock state after initialization.
