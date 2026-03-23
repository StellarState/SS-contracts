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
- **Action:** The payer settles the invoice by sending the `amount` in `payment_token` to the escrow contract. The expected `platform_fee` is distributed to the admin, and the rest is distributed to the investor.
- **State Changes:** Escrow status becomes `Settled`.
- **Tokenization:** Invoice tokens remain in the investor's wallet but the escrow contract marks the invoice as fully settled. Transferability of these tokens remains locked by default (as per token initialization) unless global transfer policies are updated by the platform admin.

## 4. Refund (`refund`)
- **Actor:** Anyone (typically Investor or Admin)
- **Action:** Triggered if a funded invoice is past its `due_dt` and was not paid. The initial funded amount is refunded to the investor.
- **State Changes:** Escrow status becomes `Refunded`.
- **Tokenization:** Invoice tokens remain linked to the user account as a claim record, but the escrow contract will no longer accept payer funds for this invoice.
