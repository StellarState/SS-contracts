#![cfg(test)]

use super::{InvoiceToken, InvoiceTokenClient};
use crate::errors::Error;
use soroban_sdk::{
    contract, contractimpl, testutils::{Address as _, Events, Ledger},
    Address, Env, String as SorobanString, Symbol, Vec, IntoVal
};

#[contract]
pub struct MockSettlementEscrow;

#[contractimpl]
impl MockSettlementEscrow {
    pub fn settle_burn(env: Env, token_id: Address, from: Address, amount: i128) -> Result<(), Error> {
        let client = InvoiceTokenClient::new(&env, &token_id);
        match client.try_burn(&from, &amount) {
            Ok(Ok(())) => {
                env.events().publish((Symbol::new(&env, "settle_burn"), from), amount);
                Ok(())
            },
            Ok(Err(e)) => Err(e),
            Err(_) => panic!("contract call failed"),
        }
    }

    pub fn settle_burn_from(env: Env, token_id: Address, from: Address, amount: i128) -> Result<(), Error> {
        let client = InvoiceTokenClient::new(&env, &token_id);
        let spender = env.current_contract_address();
        match client.try_burn_from(&spender, &from, &amount) {
            Ok(Ok(())) => {
                env.events().publish((Symbol::new(&env, "settle_burn_from"), from), amount);
                Ok(())
            },
            Ok(Err(e)) => Err(e),
            Err(_) => panic!("contract call failed"),
        }
    }

    pub fn unlock_transfers(env: Env, token_id: Address) -> Result<(), Error> {
        let client = InvoiceTokenClient::new(&env, &token_id);
        let caller = env.current_contract_address();
        match client.try_set_transfer_locked(&caller, &false) {
            Ok(Ok(())) => {
                env.events().publish((Symbol::new(&env, "transfers_unlocked"),), false);
                Ok(())
            },
            Ok(Err(e)) => Err(e),
            Err(_) => panic!("contract call failed"),
        }
    }
}

fn setup_harness(env: &Env) -> (InvoiceTokenClient<'_>, MockSettlementEscrowClient<'_>, Address, Address) {
    let token_id = env.register(InvoiceToken, ());
    let token = InvoiceTokenClient::new(env, &token_id);
    
    let mock_id = env.register(MockSettlementEscrow, ());
    let mock = MockSettlementEscrowClient::new(env, &mock_id);
    
    let admin = Address::generate(env);
    
    token.initialize(
        &admin,
        &SorobanString::from_str(env, "Invoice Token"),
        &SorobanString::from_str(env, "INV"),
        &7,
        &Symbol::new(env, "INV_1"),
        &mock_id,
    );
    
    (token, mock, admin, mock_id)
}

#[test]
fn test_happy_path_burn_on_settlement() {
    let env = Env::default();
    env.mock_all_auths();
    let (token, mock, admin, mock_id) = setup_harness(&env);
    
    let user = Address::generate(&env);
    token.mint(&user, &1000, &admin);
    
    // Escrow calls settle_burn
    let res = mock.try_settle_burn(&token.address, &user, &400);
    assert_eq!(res, Ok(Ok(())));
    
    assert_eq!(token.balance(&user), 600);
    assert_eq!(token.total_supply(), 600);
    
    // Unlock transfers
    let res2 = mock.try_unlock_transfers(&token.address);
    assert_eq!(res2, Ok(Ok(())));
    assert!(!token.transfer_locked());
    
    // Verify residual balances can be transferred
    let other = Address::generate(&env);
    token.transfer(&user, &other, &100);
    assert_eq!(token.balance(&user), 500);
    assert_eq!(token.balance(&other), 100);
}

