use invoice_escrow::{Error, InvoiceEscrow, InvoiceEscrowClient, MultiSigConfig};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, EnvTestConfig, Events as _, Ledger as _, MockAuth, MockAuthInvoke},
    token::{Client as TokenClient, StellarAssetClient},
    vec, Address, BytesN, Env, IntoVal, Map, Symbol, TryFromVal, Val, Vec,
};

// Legacy escrow also invokes a receipt-token interface. Payment balances use
// a real Stellar Asset Contract; this fixture isolates receipt bookkeeping.
#[contract]
struct ReceiptToken;

#[contractimpl]
impl ReceiptToken {
    pub fn __constructor(env: Env, minter: Address) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "minter"), &minter);
    }

    pub fn decimals() -> u32 {
        7
    }

    pub fn mint(env: Env, _to: Address, _amount: i128, by: Address) {
        by.require_auth();
        assert_eq!(
            env.storage()
                .instance()
                .get::<_, Address>(&Symbol::new(&env, "minter"))
                .unwrap(),
            by
        );
    }

    pub fn set_transfer_locked(env: Env, caller: Address, _locked: bool) {
        caller.require_auth();
        assert_eq!(
            env.storage()
                .instance()
                .get::<_, Address>(&Symbol::new(&env, "minter"))
                .unwrap(),
            caller
        );
    }
}

struct Fixture {
    env: Env,
    client: InvoiceEscrowClient<'static>,
    admin: Address,
    issuer: Address,
    token: Address,
    id: BytesN<32>,
}

impl Fixture {
    fn new() -> Self {
        let env = Env::new_with_config(EnvTestConfig {
            capture_snapshot_at_drop: false,
        });
        env.mock_all_auths();
        let contract = if let Some(directory) = std::env::var_os("SS_CONTRACT_WASM_DIR") {
            let path = std::path::Path::new(&directory).join("invoice_escrow.wasm");
            let wasm = std::fs::read(path).expect("built invoice-escrow WASM");
            // Deployment and subsequent calls have separate transaction budgets.
            let id = env.register(wasm.as_slice(), ());
            env.cost_estimate().budget().reset_default();
            id
        } else {
            env.register(InvoiceEscrow, ())
        };
        let client = InvoiceEscrowClient::new(&env, &contract);
        let admin = Address::generate(&env);
        let issuer = Address::generate(&env);
        let token = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        client.initialize(&admin, &0);
        let id = BytesN::from_array(&env, &[1; 32]);
        Self {
            env,
            client,
            admin,
            issuer,
            token,
            id,
        }
    }

    fn create(&self, id: &BytesN<32>, target: i128) {
        self.client.create_invoice(
            id,
            &self.issuer,
            &target,
            &target,
            &target,
            &1000,
            &0,
            &None,
            &self.token,
        );
    }

    fn invest(&self, holder: &Address, amount: i128) {
        self.mint(holder, amount);
        self.client.invest(&self.id, holder, &amount);
    }

    fn mint(&self, to: &Address, amount: i128) {
        StellarAssetClient::new(&self.env, &self.token).mint(to, &amount);
    }

    fn balance(&self, owner: &Address) -> i128 {
        TokenClient::new(&self.env, &self.token).balance(owner)
    }

    fn authorize(&self, signers: &[&Address], function: &str, args: Vec<Val>) {
        let invocation = MockAuthInvoke {
            contract: &self.client.address,
            fn_name: function,
            args,
            sub_invokes: &[],
        };
        let mocks: std::vec::Vec<_> = signers
            .iter()
            .map(|signer| MockAuth {
                address: signer,
                invoke: &invocation,
            })
            .collect();
        self.env.mock_auths(&mocks);
    }

