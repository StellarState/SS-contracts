//! Tests for stored emergency-approval invalidation (#692) and bounded settlement
//! proposal expiry and cancellation (#693).

#![allow(deprecated, unused_variables, dead_code, unused_mut, clippy::all)]

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};

/// Escrow with one funded legacy invoice and a 2-of-2 administrator set
/// (`admin`, `second_admin`). Returns the client, both admins and the invoice id.
fn setup_funded_invoice<'a>(
    env: &'a Env,
    id_byte: u8,
) -> (InvoiceEscrowClient<'a>, Address, Address, BytesN<32>) {
    env.mock_all_auths();
    let contract_id = env.register_contract(None, InvoiceEscrow);
    let client = InvoiceEscrowClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let second_admin = Address::generate(env);
    let invoice_id = BytesN::from_array(env, &[id_byte; 32]);
    client.initialize(&admin, &300);
    client.set_emergency_config(
        &admin,
        &MultiSigConfig {
            admins: Vec::from_array(env, [admin.clone(), second_admin.clone()]),
            threshold: 2,
        },
    );
    let investor = Address::generate(env);
    client.register_invoice(
        &invoice_id,
        &100_000,
        &80_000,
        &500,
        &1000,
        &BytesN::from_array(env, &[id_byte; 32]),
    );
    client.invest(&invoice_id, &investor, &80_000);
    client.finalise_funding(&invoice_id);
    (client, admin, second_admin, invoice_id)
}

// ---------- #692: emergency approvals bound to the configuration ----------

#[test]
fn emergency_approval_from_removed_signer_does_not_count_after_rotation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, InvoiceEscrow);
    let client = InvoiceEscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    let d = Address::generate(&env);
    let invoice = Symbol::new(&env, "rotate_inv");
    client.initialize(&admin, &300);
    client.set_emergency_config(
        &admin,
        &MultiSigConfig {
            admins: Vec::from_array(&env, [admin.clone(), b.clone(), c.clone()]),
            threshold: 2,
        },
    );
    // First approval under the original membership: one of two needed, nothing executes.
    assert_eq!(client.emergency_release(&admin, &invoice), false);

    // Rotate membership: `admin` is removed. Their earlier approval must stop counting.
    client.set_emergency_config(
        &admin,
        &MultiSigConfig {
            admins: Vec::from_array(&env, [b.clone(), c.clone(), d.clone()]),
            threshold: 2,
        },
    );
    // `b` is the only fresh approval, so the threshold of 2 is still not met.
    assert_eq!(client.emergency_release(&b, &invoice), false);
    // `admin` is no longer a member and cannot approve under the new configuration.
    assert_eq!(
        client.try_emergency_release(&admin, &invoice),
        Err(Ok(Error::NotEmergencyAdmin))
    );
}

#[test]
fn lowering_threshold_does_not_reuse_old_approvals() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, InvoiceEscrow);
    let client = InvoiceEscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let b = Address::generate(&env);
    let invoice = Symbol::new(&env, "lower_inv");
    let members = Vec::from_array(&env, [admin.clone(), b.clone()]);
    client.initialize(&admin, &300);
    client.set_emergency_config(
        &admin,
        &MultiSigConfig {
            admins: members.clone(),
            threshold: 2,
        },
    );
    assert_eq!(client.emergency_release(&admin, &invoice), false);

    // Same members, lower threshold. The stored approval from `admin` belongs to the old
    // generation, so it is discarded. `admin` approving again is therefore a fresh approval
    // (not AlreadyApproved), and under threshold 1 it reaches the threshold.
    client.set_emergency_config(
        &admin,
        &MultiSigConfig {
            admins: members.clone(),
            threshold: 1,
        },
    );
    // Execution needs an escrow, which this test does not create. EscrowNotFound therefore
    // shows the threshold was reached by the fresh approval, and no escrow state was touched.
    assert_eq!(
        client.try_emergency_release(&admin, &invoice),
        Err(Ok(Error::EscrowNotFound))
    );
}

#[test]
fn emergency_config_rejects_duplicate_members() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, InvoiceEscrow);
    let client = InvoiceEscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let b = Address::generate(&env);
    client.initialize(&admin, &300);
    assert_eq!(
        client.try_set_emergency_config(
            &admin,
            &MultiSigConfig {
                admins: Vec::from_array(&env, [b.clone(), b.clone()]),
                threshold: 2,
            },
        ),
        Err(Ok(Error::DuplicateEmergencyAdmin))
    );
    // The rejected configuration was never stored, so emergency release still reports
    // that no configuration exists.
    assert_eq!(
        client.try_emergency_release(&b, &Symbol::new(&env, "dup_inv")),
        Err(Ok(Error::EmergencyNotConfigured))
    );
}

#[test]
fn fresh_approvals_meet_threshold_under_new_valid_configuration() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, InvoiceEscrow);
    let client = InvoiceEscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    let invoice = Symbol::new(&env, "fresh_inv");
    client.initialize(&admin, &300);
    client.set_emergency_config(
        &admin,
        &MultiSigConfig {
            admins: Vec::from_array(&env, [admin.clone(), b.clone()]),
            threshold: 2,
        },
    );
    assert_eq!(client.emergency_release(&admin, &invoice), false);
    client.set_emergency_config(
        &admin,
        &MultiSigConfig {
            admins: Vec::from_array(&env, [b.clone(), c.clone()]),
            threshold: 2,
        },
    );
    assert_eq!(client.emergency_release(&b, &invoice), false);
    // The second fresh approval meets the threshold. Execution needs an escrow, which this
    // test does not create, so EscrowNotFound confirms the threshold was reached.
    assert_eq!(
        client.try_emergency_release(&c, &invoice),
        Err(Ok(Error::EscrowNotFound))
    );
}

