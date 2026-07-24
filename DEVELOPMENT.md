# Developer Guide & Local Workflow

This guide covers local environment setup, build steps, and local CI validation for the StellarSettle smart contract workspace.

---

## 🔍 Running CI Checks Locally

Before submitting a Pull Request targeting `dev`, run these checks locally to ensure your changes pass CI:

```bash
# 1. Format check
cargo fmt --all -- --check

# 2. Lint check with Clippy
cargo clippy --all-targets --all-features -- -D warnings

# 3. Run all unit and integration tests across all workspace contracts
cargo test --all --verbose

# 4. Build release WASM binaries for target wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown
```

---

## 🛠️ Local Development Setup

### 1. Requirements
- Rust stable toolchain (`1.80+`) with target `wasm32-unknown-unknown`
- Soroban CLI (`soroban-cli` 22.0.0+)
- `cargo-tarpaulin` (optional, for code coverage reports)

### 2. Standard Commands

```bash
# Build dev binaries
cargo build

# Run tests for specific contract
cargo test --package invoice-escrow
cargo test --package invoice-token
cargo test --package payment-distributor

# Run deployment script locally or against testnet
bash scripts/deploy.sh
```

---

## 🧪 Smoke Testing & Testnet Script Execution

To run end-to-end smoke tests against Stellar Testnet:

1. **Configure Environment Variables**: Set `STELLAR_NETWORK=testnet` and export your test account secret key `SECRET_KEY=S...`.
2. **Execute Deploy Script**: Run `bash scripts/deploy.sh` to compile WASM, deploy contract instances, and register test accounts.
3. **Execute Smoke Test Recipe**:
   ```bash
   # Initialize escrow contract
   soroban contract invoke --id <ESCROW_ID> --source seller --network testnet -- initialize --admin <ADMIN> --payment_token <TOKEN_ID>
   ```

---

## 🌿 Contribution Workflow Reminder

All Pull Requests MUST target the `dev` branch. Direct pushes to `main` or `dev` are prohibited. See [CONTRIBUTING.md](./CONTRIBUTING.md) for details.
