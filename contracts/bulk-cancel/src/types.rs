use soroban_sdk::{contracttype, Address, BytesN};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrderStatus {
    Open,
    Filled,
    Cancelled,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuyOrder {
    pub order_id: u32,
    pub buyer: Address,
    pub invoice_id: BytesN<32>,
    pub amount: i128,
    pub escrowed_funds: i128,
    pub status: OrderStatus,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Token,
    NextOrderId,
    Order(u32),
    UserInvoiceOrders(Address, BytesN<32>),
    UserAllOrders(Address),
}