    fn event(&self, name: &str) -> (Vec<Val>, Map<Symbol, Val>) {
        let events = self.env.events().all();
        let own = events.filter_by_contract(&self.client.address);
        let event = own
            .events()
            .iter()
            .find(|event| {
                let soroban_sdk::xdr::ContractEventBody::V0(body) = &event.body;
                body.topics.first() == Some(&Symbol::new(&self.env, name).into_val(&self.env))
            })
            .expect("expected contract event");
        let soroban_sdk::xdr::ContractEventBody::V0(body) = &event.body;
        let topics = Vec::try_from_val(
            &self.env,
            &soroban_sdk::xdr::ScVal::Vec(Some(body.topics.clone().into())),
        )
        .unwrap();
        let data = Map::try_from_val(&self.env, &body.data).unwrap();
        (topics, data)
    }

    fn settle(&self, repayment: i128) {
        let other_admin = Address::generate(&self.env);
        self.client.set_emergency_config(
            &self.admin,
            &MultiSigConfig {
                admins: vec![&self.env, self.admin.clone(), other_admin.clone()],
                threshold: 2,
            },
        );
        self.mint(&self.issuer, repayment);
        self.client
            .deposit_repayment(&self.id, &self.issuer, &repayment);
        self.client
            .propose_settlement(&self.admin, &self.id, &repayment);
        assert_eq!(
            self.client.try_approve_settlement(&self.admin, &self.id),
            Err(Ok(Error::SameAdminApproval))
        );
        self.client.approve_settlement(&other_admin, &self.id);
    }
}

#[test]
fn invoice_cap_blocks_only_new_holders_and_counts_real_positions() {
    let f = Fixture::new();
    f.create(&f.id, 1000);
    f.client.set_investor_cap(&f.id, &1);
    let holder = Address::generate(&f.env);
    let stranger = Address::generate(&f.env);
    f.invest(&holder, 100);
    f.mint(&stranger, 100);
    assert_eq!(
        f.client.try_invest(&f.id, &stranger, &100),
        Err(Ok(Error::InvestorCapReached))
    );
    assert_eq!(f.balance(&stranger), 100);
    f.invest(&holder, 100);
    f.mint(&holder, 100);
    f.client.top_up(&holder, &f.id, &100);
    assert_eq!(f.client.get_investor_position(&f.id, &holder), 300);
    let count = f.client.get_investor_count(&f.id);
    assert_eq!((count.current, count.max), (1, 1));
    assert_eq!(f.client.get_invoice_investor_count(&f.id), 1);
    assert_eq!(
        f.client.try_set_investor_cap(&f.id, &2),
        Err(Ok(Error::InvalidInvoiceStatus))
    );
    f.client.partial_refund(&holder, &f.id, &300);
    assert_eq!(f.client.get_investor_count(&f.id).current, 0);
    // Refunding all positions cannot reopen cap configuration after going live.
    assert_eq!(
        f.client.try_set_investor_cap(&f.id, &2),
        Err(Ok(Error::InvalidInvoiceStatus))
    );
    f.client.invest(&f.id, &stranger, &100);
    assert_eq!(f.client.get_investor_count(&f.id).current, 1);
}

#[test]
fn cap_configuration_requires_issuer_signature_and_valid_bounds() {
    let f = Fixture::new();
    f.create(&f.id, 1000);
    f.authorize(
        &[&f.admin],
        "set_investor_cap",
        (&f.id, 2u32).into_val(&f.env),
    );
    assert!(f.client.try_set_investor_cap(&f.id, &2).is_err());
    f.authorize(
        &[&f.issuer],
        "set_investor_cap",
        (&f.id, 2u32).into_val(&f.env),
    );
    f.client.set_investor_cap(&f.id, &2);
    assert_eq!(
        f.client.try_set_investor_cap(&f.id, &0),
        Err(Ok(Error::InvalidMaxInvestors))
    );
    assert_eq!(
        f.client.try_set_investor_cap(&f.id, &501),
        Err(Ok(Error::InvalidMaxInvestors))
    );
}

