<div align="center">
  <img src="logo.png" alt="StellarSettle Logo" width="200"/>
  
  # StellarSettle Smart Contracts
  
  **Soroban smart contracts powering decentralized invoice financing on Stellar**
  
  [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
  [![Stellar](https://img.shields.io/badge/Stellar-Soroban-blue)](https://stellar.org)
  [![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=flat&logo=rust&logoColor=white)](https://www.rust-lang.org/)
</div>

## 📋 Overview

This repository contains the core Soroban smart contracts that power StellarSettle's decentralized invoice financing platform. These contracts handle:

- **Invoice Escrow**: Secure escrow creation, funding, and settlement
- **Invoice Tokens**: SEP-41 compliant tokenization of invoices
- **Payment Distribution**: Automated payment distribution to investors

## 🏗️ Architecture
```
contracts/
├── invoice-escrow/       # Main escrow contract
├── invoice-token/        # Invoice tokenization (SEP-41)
└── payment-distributor/  # Settlement & distribution logic
```

## 🚀 Quick Start

### Prerequisites

- Rust 1.74+
- Soroban CLI
- Stellar account (testnet/mainnet)

**Windows:** Rust uses the MSVC toolchain by default. Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the **“Desktop development with C++”** workload so `link.exe` is available. (VS Code alone is not sufficient.)

### Installation
```bash
# Install Soroban CLI
cargo install --locked soroban-cli --features opt

# Install dependencies
cargo build

# Run tests
cargo test

# Build contracts
soroban contract build
```

### Deployment
```bash
# Deploy to testnet
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/invoice_escrow.wasm \
  --source YOUR_SECRET_KEY \
  --network testnet

# Initialize contract
soroban contract invoke \
  --id CONTRACT_ID \
  --source YOUR_SECRET_KEY \
  --network testnet \
  -- initialize \
  --admin YOUR_PUBLIC_KEY \
  --platform_fee_bps 300
```

## 📚 Contract Documentation

### Invoice Escrow Contract
```rust
// Create new invoice escrow
pub fn create_escrow(
    env: Env,
    invoice_id: Symbol,
    seller: Address,
    amount: i128,
    due_date: u64,
    payment_token: Address
)

// Fund escrow (investor buys invoice)
pub fn fund_escrow(
    env: Env,
    invoice_id: Symbol,
    buyer: Address
)

// Record payment and distribute funds
pub fn record_payment(
    env: Env,
    invoice_id: Symbol,
    payer: Address,
    amount: i128
)
```

Full API documentation: [docs/API.md](docs/API.md)

## 🧪 Testing
```bash
# Run all tests
cargo test

# Run specific contract tests
cargo test --package invoice-escrow

# Run with output
cargo test -- --nocapture

# Coverage report
cargo tarpaulin --out Html
```

## 🔒 Security

- Smart contracts audited by [Audit Firm] (pending)
- Continuous security scanning via GitHub Actions
- Bug bounty program: [Link] (coming soon)

## 📊 Contract Addresses

### Testnet
- Invoice Escrow: `CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX`
- Invoice Token: `CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX`
- Payment Distributor: `CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX`

### Mainnet
- Coming soon after audit completion

## 🤝 Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md)

## 📄 License

MIT License - see [LICENSE](LICENSE) file for details

## 🔗 Links

- [Main Website](https://stellarsettle.com)
- [Documentation](https://docs.stellarsettle.com)
- [API Backend](https://github.com/stellarsettle/stellarsettle-api)
- [Web App](https://github.com/stellarsettle/stellarsettle-app)

---

Built with ❤️ on Stellar