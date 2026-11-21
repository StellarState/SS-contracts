//! Invoice metadata registry contract for StellarSettle.
//!
//! Stores key invoice parameters on-chain (face value, yield, maturity date,
//! issuer, lifecycle status), accessible by other contracts and indexers
//! without reconstructing the invoice from off-chain records.

#![no_std]

mod errors;
mod events;
mod storage;
mod types;

pub use types::{InvoiceMetadata, InvoiceMetadataInput, InvoiceStatus, MetadataRevision};

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String, Vec};

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
pub struct InvoiceMetadataRegistry;

#[contractimpl]
impl InvoiceMetadataRegistry {
    /// Initialize the registry with the admin allowed to register invoices
    /// and transition their status.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        ensure_non_zero_address(&env, &admin)?;
        if storage::get_admin(&env).is_some() {
            return Err(Error::AlreadyInit);
        }
        storage::set_admin(&env, &admin);
        events::initialized(&env, &admin);
        Ok(())
    }

    /// Admin-only: register a new invoice with its full metadata. Starts in
    /// `Pending` status. Fails if `invoice_id` is already registered.
    pub fn register_invoice(
        env: Env,
        admin: Address,
        invoice_id: BytesN<32>,
        metadata: InvoiceMetadataInput,
    ) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();
        ensure_non_zero_address(&env, &metadata.issuer)?;

        if metadata.face_value <= 0 || metadata.yield_amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if storage::get_invoice(&env, &invoice_id).is_some() {
            return Err(Error::AlreadyRegistered);
        }

        let record = InvoiceMetadata {
            invoice_id: invoice_id.clone(),
            face_value: metadata.face_value,
            yield_amount: metadata.yield_amount,
            maturity_date: metadata.maturity_date,
            issuer: metadata.issuer.clone(),
            status: InvoiceStatus::Pending,
            metadata_editor: metadata.issuer.clone(),
            revision: 1,
        };
        storage::set_invoice(&env, &invoice_id, &record);
        let mut revisions = Vec::new(&env);
        revisions.push_back(MetadataRevision {
            revision: 1,
            metadata: metadata.clone(),
            updated_by: admin,
            timestamp: env.ledger().timestamp(),
        });
        storage::set_revisions(&env, &invoice_id, &revisions);
        events::invoice_registered(&env, &invoice_id, &metadata.issuer);
        Ok(())
    }

    /// The issuer assigns the account allowed to edit this invoice's metadata.
    pub fn set_metadata_editor(
        env: Env,
        issuer: Address,
        invoice_id: BytesN<32>,
        editor: Address,
    ) -> Result<(), Error> {
        let mut record = storage::get_invoice(&env, &invoice_id).ok_or(Error::NotRegistered)?;
        if issuer != record.issuer {
            return Err(Error::Unauthorized);
        }
        issuer.require_auth();
        ensure_non_zero_address(&env, &editor)?;
        record.metadata_editor = editor;
        storage::set_invoice(&env, &invoice_id, &record);
        Ok(())
    }

    /// Update invoice metadata before funding. Funding administration does not
    /// grant metadata-editing authority.
    pub fn update_metadata(
        env: Env,
        editor: Address,
        invoice_id: BytesN<32>,
        metadata: InvoiceMetadataInput,
    ) -> Result<u32, Error> {
        let mut record = storage::get_invoice(&env, &invoice_id).ok_or(Error::NotRegistered)?;
        if editor != record.metadata_editor {
            return Err(Error::Unauthorized);
        }
        editor.require_auth();
        if !matches!(record.status, InvoiceStatus::Pending | InvoiceStatus::Live) {
            return Err(Error::MetadataLocked);
        }
        ensure_non_zero_address(&env, &metadata.issuer)?;
        if metadata.face_value <= 0 || metadata.yield_amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if metadata.issuer != record.issuer {
            return Err(Error::Unauthorized);
        }

        let revisions = storage::get_revisions(&env, &invoice_id).ok_or(Error::NotRegistered)?;
        let old_revision = record.revision;
        let new_revision = record
            .revision
            .checked_add(1)
            .ok_or(Error::RevisionOverflow)?;
        record.face_value = metadata.face_value;
        record.yield_amount = metadata.yield_amount;
        record.maturity_date = metadata.maturity_date;
        record.issuer = metadata.issuer.clone();
        record.revision = new_revision;
        storage::set_invoice(&env, &invoice_id, &record);

        let mut updated_revisions = revisions;
        updated_revisions.push_back(MetadataRevision {
            revision: new_revision,
            metadata,
            updated_by: editor,
            timestamp: env.ledger().timestamp(),
        });
        storage::set_revisions(&env, &invoice_id, &updated_revisions);
        events::metadata_updated(&env, &invoice_id, old_revision, new_revision);
        Ok(new_revision)
    }

    /// Admin-only: move an invoice to `new_status`. Rejected with
    /// `InvalidTransition` if the move is not a valid edge in the invoice
    /// state machine (see `InvoiceStatus::can_transition_to`).
    pub fn update_status(
        env: Env,
        admin: Address,
        invoice_id: BytesN<32>,
        new_status: InvoiceStatus,
    ) -> Result<(), Error> {
        let stored_admin = storage::get_admin(&env).ok_or(Error::NotInit)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();

        let mut record = storage::get_invoice(&env, &invoice_id).ok_or(Error::NotRegistered)?;
        if !record.status.can_transition_to(new_status) {
            return Err(Error::InvalidTransition);
        }

        let old_status = record.status;
        record.status = new_status;
        storage::set_invoice(&env, &invoice_id, &record);
        events::invoice_status_updated(&env, &invoice_id, old_status, new_status);
        Ok(())
    }

    /// View: return an invoice's full metadata at its current state.
    pub fn get_invoice(env: Env, invoice_id: BytesN<32>) -> Result<InvoiceMetadata, Error> {
        storage::get_invoice(&env, &invoice_id).ok_or(Error::NotRegistered)
    }

    /// Return the current revision number for an invoice.
    pub fn get_current_revision(env: Env, invoice_id: BytesN<32>) -> Result<u32, Error> {
        Ok(storage::get_invoice(&env, &invoice_id)
            .ok_or(Error::NotRegistered)?
            .revision)
    }

    /// Return the append-only metadata revision history for an invoice.
    pub fn get_revision_history(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<Vec<MetadataRevision>, Error> {
        storage::get_revisions(&env, &invoice_id).ok_or(Error::NotRegistered)
    }

    /// View: return the current admin.
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        storage::get_admin(&env).ok_or(Error::NotInit)
    }
}

#[cfg(test)]
mod test;