#[test]
fn registered_invoices_enforce_their_own_cap_and_refund_updates_count() {
    let f = Fixture::new();
    f.client.register_invoice(
        &f.id,
        &1200,
        &1000,
        &200,
        &1000,
        &BytesN::from_array(&f.env, &[2; 32]),
    );
    f.client.set_investor_cap(&f.id, &1);
    let holder = Address::generate(&f.env);
    let other = Address::generate(&f.env);
    f.client.invest(&f.id, &holder, &100);
    f.client.invest(&f.id, &holder, &100);
    assert_eq!(
        f.client.try_invest(&f.id, &other, &100),
        Err(Ok(Error::InvestorCapReached))
    );
    assert_eq!(f.client.get_investor_count(&f.id).current, 1);
    f.client.cancel_invoice(&f.id);
    f.client.refund(&f.id, &holder);
    assert_eq!(f.client.get_investor_count(&f.id).current, 0);
}

#[test]
fn custody_is_backed_admin_only_and_emits_owner_and_amount() {
    let f = Fixture::new();
    f.create(&f.id, 1000);
    let custodian = Address::generate(&f.env);
    let beneficiary = Address::generate(&f.env);
    f.invest(&custodian, 200);
    assert_eq!(
        f.client
            .try_register_custody(&custodian, &beneficiary, &f.id, &201),
        Err(Ok(Error::InvalidAmount))
    );
    f.env.set_auths(&[]);
    assert!(f
        .client
        .try_register_custody(&custodian, &beneficiary, &f.id, &200)
        .is_err());
    f.authorize(
        &[&f.admin],
        "register_custody",
        (&custodian, &beneficiary, &f.id, 200i128).into_val(&f.env),
    );
    f.client
        .register_custody(&custodian, &beneficiary, &f.id, &200);
    let (topics, data) = f.event("CustodyRegistered");
    assert_eq!(
        BytesN::<32>::try_from_val(&f.env, &topics.get(1).unwrap()).unwrap(),
        f.id
    );
    assert_eq!(
        Address::try_from_val(&f.env, &topics.get(2).unwrap()).unwrap(),
        custodian
    );
    assert_eq!(
        Address::try_from_val(
            &f.env,
            &data.get(Symbol::new(&f.env, "beneficiary")).unwrap()
        )
        .unwrap(),
        beneficiary
    );
    assert_eq!(
        i128::try_from_val(&f.env, &data.get(Symbol::new(&f.env, "amount")).unwrap()).unwrap(),
        200
    );
    assert_eq!(
        f.client.get_beneficial_owner(&custodian, &f.id),
        beneficiary
    );
    f.env.mock_all_auths();
    assert_eq!(
        f.client
            .try_register_custody(&custodian, &beneficiary, &f.id, &200),
        Err(Ok(Error::CustodyExists))
    );
    assert_eq!(
        f.client.try_partial_refund(&custodian, &f.id, &1),
        Err(Ok(Error::PositionInCustody))
    );
}

#[test]
fn custody_transfer_requires_both_signatures_and_moves_the_backing_position() {
    let f = Fixture::new();
    f.create(&f.id, 1000);
    f.client.set_investor_cap(&f.id, &1);
    let old = Address::generate(&f.env);
    let new = Address::generate(&f.env);
    let beneficiary = Address::generate(&f.env);
    f.invest(&old, 500);
    f.client.register_custody(&old, &beneficiary, &f.id, &500);
    for missing_beneficiary in [true, false] {
        let signer = if missing_beneficiary {
            &old
        } else {
            &beneficiary
        };
        f.authorize(
            &[signer, &new],
            "transfer_custody",
            (&old, &new, &f.id).into_val(&f.env),
        );
        assert!(f.client.try_transfer_custody(&old, &new, &f.id).is_err());
        assert_eq!(f.client.get_investor_position(&f.id, &old), 500);
    }
    f.authorize(
        &[&old, &beneficiary, &new],
        "transfer_custody",
        (&old, &new, &f.id).into_val(&f.env),
    );
    f.client.transfer_custody(&old, &new, &f.id);
    let (_, data) = f.event("CustodyTransferred");
    for (name, expected) in [
        ("old_custodian", &old),
        ("new_custodian", &new),
        ("beneficiary", &beneficiary),
    ] {
        assert_eq!(
            Address::try_from_val(&f.env, &data.get(Symbol::new(&f.env, name)).unwrap()).unwrap(),
            *expected
        );
    }
    assert_eq!(
        i128::try_from_val(&f.env, &data.get(Symbol::new(&f.env, "amount")).unwrap()).unwrap(),
        500
    );
    assert_eq!(f.client.get_investor_position(&f.id, &old), 0);
    assert_eq!(f.client.get_investor_position(&f.id, &new), 500);
    assert_eq!(f.client.get_investor_count(&f.id).current, 1);
    assert_eq!(f.client.get_beneficial_owner(&new, &f.id), beneficiary);
    assert_eq!(
        f.client.try_get_beneficial_owner(&old, &f.id),
        Err(Ok(Error::CustodyNotFound))
    );
}

