#![no_std]

mod errors;
mod events;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
pub use events::emit_bulk_orders_cancelled;
pub use types::{BuyOrder, OrderStatus, StorageKey};

use soroban_sdk::{contract, contractimpl, token, vec, Address, BytesN, Env, Vec};

const MAX_BATCH_SIZE: u32 = 20;

#[contract]
pub struct BulkCancel;

#[contractimpl]
impl BulkCancel {
    pub fn initialize(env: Env, token: Address) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Token) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&StorageKey::Token, &token);
        env.storage().instance().set(&StorageKey::NextOrderId, &1u32);
        Ok(())
    }

    pub fn place_buy_order(
        env: Env,
        buyer: Address,
        invoice_id: BytesN<32>,
        amount: i128,
        escrowed_funds: i128,
    ) -> Result<u32, Error> {
        buyer.require_auth();

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Token)
            .ok_or(Error::NotInitialized)?;

        let token_client = token::Client::new(&env, &token_addr);
        token_client.transfer(&buyer, &env.current_contract_address(), &escrowed_funds);

        let order_id: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::NextOrderId)
            .unwrap_or(1u32);
        env.storage()
            .instance()
            .set(&StorageKey::NextOrderId, &(order_id + 1));

        let order = BuyOrder {
            order_id,
            buyer: buyer.clone(),
            invoice_id: invoice_id.clone(),
            amount,
            escrowed_funds,
            status: OrderStatus::Open,
        };

        env.storage()
            .persistent()
            .set(&StorageKey::Order(order_id), &order);

        let mut inv_orders: Vec<u32> = env
            .storage()
            .persistent()
            .get(&StorageKey::UserInvoiceOrders(buyer.clone(), invoice_id.clone()))
            .unwrap_or(vec![&env]);
        inv_orders.push_back(order_id);
        env.storage().persistent().set(
            &StorageKey::UserInvoiceOrders(buyer.clone(), invoice_id),
            &inv_orders,
        );

        let mut all_orders: Vec<u32> = env
            .storage()
            .persistent()
            .get(&StorageKey::UserAllOrders(buyer.clone()))
            .unwrap_or(vec![&env]);
        all_orders.push_back(order_id);
        env.storage()
            .persistent()
            .set(&StorageKey::UserAllOrders(buyer), &all_orders);

        Ok(order_id)
    }

    /// Cancels all open buy orders for the wallet on the given invoice and refunds escrow atomically.
    pub fn cancel_all_orders(
        env: Env,
        buyer: Address,
        invoice_id: BytesN<32>,
    ) -> Result<(u32, i128), Error> {
        buyer.require_auth();

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Token)
            .ok_or(Error::NotInitialized)?;

        let order_ids: Vec<u32> = env
            .storage()
            .persistent()
            .get(&StorageKey::UserInvoiceOrders(buyer.clone(), invoice_id))
            .unwrap_or(vec![&env]);

        let mut count = 0u32;
        let mut total_refunded = 0i128;

        for id in order_ids.iter() {
            if let Some(mut order) = env
                .storage()
                .persistent()
                .get::<StorageKey, BuyOrder>(&StorageKey::Order(id))
            {
                if order.buyer == buyer && order.status == OrderStatus::Open {
                    order.status = OrderStatus::Cancelled;
                    total_refunded += order.escrowed_funds;
                    count += 1;
                    env.storage()
                        .persistent()
                        .set(&StorageKey::Order(id), &order);
                }
            }
        }

        if total_refunded > 0 {
            let token_client = token::Client::new(&env, &token_addr);
            token_client.transfer(&env.current_contract_address(), &buyer, &total_refunded);
        }

        emit_bulk_orders_cancelled(&env, &buyer, count, total_refunded);

        Ok((count, total_refunded))
    }

    /// Cancels a specific list of order IDs up to 20 per call.
    pub fn cancel_orders_batch(
        env: Env,
        buyer: Address,
        order_ids: Vec<u32>,
    ) -> Result<(u32, i128), Error> {
        buyer.require_auth();

        if order_ids.len() > MAX_BATCH_SIZE {
            return Err(Error::BatchTooLarge);
        }
        if order_ids.is_empty() {
            return Err(Error::EmptyBatch);
        }

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Token)
            .ok_or(Error::NotInitialized)?;

        let mut count = 0u32;
        let mut total_refunded = 0i128;

        for id in order_ids.iter() {
            let mut order: BuyOrder = env
                .storage()
                .persistent()
                .get(&StorageKey::Order(id))
                .ok_or(Error::OrderNotFound)?;

            if order.buyer != buyer {
                return Err(Error::Unauthorized);
            }
            if order.status != OrderStatus::Open {
                return Err(Error::OrderNotOpen);
            }

            order.status = OrderStatus::Cancelled;
            total_refunded += order.escrowed_funds;
            count += 1;

            env.storage()
                .persistent()
                .set(&StorageKey::Order(id), &order);
        }

        if total_refunded > 0 {
            let token_client = token::Client::new(&env, &token_addr);
            token_client.transfer(&env.current_contract_address(), &buyer, &total_refunded);
        }

        emit_bulk_orders_cancelled(&env, &buyer, count, total_refunded);

        Ok((count, total_refunded))
    }

    pub fn get_order(env: Env, order_id: u32) -> Option<BuyOrder> {
        env.storage().persistent().get(&StorageKey::Order(order_id))
    }

    pub fn get_user_invoice_orders(
        env: Env,
        buyer: Address,
        invoice_id: BytesN<32>,
    ) -> Vec<u32> {
        env.storage()
            .persistent()
            .get(&StorageKey::UserInvoiceOrders(buyer, invoice_id))
            .unwrap_or(vec![&env])
    }
}
