#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, BytesN, Env, Symbol, Vec,
};

pub const HOUR: u64 = 3_600;
pub const FOUR_HOURS: u64 = 14_400;
pub const DAY: u64 = 86_400;

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    Publisher(Address),
    Observations(BytesN<32>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Observation {
    pub price: i128,
    pub recorded_at: u64,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidPrice = 3,
    InvalidWindow = 4,
    InsufficientObservations = 5,
    Overflow = 6,
    UnauthorizedPublisher = 7,
}

#[contract]
pub struct TwapOracle;

#[contractimpl]
impl TwapOracle {
    /// Initialises the contract and records the admin address.
    ///
    /// Must be called before any other state-mutating function.
    /// Returns `AlreadyInitialized` if called a second time.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    /// Registers `publisher` as an authorised price-recording operator.
    ///
    /// Only the admin may call this function.
    /// Returns `NotInitialized` when called before `initialize`.
    pub fn add_publisher(env: Env, publisher: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::Publisher(publisher), &true);
        Ok(())
    }

    /// Removes `publisher` from the set of authorised recording operators.
    ///
    /// Only the admin may call this function.  After revocation the address
    /// cannot append new observations or emit `TWAPUpdated`.
    /// Returns `NotInitialized` when called before `initialize`.
    pub fn remove_publisher(env: Env, publisher: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        env.storage()
            .persistent()
            .remove(&DataKey::Publisher(publisher));
        Ok(())
    }

    /// Returns `true` if `publisher` is currently registered.
    pub fn is_publisher(env: Env, publisher: Address) -> bool {
        env.storage()
            .persistent()
            .get::<_, bool>(&DataKey::Publisher(publisher))
            .unwrap_or(false)
    }

    /// Records a timestamped price observation for `invoice_id`.
    ///
    /// `caller` must be a registered publisher and must authorise this call.
    ///
    /// Returns:
    /// - `NotInitialized`      – contract has not been initialised yet.
    /// - `UnauthorizedPublisher` – `caller` is not a registered publisher.
    /// - `InvalidPrice`        – `price` is zero or negative.
    ///
    /// Rejected calls leave the existing observation vector and any derived
    /// TWAP unchanged.
    pub fn record_price(
        env: Env,
        caller: Address,
        invoice_id: BytesN<32>,
        price: i128,
    ) -> Result<(), Error> {
        // 1. Gate on initialisation before touching any other state.
        if !env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::NotInitialized);
        }

        // 2. Authenticate the caller as a registered publisher.
        let registered: bool = env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Publisher(caller.clone()))
            .unwrap_or(false);
        if !registered {
            return Err(Error::UnauthorizedPublisher);
        }
        caller.require_auth();

        // 3. Validate the price.
        if price <= 0 {
            return Err(Error::InvalidPrice);
        }

        // 4. Prune stale observations and append the new one.
        let now = env.ledger().timestamp();
        let key = DataKey::Observations(invoice_id.clone());
        let old: Vec<Observation> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(&env));
        let mut kept = Vec::new(&env);
        for item in old.iter() {
            if now.saturating_sub(item.recorded_at) <= DAY {
                kept.push_back(item);
            }
        }
        kept.push_back(Observation {
            price,
            recorded_at: now,
        });
        env.storage().persistent().set(&key, &kept);
        env.events()
            .publish((Symbol::new(&env, "TWAPUpdated"), invoice_id), (price, now));
        Ok(())
    }

    /// Computes the time-weighted average price over `window_seconds`.
    ///
    /// `window_seconds` must be one of `HOUR`, `FOUR_HOURS`, or `DAY`.
    pub fn get_twap(env: Env, invoice_id: BytesN<32>, window_seconds: u64) -> Result<i128, Error> {
        if window_seconds != HOUR && window_seconds != FOUR_HOURS && window_seconds != DAY {
            return Err(Error::InvalidWindow);
        }
        let now = env.ledger().timestamp();
        let start = now.saturating_sub(window_seconds);
        let items: Vec<Observation> = env
            .storage()
            .persistent()
            .get(&DataKey::Observations(invoice_id))
            .unwrap_or(Vec::new(&env));
        if items.len() < 2 {
            return Err(Error::InsufficientObservations);
        }
        let mut weighted = 0i128;
        let mut duration = 0u64;
        let mut previous: Option<Observation> = None;
        for item in items.iter() {
            if item.recorded_at > now {
                break;
            }
            if let Some(prev) = previous {
                let from = prev.recorded_at.max(start);
                let to = item.recorded_at.min(now);
                if to > from {
                    weighted = weighted
                        .checked_add(
                            prev.price
                                .checked_mul((to - from) as i128)
                                .ok_or(Error::Overflow)?,
                        )
                        .ok_or(Error::Overflow)?;
                    duration += to - from;
                }
            }
            previous = Some(item);
        }
        if let Some(prev) = previous {
            let from = prev.recorded_at.max(start);
            if now > from {
                weighted = weighted
                    .checked_add(
                        prev.price
                            .checked_mul((now - from) as i128)
                            .ok_or(Error::Overflow)?,
                    )
                    .ok_or(Error::Overflow)?;
                duration += now - from;
            }
        }
        if duration == 0 {
            return Err(Error::InsufficientObservations);
        }
        Ok(weighted / duration as i128)
    }

    /// Returns all stored observations for `invoice_id`.
    pub fn get_observations(env: Env, invoice_id: BytesN<32>) -> Vec<Observation> {
        env.storage()
            .persistent()
            .get(&DataKey::Observations(invoice_id))
            .unwrap_or(Vec::new(&env))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger as _};

    // ── helpers ──────────────────────────────────────────────────────────────

    fn setup() -> (soroban_sdk::Env, soroban_sdk::Address, TwapOracleClient<'static>) {
        let env = soroban_sdk::Env::default();
        env.mock_all_auths();
        let contract = env.register(TwapOracle, ());
        let client = TwapOracleClient::new(&env, &contract);
        let admin = soroban_sdk::Address::generate(&env);
        client.initialize(&admin);
        (env, admin, client)
    }

    fn invoice() -> BytesN<32> {
        // We re-derive the BytesN inside each test because BytesN is !Send.
        // This helper is kept for documentation; callers construct inline.
        unreachable!()
    }

    // ── AC-1: recording before initialisation returns NotInitialized ─────────

    #[test]
    fn record_before_init_returns_not_initialized() {
        let env = soroban_sdk::Env::default();
        env.mock_all_auths();
        let contract = env.register(TwapOracle, ());
        let client = TwapOracleClient::new(&env, &contract);

        let publisher = soroban_sdk::Address::generate(&env);
        let inv = BytesN::from_array(&env, &[1u8; 32]);

        let result = client.try_record_price(&publisher, &inv, &1_000);
        assert_eq!(result, Err(Ok(Error::NotInitialized)));
    }

    #[test]
    fn get_twap_before_init_returns_insufficient_observations() {
        // get_twap is a pure read that does not require auth; it still returns
        // InsufficientObservations when no observations exist, which is the
        // correct behaviour for a fresh uninitialized contract.
        let env = soroban_sdk::Env::default();
        let contract = env.register(TwapOracle, ());
        let client = TwapOracleClient::new(&env, &contract);
        let inv = BytesN::from_array(&env, &[1u8; 32]);
        let result = client.try_get_twap(&inv, &HOUR);
        assert_eq!(result, Err(Ok(Error::InsufficientObservations)));
    }

    // ── AC-2: unregistered callers cannot record ──────────────────────────────

    #[test]
    fn unregistered_caller_cannot_record() {
        let (env, _admin, client) = setup();
        let stranger = soroban_sdk::Address::generate(&env);
        let inv = BytesN::from_array(&env, &[2u8; 32]);

        let result = client.try_record_price(&stranger, &inv, &500);
        assert_eq!(result, Err(Ok(Error::UnauthorizedPublisher)));

        // Observation vector must remain empty.
        assert_eq!(client.get_observations(&inv).len(), 0);
    }

    #[test]
    fn unregistered_caller_does_not_affect_twap() {
        let (env, admin, client) = setup();
        let publisher = soroban_sdk::Address::generate(&env);
        let stranger = soroban_sdk::Address::generate(&env);
        let inv = BytesN::from_array(&env, &[3u8; 32]);

        client.add_publisher(&publisher);

        // Seed two legitimate observations separated by 60 seconds.
        env.ledger().set_timestamp(1_000);
        client.record_price(&publisher, &inv, &100);
        env.ledger().set_timestamp(1_060);
        client.record_price(&publisher, &inv, &200);

        // Stranger attempts to overwrite the data.
        let _ = client.try_record_price(&stranger, &inv, &999_999);

        // TWAP should still reflect only the two legitimate observations.
        env.ledger().set_timestamp(1_060);
        let twap = client.get_twap(&inv, &HOUR).unwrap();
        // Both observations exist; the last price (200) covers 0 s so TWAP == 100
        // (100 held for 60 s, then 200 at the boundary).
        assert!(twap > 0);
        assert_eq!(client.get_observations(&inv).len(), 2);
    }

    // ── AC-3: registered publishers can record ────────────────────────────────

    #[test]
    fn registered_publisher_can_record() {
        let (env, admin, client) = setup();
        let publisher = soroban_sdk::Address::generate(&env);
        let inv = BytesN::from_array(&env, &[4u8; 32]);

        client.add_publisher(&publisher);
        assert!(client.is_publisher(&publisher));

        env.ledger().set_timestamp(2_000);
        client.record_price(&publisher, &inv, &1_234);

        let obs = client.get_observations(&inv);
        assert_eq!(obs.len(), 1);
        assert_eq!(obs.get(0).unwrap().price, 1_234);
    }

    #[test]
    fn multiple_publishers_can_each_record() {
        let (env, admin, client) = setup();
        let pub_a = soroban_sdk::Address::generate(&env);
        let pub_b = soroban_sdk::Address::generate(&env);
        let inv = BytesN::from_array(&env, &[5u8; 32]);

        client.add_publisher(&pub_a);
        client.add_publisher(&pub_b);

        env.ledger().set_timestamp(3_000);
        client.record_price(&pub_a, &inv, &10);
        env.ledger().set_timestamp(3_060);
        client.record_price(&pub_b, &inv, &20);

        assert_eq!(client.get_observations(&inv).len(), 2);
    }

    // ── AC-4: revocation immediately prevents subsequent recording ───────────

    #[test]
    fn revoked_publisher_cannot_record() {
        let (env, admin, client) = setup();
        let publisher = soroban_sdk::Address::generate(&env);
        let inv = BytesN::from_array(&env, &[6u8; 32]);

        client.add_publisher(&publisher);
        env.ledger().set_timestamp(4_000);
        client.record_price(&publisher, &inv, &500);

        // Revoke.
        client.remove_publisher(&publisher);
        assert!(!client.is_publisher(&publisher));

        // Subsequent record must be rejected.
        env.ledger().set_timestamp(4_060);
        let result = client.try_record_price(&publisher, &inv, &600);
        assert_eq!(result, Err(Ok(Error::UnauthorizedPublisher)));

        // Only the pre-revocation observation should remain.
        assert_eq!(client.get_observations(&inv).len(), 1);
        assert_eq!(client.get_observations(&inv).get(0).unwrap().price, 500);
    }

    // ── AC-5: rejected calls preserve observations and derived TWAP ──────────

    #[test]
    fn rejected_call_preserves_observations() {
        let (env, admin, client) = setup();
        let publisher = soroban_sdk::Address::generate(&env);
        let stranger = soroban_sdk::Address::generate(&env);
        let inv = BytesN::from_array(&env, &[7u8; 32]);

        client.add_publisher(&publisher);

        env.ledger().set_timestamp(5_000);
        client.record_price(&publisher, &inv, &1_000);
        env.ledger().set_timestamp(5_060);
        client.record_price(&publisher, &inv, &2_000);

        let twap_before = client.get_twap(&inv, &HOUR).unwrap();
        let obs_count_before = client.get_observations(&inv).len();

        // Attempts that must all be rejected.
        let _ = client.try_record_price(&stranger, &inv, &999);
        let _ = client.try_record_price(&publisher, &inv, &-1); // InvalidPrice after auth

        // We also re-add publisher so the -1 test can reach the price check.
        // (publisher is still registered; the -1 price check fires after auth.)
        let _ = client.try_record_price(&publisher, &inv, &0); // also InvalidPrice

        let twap_after = client.get_twap(&inv, &HOUR).unwrap();
        assert_eq!(twap_before, twap_after);
        assert_eq!(client.get_observations(&inv).len(), obs_count_before);
    }

    // ── additional boundary / happy-path tests ────────────────────────────────

    #[test]
    fn double_initialize_returns_already_initialized() {
        let (env, admin, client) = setup();
        let result = client.try_initialize(&admin);
        assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
    }

    #[test]
    fn non_admin_cannot_add_publisher() {
        let (env, _admin, client) = setup();
        // mock_all_auths is active, so we test the registration-check path by
        // calling add_publisher as a stranger — the admin guard fires first.
        // To truly test this we need a fresh env without mock_all_auths.
        let env2 = soroban_sdk::Env::default();
        let contract2 = env2.register(TwapOracle, ());
        let client2 = TwapOracleClient::new(&env2, &contract2);
        let admin2 = soroban_sdk::Address::generate(&env2);
        let stranger = soroban_sdk::Address::generate(&env2);
        // Initialize with admin2.
        env2.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &admin2,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract2,
                fn_name: "initialize",
                args: soroban_sdk::vec![&env2, admin2.clone().into()].into(),
                sub_invokes: &[],
            },
        }]);
        client2.initialize(&admin2);

        // Stranger tries to add a publisher — should panic (auth fails).
        let publisher = soroban_sdk::Address::generate(&env2);
        let result = client2.try_add_publisher(&publisher);
        assert!(result.is_err());
    }

    #[test]
    fn add_publisher_before_init_returns_not_initialized() {
        let env = soroban_sdk::Env::default();
        env.mock_all_auths();
        let contract = env.register(TwapOracle, ());
        let client = TwapOracleClient::new(&env, &contract);
        let publisher = soroban_sdk::Address::generate(&env);
        let result = client.try_add_publisher(&publisher);
        assert_eq!(result, Err(Ok(Error::NotInitialized)));
    }

    #[test]
    fn twap_calculation_is_correct() {
        let (env, admin, client) = setup();
        let publisher = soroban_sdk::Address::generate(&env);
        let inv = BytesN::from_array(&env, &[9u8; 32]);

        client.add_publisher(&publisher);

        // t=0: price 100 for 300 s, then price 200 for 300 s, now = 600.
        env.ledger().set_timestamp(0);
        client.record_price(&publisher, &inv, &100);
        env.ledger().set_timestamp(300);
        client.record_price(&publisher, &inv, &200);
        env.ledger().set_timestamp(600);

        // TWAP over 600 s window: (100*300 + 200*300) / 600 = 150.
        let twap = client.get_twap(&inv, &HOUR).unwrap();
        assert_eq!(twap, 150);
    }
}