#[test]
fn partial_custody_cannot_create_a_new_holder_when_cap_is_full() {
    let f = Fixture::new();
    f.create(&f.id, 1000);
    f.client.set_investor_cap(&f.id, &1);
    let old = Address::generate(&f.env);
    let new = Address::generate(&f.env);
    let beneficiary = Address::generate(&f.env);
    f.invest(&old, 500);
    f.client.register_custody(&old, &beneficiary, &f.id, &200);
    assert_eq!(
        f.client.try_transfer_custody(&old, &new, &f.id),
        Err(Ok(Error::InvestorCapReached))
    );
    assert_eq!(f.client.get_investor_position(&f.id, &old), 500);
    assert_eq!(f.client.get_investor_count(&f.id).current, 1);
    assert_eq!(f.client.get_custody(&old, &f.id).amount, 200);
}

#[test]
fn settlement_pays_beneficiary_and_direct_investor_with_no_token_dust() {
    let f = Fixture::new();
    f.create(&f.id, 1000);
    let custodian = Address::generate(&f.env);
    let beneficiary = Address::generate(&f.env);
    let direct = Address::generate(&f.env);
    f.invest(&custodian, 600);
    f.invest(&direct, 400);
    f.client
        .register_custody(&custodian, &beneficiary, &f.id, &600);
    f.client.release_to_issuer(&f.id);
    assert_eq!(f.balance(&f.issuer), 1000);
    assert_eq!(
        f.client
            .try_transfer_position(&custodian, &f.id, &direct, &0),
        Err(Ok(Error::PositionInCustody))
    );
    f.settle(1101);
    assert_eq!(f.balance(&beneficiary), 660);
    assert_eq!(f.balance(&custodian), 0);
    assert_eq!(f.balance(&direct), 441);
    assert_eq!(f.balance(&f.client.address), 0);
    assert_eq!(f.client.get_investor_count(&f.id).current, 0);
    assert_eq!(
        f.client.try_deposit_repayment(&f.id, &f.issuer, &1),
        Err(Ok(Error::InvalidInvoiceStatus))
    );
    assert_eq!(
        f.client.try_propose_settlement(&f.admin, &f.id, &1101),
        Err(Ok(Error::InvalidInvoiceStatus))
    );
}

#[test]
fn partially_custodied_positions_split_only_the_reserved_share() {
    let f = Fixture::new();
    f.create(&f.id, 1000);
    let holder = Address::generate(&f.env);
    let beneficiary = Address::generate(&f.env);
    f.invest(&holder, 1000);
    f.client
        .register_custody(&holder, &beneficiary, &f.id, &400);
    f.client.release_to_issuer(&f.id);
    f.settle(1100);
    assert_eq!(f.balance(&beneficiary), 440);
    assert_eq!(f.balance(&holder), 660);
    assert_eq!(f.balance(&f.client.address), 0);
}