// ---------- #693: bounded expiry and cancellation of settlement proposals ----------

#[test]
fn cancelled_proposal_can_be_replaced_and_keeps_repayment_state() {
    let env = Env::default();
    let (client, admin, second_admin, invoice_id) = setup_funded_invoice(&env, 0x51);
    client.propose_settlement(&admin, &invoice_id, &100_000);
    client.cancel_settlement(&admin, &invoice_id);

    assert_eq!(
        client.try_get_pending_settlement(&invoice_id),
        Err(Ok(Error::SettlementNotProposed))
    );
    // Cancellation moves no funds and does not finalise the invoice.
    assert_eq!(
        client.get_invoice_record(&invoice_id).status,
        EscrowStatus::Funded
    );

    // A replacement can be proposed, here by the other administrator.
    client.propose_settlement(&second_admin, &invoice_id, &100_000);
    let pending = client.get_pending_settlement(&invoice_id);
    assert_eq!(pending.proposer, second_admin);
    assert_eq!(pending.repayment_amount, 100_000);
    client.approve_settlement(&admin, &invoice_id);
    assert_eq!(
        client.get_invoice_record(&invoice_id).status,
        EscrowStatus::Settled
    );
}

#[test]
fn current_administrator_can_cancel_another_admins_proposal() {
    let env = Env::default();
    let (client, admin, second_admin, invoice_id) = setup_funded_invoice(&env, 0x52);
    // `second_admin` proposes, and the configured administrator (`admin`) cancels it.
    client.propose_settlement(&second_admin, &invoice_id, &100_000);
    client.cancel_settlement(&admin, &invoice_id);
    assert_eq!(
        client.try_get_pending_settlement(&invoice_id),
        Err(Ok(Error::SettlementNotProposed))
    );
}

#[test]
fn unauthorized_cancel_keeps_proposal_and_repayment_state() {
    let env = Env::default();
    let (client, admin, _second_admin, invoice_id) = setup_funded_invoice(&env, 0x53);
    let outsider = Address::generate(&env);
    client.propose_settlement(&admin, &invoice_id, &100_000);

    assert_eq!(
        client.try_cancel_settlement(&outsider, &invoice_id),
        Err(Ok(Error::Unauthorized))
    );
    let pending = client.get_pending_settlement(&invoice_id);
    assert_eq!(pending.proposer, admin);
    assert_eq!(pending.repayment_amount, 100_000);
    assert_eq!(
        client.get_invoice_record(&invoice_id).status,
        EscrowStatus::Funded
    );
}

#[test]
fn duplicate_cancellation_is_rejected() {
    let env = Env::default();
    let (client, admin, _second_admin, invoice_id) = setup_funded_invoice(&env, 0x54);
    client.propose_settlement(&admin, &invoice_id, &100_000);
    client.cancel_settlement(&admin, &invoice_id);
    assert_eq!(
        client.try_cancel_settlement(&admin, &invoice_id),
        Err(Ok(Error::SettlementNotProposed))
    );
}

#[test]
fn expired_proposal_cannot_be_approved_and_can_be_replaced() {
    let env = Env::default();
    let (client, admin, second_admin, invoice_id) = setup_funded_invoice(&env, 0x55);
    client.propose_settlement(&admin, &invoice_id, &100_000);
    let expires_at = client.get_pending_settlement(&invoice_id).expires_at_ledger;

    // On the boundary ledger the proposal is not yet expired, so expiry is refused.
    env.ledger().with_mut(|li| li.sequence_number = expires_at);
    assert_eq!(
        client.try_expire_settlement(&invoice_id),
        Err(Ok(Error::SettlementNotExpired))
    );

    // One ledger past the boundary: approval is refused and the proposal stays stored.
    env.ledger()
        .with_mut(|li| li.sequence_number = expires_at + 1);
    assert_eq!(
        client.try_approve_settlement(&second_admin, &invoice_id),
        Err(Ok(Error::SettlementProposalExpired))
    );
    assert_eq!(
        client.get_pending_settlement(&invoice_id).repayment_amount,
        100_000
    );

    client.expire_settlement(&invoice_id);
    assert_eq!(
        client.try_get_pending_settlement(&invoice_id),
        Err(Ok(Error::SettlementNotProposed))
    );
    assert_eq!(
        client.get_invoice_record(&invoice_id).status,
        EscrowStatus::Funded
    );

    // A new proposal can be made after expiry.
    client.propose_settlement(&second_admin, &invoice_id, &100_000);
    assert_eq!(
        client.get_pending_settlement(&invoice_id).proposer,
        second_admin
    );
}

#[test]
fn proposal_is_still_approvable_at_exact_expiry_ledger() {
    let env = Env::default();
    let (client, admin, second_admin, invoice_id) = setup_funded_invoice(&env, 0x56);
    client.propose_settlement(&admin, &invoice_id, &100_000);
    let expires_at = client.get_pending_settlement(&invoice_id).expires_at_ledger;
    env.ledger().with_mut(|li| li.sequence_number = expires_at);
    client.approve_settlement(&second_admin, &invoice_id);
    assert_eq!(
        client.get_invoice_record(&invoice_id).status,
        EscrowStatus::Settled
    );
}