#[test]
fn test_happy_path_burn_from_on_settlement() {
    let env = Env::default();
    env.mock_all_auths();
    let (token, mock, admin, mock_id) = setup_harness(&env);
    
    let user = Address::generate(&env);
    token.mint(&user, &1000, &admin);
    
    let expiration = env.ledger().sequence() + 100;
    token.approve(&user, &mock_id, &500, &expiration);
    
    // Escrow calls settle_burn_from
    let res = mock.try_settle_burn_from(&token.address, &user, &400);
    assert_eq!(res, Ok(Ok(())));
    
    assert_eq!(token.balance(&user), 600);
    assert_eq!(token.total_supply(), 600);
    assert_eq!(token.allowance(&user, &mock_id), 100);
}

#[test]
fn test_failure_insufficient_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let (token, mock, admin, _mock_id) = setup_harness(&env);
    
    let user = Address::generate(&env);
    token.mint(&user, &100, &admin);
    
    let res = mock.try_settle_burn(&token.address, &user, &200);
    assert_eq!(res, Err(Ok(Error::InsufficientBalance)));
}

#[test]
fn test_failure_invalid_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (token, mock, admin, _mock_id) = setup_harness(&env);
    
    let user = Address::generate(&env);
    token.mint(&user, &100, &admin);
    
    let res = mock.try_settle_burn(&token.address, &user, &0);
    assert_eq!(res, Err(Ok(Error::InvalidAmount)));
    
    let res2 = mock.try_settle_burn(&token.address, &user, &-50);
    assert_eq!(res2, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_failure_insufficient_allowance() {
    let env = Env::default();
    env.mock_all_auths();
    let (token, mock, admin, mock_id) = setup_harness(&env);
    
    let user = Address::generate(&env);
    token.mint(&user, &1000, &admin);
    
    let expiration = env.ledger().sequence() + 100;
    token.approve(&user, &mock_id, &300, &expiration);
    
    let res = mock.try_settle_burn_from(&token.address, &user, &400);
    assert_eq!(res, Err(Ok(Error::InsufficientAllowance)));
}

#[test]
fn test_failure_allowance_expired() {
    let env = Env::default();
    env.mock_all_auths();
    let (token, mock, admin, mock_id) = setup_harness(&env);
    
    let user = Address::generate(&env);
    token.mint(&user, &1000, &admin);
    
    let expiration = env.ledger().sequence() + 10;
    token.approve(&user, &mock_id, &500, &expiration);
    
    // Advance ledger beyond expiration
    env.ledger().with_mut(|l| l.sequence = expiration + 1);
    
    let res = mock.try_settle_burn_from(&token.address, &user, &400);
    assert_eq!(res, Err(Ok(Error::AllowanceExpired)));
}

#[test]
fn test_failure_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (token, mock, admin, _mock_id) = setup_harness(&env);
    
    let user = Address::generate(&env);
    token.mint(&user, &1000, &admin);
    
    token.set_paused(&true);
    
    let res = mock.try_settle_burn(&token.address, &user, &400);
    assert_eq!(res, Err(Ok(Error::Paused)));
}

#[test]
fn test_event_emissions() {
    let env = Env::default();
    env.mock_all_auths();
    let (token, mock, admin, _mock_id) = setup_harness(&env);
    
    let user = Address::generate(&env);
    token.mint(&user, &1000, &admin);
    
    let _ = mock.settle_burn(&token.address, &user, &400);
    
    let events = env.events().all();
    let mock_events: Vec<_> = events.iter().filter(|e| e.0 == mock.address).collect();
    let token_events: Vec<_> = events.iter().filter(|e| e.0 == token.address).collect();
    
    assert!(mock_events.len() > 0);
    assert!(token_events.len() > 0);
    
    // Verify the specific settle_burn event on the mock contract
    let last_mock_event = mock_events.last().unwrap();
    let topics = last_mock_event.1;
    let data = last_mock_event.2;
    assert_eq!(topics, (Symbol::new(&env, "settle_burn"), user.clone()).into_val(&env));
    let amount: i128 = data.into_val(&env);
    assert_eq!(amount, 400);
}