#[test]
fn collateral_breach_blocks_creation_until_a_real_topup_restores_ratio() {
    let f = Fixture::new();
    f.client.configure_collateral(&f.issuer, &f.token, &2000);
    f.create(&f.id, 1000);
    f.mint(&f.issuer, 300);
    f.client.top_up_collateral(&f.id, &100);
    assert_eq!(f.client.check_collateral_ratio(&f.issuer), (1000, 2000));
    let status = f.client.get_collateral_status(&f.issuer);
    assert_eq!(
        (
            status.total_collateral,
            status.total_outstanding,
            status.required_topup
        ),
        (100, 1000, 100)
    );
    f.client.request_topup(&f.issuer);
    let (topics, data) = f.event("CollateralTopUpRequired");
    assert_eq!(
        Address::try_from_val(&f.env, &topics.get(1).unwrap()).unwrap(),
        f.issuer
    );
    assert_eq!(
        i128::try_from_val(&f.env, &data.get(Symbol::new(&f.env, "amount")).unwrap()).unwrap(),
        100
    );
    let next = BytesN::from_array(&f.env, &[2; 32]);
    assert_eq!(
        f.client
            .try_create_invoice(&next, &f.issuer, &1000, &1000, &1000, &1000, &0, &None, &f.token),
        Err(Ok(Error::CollateralTopUpRequired))
    );
    assert_eq!(
        f.client.try_create_invoice_legacy(
            &next, &f.issuer, &1000, &1000, &0, &None, &f.token, &None,
        ),
        Err(Ok(Error::CollateralTopUpRequired))
    );
    f.client.top_up_collateral(&f.id, &50);
    assert!(f.client.get_collateral_status(&f.issuer).issuance_blocked);
    f.client.top_up_collateral(&f.id, &50);
    let (topics, data) = f.event("CollateralToppedUp");
    assert_eq!(
        Address::try_from_val(&f.env, &topics.get(1).unwrap()).unwrap(),
        f.issuer
    );
    assert_eq!(
        i128::try_from_val(&f.env, &data.get(Symbol::new(&f.env, "amount")).unwrap()).unwrap(),
        50
    );
    assert!(!f.client.get_collateral_status(&f.issuer).issuance_blocked);
    assert_eq!(f.balance(&f.client.address), 200);
    assert_eq!(f.balance(&f.issuer), 100);
    assert_eq!(
        f.client.try_request_topup(&f.issuer),
        Err(Ok(Error::CollateralRatioHealthy))
    );
    f.create(&next, 1000);
    assert_eq!(
        f.client.get_collateral_status(&f.issuer).total_outstanding,
        2000
    );
}

#[test]
fn collateral_tracks_yield_updates_rounds_shortfalls_up_and_requires_auth() {
    let f = Fixture::new();
    f.client.configure_collateral(&f.issuer, &f.token, &2000);
    assert_eq!(
        f.client.check_collateral_ratio(&f.issuer),
        (i128::MAX, 2000)
    );
    f.create(&f.id, 1000);
    f.mint(&f.issuer, 200);
    f.client.top_up_collateral(&f.id, &200);
    f.client.set_invoice_outstanding(&f.id, &1001);
    let status = f.client.get_collateral_status(&f.issuer);
    assert_eq!((status.total_outstanding, status.required_topup), (1001, 1));
    f.env.set_auths(&[]);
    assert!(f.client.try_request_topup(&f.issuer).is_err());
    assert!(f.client.try_set_invoice_outstanding(&f.id, &1000).is_err());
    assert!(f.client.try_top_up_collateral(&f.id, &1).is_err());
    let other = Address::generate(&f.env);
    assert!(f
        .client
        .try_configure_collateral(&other, &f.token, &2000)
        .is_err());
    f.env.mock_all_auths();
    f.client.request_topup(&f.issuer);
    f.client.set_invoice_outstanding(&f.id, &500);
    assert!(!f.client.get_collateral_status(&f.issuer).issuance_blocked);
    assert_eq!(
        f.client.get_collateral_status(&f.issuer).total_outstanding,
        500
    );
}

#[test]
fn settlement_preserves_collateral_and_clears_outstanding_debt() {
    let f = Fixture::new();
    f.client.configure_collateral(&f.issuer, &f.token, &2000);
    f.create(&f.id, 1000);
    f.mint(&f.issuer, 200);
    f.client.top_up_collateral(&f.id, &200);
    f.invest(&Address::generate(&f.env), 1000);
    f.client.release_to_issuer(&f.id);
    f.settle(1100);
    assert_eq!(f.balance(&f.client.address), 200);
    let status = f.client.get_collateral_status(&f.issuer);
    assert_eq!(
        (status.total_collateral, status.total_outstanding),
        (200, 0)
    );
    assert_eq!(status.ratio_bps, i128::MAX);
}

