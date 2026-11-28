use std::fmt::Write;
use std::{
    env as std_env, fs,
    path::Path,
    string::{String, ToString},
};

use invoice_escrow::{EscrowStatus, InvoiceCategory, InvoiceEscrow, InvoiceEscrowClient};
use invoice_token::{InvoiceToken, InvoiceTokenClient};
use payment_distributor::{PaymentDistributor, PaymentDistributorClient};
use soroban_sdk::testutils::EnvTestConfig;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, BytesN, Env, Symbol, Vec};

struct SnapshotEnv {
    env: Env,
    admin: Address,
    seller: Address,
    debtor: Address,
    buyer: Address,
    fee_recipient: Address,
    invoice_id: Symbol,
    escrow_id: Address,
    escrow: InvoiceEscrowClient<'static>,
    distributor_id: Address,
    inv_token_id: Address,
    inv_token: InvoiceTokenClient<'static>,
    payment_token: Address,
    payment_asset: soroban_sdk::token::StellarAssetClient<'static>,
}

impl SnapshotEnv {
    fn new() -> Self {
        let env = Env::new_with_config(EnvTestConfig {
            capture_snapshot_at_drop: false,
        });
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let seller = Address::generate(&env);
        let debtor = Address::generate(&env);
        let buyer = Address::generate(&env);
        let fee_recipient = Address::generate(&env);
        let invoice_id = Symbol::new(&env, "INV_INTEGRATION");

        let escrow_id = env.register(InvoiceEscrow, ());
        let escrow = InvoiceEscrowClient::new(&env, &escrow_id);

        let distributor_id = env.register(PaymentDistributor, ());
        let distributor = PaymentDistributorClient::new(&env, &distributor_id);

        let inv_token_id = env.register(InvoiceToken, ());
        let inv_token = InvoiceTokenClient::new(&env, &inv_token_id);

        let token_admin = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract_v2(token_admin);
        let payment_token = token_id.address();
        let payment_asset = soroban_sdk::token::StellarAssetClient::new(&env, &payment_token);

        inv_token.initialize(
            &admin,
            &soroban_sdk::String::from_str(&env, "Integration Invoice"),
            &soroban_sdk::String::from_str(&env, "INVINT"),
            &7,
            &invoice_id,
            &escrow_id,
        );
        escrow.initialize(&admin, &300);
        distributor.initialize(&admin);
        distributor.set_fee_recipient(&admin, &fee_recipient);
        distributor.set_escrow_contract(&admin, &escrow_id);
        escrow.set_payment_distributor(&distributor_id);

        payment_asset.mint(&buyer, &9_000);
        payment_asset.mint(&debtor, &10_000);

        Self {
            env,
            admin,
            seller,
            debtor,
            buyer,
            fee_recipient,
            invoice_id,
            escrow_id,
            escrow,
            distributor_id,
            inv_token_id,
            inv_token,
            payment_token,
            payment_asset,
        }
    }

    fn create_and_fund(&self) {
        self.escrow.create_escrow_legacy(
            &self.invoice_id,
            &self.seller,
            &self.debtor,
            &10_000,
            &9_000,
            &1_000_000,
            &self.payment_token,
            &self.inv_token_id,
            &BytesN::from_array(&self.env, &[0; 32]),
            &None,
            &None,
        );
        self.escrow
            .fund_escrow_legacy(&self.invoice_id, &self.buyer, &9_000);
    }

    fn token_balance(&self, address: &Address) -> i128 {
        self.payment_asset.balance(address)
    }
}

fn address_role(test_env: &SnapshotEnv, address: &Address) -> &'static str {
    if *address == test_env.admin {
        "admin"
    } else if *address == test_env.buyer {
        "buyer"
    } else if *address == test_env.debtor {
        "debtor"
    } else if *address == test_env.distributor_id {
        "distributor"
    } else if *address == test_env.escrow_id {
        "escrow"
    } else if *address == test_env.fee_recipient {
        "fee_recipient"
    } else if *address == test_env.inv_token_id {
        "invoice_token"
    } else if *address == test_env.payment_token {
        "payment_token"
    } else if *address == test_env.seller {
        "seller"
    } else {
        panic!("snapshot contains an address without a fixture role");
    }
}