#[test]
fn oversubscribed_investment_cannot_withdraw_pooled_collateral() {
    let f = Fixture::new();
    f.client.configure_collateral(&f.issuer, &f.token, &2000);
    f.create(&f.id, 1000);
    f.mint(&f.issuer, 200);
    f.client.top_up_collateral(&f.id, &200);
    let holder = Address::generate(&f.env);
    f.mint(&holder, 1500);
    f.client.invest(&f.id, &holder, &1500);
    assert_eq!(f.client.get_investor_position(&f.id, &holder), 1000);
    assert_eq!(f.balance(&holder), 500);
    assert_eq!(f.balance(&f.client.address), 1200);
}

#[test]
fn settling_one_invoice_preserves_another_invoices_repayment_deposit() {
    let f = Fixture::new();
    let second = BytesN::from_array(&f.env, &[2; 32]);
    f.create(&f.id, 1000);
    f.create(&second, 1000);
    let investor = Address::generate(&f.env);
    f.invest(&investor, 1000);
    f.mint(&investor, 1000);
    f.client.invest(&second, &investor, &1000);
    f.client.release_to_issuer(&f.id);
    f.client.release_to_issuer(&second);
    f.mint(&f.issuer, 1100);
    f.client.deposit_repayment(&second, &f.issuer, &1100);
    f.settle(1100);
    assert_eq!(f.balance(&investor), 1100);
    assert_eq!(f.balance(&f.client.address), 1100);
    assert_eq!(
        f.client.try_propose_settlement(&f.admin, &second, &1099),
        Err(Ok(Error::InsufficientRepayment))
    );
    // A mismatched proposal must not lock the correctly deposited repayment.
    f.client.propose_settlement(&f.admin, &second, &1100);
}

#[test]
fn legacy_payouts_preserve_collateral_and_legacy_creation_respects_the_block() {
    let f = Fixture::new();
    f.client.configure_collateral(&f.issuer, &f.token, &2000);
    f.create(&f.id, 1000);
    let legacy_id = Symbol::new(&f.env, "legacy");
    let invoice_token = f.env.register(ReceiptToken, (&f.client.address,));
    let debtor = Address::generate(&f.env);
    let commitment = BytesN::from_array(&f.env, &[3; 32]);
    f.client.create_escrow_legacy(
        &legacy_id,
        &f.issuer,
        &debtor,
        &1100,
        &1000,
        &3600,
        &f.token,
        &invoice_token,
        &commitment,
        &None,
        &None,
    );
    f.mint(&f.issuer, 500);
    f.client.top_up_collateral(&f.id, &500);
    assert_eq!(
        f.client.get_collateral_status(&f.issuer).total_outstanding,
        2100
    );
    let investor = Address::generate(&f.env);
    f.mint(&investor, 1000);
    f.client.fund_escrow_legacy(&legacy_id, &investor, &1000);
    f.env.ledger().with_mut(|ledger| ledger.timestamp = 3600);
    f.mint(&debtor, 1100);
    // The legacy payout formula would consume 100 units of collateral. The
    // reserve guard must reject atomically, preserving the payer's balance.
    assert_eq!(
        f.client.try_record_payment(&legacy_id, &debtor, &1100),
        Err(Ok(Error::CollateralReserved))
    );
    assert_eq!(f.balance(&debtor), 1100);
    assert_eq!(f.balance(&f.client.address), 1500);
    f.client.set_invoice_outstanding(&f.id, &10_000);
    f.client.request_topup(&f.issuer);
    assert_eq!(
        f.client.try_create_escrow_legacy(
            &Symbol::new(&f.env, "blocked"),
            &f.issuer,
            &debtor,
            &1000,
            &1000,
            &7200,
            &f.token,
            &invoice_token,
            &commitment,
            &None,
            &None
        ),
        Err(Ok(Error::CollateralTopUpRequired))
    );
}