fn address_list(test_env: &SnapshotEnv, addresses: Vec<Address>) -> String {
    let mut json = String::from("[");
    for (index, address) in addresses.iter().enumerate() {
        if index > 0 {
            json.push_str(", ");
        }
        write!(json, "\"{}\"", address_role(test_env, &address)).unwrap();
    }
    json.push(']');
    json
}

fn optional_address_role(test_env: &SnapshotEnv, address: Option<Address>) -> String {
    match address {
        Some(address) => format!("\"{}\"", address_role(test_env, &address)),
        None => "null".to_string(),
    }
}

fn json_string(value: &str) -> String {
    let mut escaped = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                write!(escaped, "\\u{:04x}", character as u32).unwrap();
            }
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

fn canonical_state_json(test_env: &SnapshotEnv) -> String {
    let escrow = test_env.escrow.get_escrow(&test_env.invoice_id);
    let config = test_env.escrow.get_config();
    let expected_commitment = BytesN::from_array(&test_env.env, &[0; 32]);
    assert_eq!(escrow.commitment, expected_commitment);

    let status = match escrow.status {
        EscrowStatus::Created => "created",
        EscrowStatus::Funded => "funded",
        EscrowStatus::Settled => "settled",
        EscrowStatus::Refunded => "refunded",
        EscrowStatus::Cancelled => "cancelled",
        EscrowStatus::Disputed => "disputed",
    };
    let category = match escrow.category {
        InvoiceCategory::Standard => "standard",
        InvoiceCategory::Factoring => "factoring",
        InvoiceCategory::Reverse => "reverse",
        InvoiceCategory::Government => "government",
    };
    let ledger = LedgerSnapshot::capture(&test_env.env);
    let mut balances = String::new();
    for (index, (role, address)) in [
        ("admin", &test_env.admin),
        ("buyer", &test_env.buyer),
        ("debtor", &test_env.debtor),
        ("distributor", &test_env.distributor_id),
        ("escrow", &test_env.escrow_id),
        ("fee_recipient", &test_env.fee_recipient),
        ("seller", &test_env.seller),
    ]
    .iter()
    .enumerate()
    {
        if index > 0 {
            balances.push_str(",\n");
        }
        write!(
            balances,
            "    \"{}\": {}",
            role,
            test_env.token_balance(address)
        )
        .unwrap();
    }

    let mut storage_debug = format!("{:#?}\n{:#?}", config, escrow);
    for (role, address) in [
        ("admin", &test_env.admin),
        ("buyer", &test_env.buyer),
        ("debtor", &test_env.debtor),
        ("distributor", &test_env.distributor_id),
        ("escrow", &test_env.escrow_id),
        ("fee_recipient", &test_env.fee_recipient),
        ("invoice_token", &test_env.inv_token_id),
        ("payment_token", &test_env.payment_token),
        ("seller", &test_env.seller),
    ] {
        storage_debug = storage_debug.replace(&format!("{:?}", address), role);
    }
    assert!(
        !storage_debug.contains("Contract(C") && !storage_debug.contains("Account(G"),
        "snapshot contains an address without a fixture role"
    );

    format!(
        r#"{{
  "config": {{
    "accreditation_callback": {},
    "admin": "{}",
    "dispute_timeout_secs": {},
    "fee_bps": {},
    "grace_period_seconds": {},
    "min_investment": {},
    "paused": {},
    "penalty_interest_bps": {},
    "payment_distributor": {},
    "referral_contract": {},
    "whitelist_enabled": {}
  }},
  "contract_storage_debug": {},
  "escrow": {{
    "accepted_tokens": {},
    "category": "{}",
    "commitment": "fixture:zero",
    "debtor": "{}",
    "due_dt": {},
    "early_settlement_cutoff": {},
    "early_settlement_discount_bps": {},
    "face_value": {},
    "funded_amt": {},
    "funded_dt": {},
    "funding_milestone": {},
    "funder": {},
    "funders": {},
    "inv_id": "{}",
    "inv_token": "{}",
    "paid_amt": {},
    "purchase_price": {},
    "seller": "{}",
    "status": "{}",
    "token": "{}"
  }},
  "invoice_token": {{
    "buyer_balance": {},
    "transfer_locked": {}
  }},
  "ledger": {{
    "sequence": {},
    "timestamp": {}
  }},
  "payment_token_balances": {{
{}
  }}
}}"#,
        optional_address_role(test_env, config.accreditation_callback),
        address_role(test_env, &config.admin),
        config.dispute_timeout_secs,
        config.fee_bps,
        config.grace_period_seconds,
        config.min_investment,
        config.paused,
        config.penalty_interest_bps,
        optional_address_role(test_env, config.payment_distributor),
        optional_address_role(test_env, config.referral_contract),
        config.whitelist_enabled,
        json_string(&storage_debug),
        address_list(test_env, escrow.accepted_tokens),
        category,
        address_role(test_env, &escrow.debtor),
        escrow.due_dt,
        escrow.early_settlement_cutoff,
        escrow.early_settlement_discount_bps,
        escrow.face_value,
        escrow.funded_amt,
        escrow.funded_dt,
        match escrow.funding_milestone {
            Some(amount) => amount.to_string(),
            None => "null".to_string(),
        },
        optional_address_role(test_env, escrow.funder),
        address_list(test_env, escrow.funders),
        format!("{:?}", escrow.inv_id)
            .trim_start_matches("Symbol(")
            .trim_end_matches(')'),
        address_role(test_env, &escrow.inv_token),
        escrow.paid_amt,
        escrow.purchase_price,
        address_role(test_env, &escrow.seller),
        status,
        address_role(test_env, &escrow.token),
        test_env.inv_token.balance(&test_env.buyer),
        test_env.inv_token.transfer_locked(),
        ledger.sequence,
        ledger.timestamp,
        balances
    )
}

fn assert_state_snapshot(name: &str, actual: &str, expected: &str) {
    if std_env::var_os("UPDATE_CONTRACT_STATE_SNAPSHOTS").is_some() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test_snapshots")
            .join("contract-state")
            .join(name);
        fs::create_dir_all(path.parent().unwrap()).expect("create snapshot directory");
        fs::write(path, actual).expect("write contract state snapshot");
        return;
    }

    if expected != actual {
        eprintln!("Contract state snapshot changed: {name}");
        eprintln!("--- expected\n{expected}");
        eprintln!("+++ generated\n{actual}");
        panic!("review and update the contract state snapshot");
    }
}

#[test]
fn contract_state_snapshots() {
    let test_env = SnapshotEnv::new();
    test_env.env.ledger().set_timestamp(1_000);
    test_env.env.ledger().set_sequence_number(4_242);
    test_env.create_and_fund();

    let funded = canonical_state_json(&test_env);
    assert_state_snapshot(
        "funded-escrow.json",
        &funded,
        include_str!("../test_snapshots/contract-state/funded-escrow.json"),
    );

    assert_eq!(
        test_env
            .escrow
            .try_record_payment(&test_env.invoice_id, &test_env.debtor, &0),
        Err(Ok(invoice_escrow::Error::InvalidAmount))
    );
    assert_eq!(canonical_state_json(&test_env), funded);

    test_env
        .escrow
        .record_payment(&test_env.invoice_id, &test_env.debtor, &10_000);
    assert_state_snapshot(
        "settled-escrow.json",
        &canonical_state_json(&test_env),
        include_str!("../test_snapshots/contract-state/settled-escrow.json"),
    );
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LedgerSnapshot {
    timestamp: u64,
    sequence: u32,
}

impl LedgerSnapshot {
    fn capture(env: &Env) -> Self {
        Self {
            timestamp: env.ledger().timestamp(),
            sequence: env.ledger().sequence(),
        }
    }
}
