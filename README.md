# AgriTrust-Contracts

Smart contracts for managing trust streams with milestone completion proof hashing and integrated dispute resolution system on Stellar (Soroban WASM) and Ethereum/L2s (Solidity).

**📚 Documentation:** For detailed information on the contract architecture, function references, and security model, please see [CONTRACTS.md](./CONTRACTS.md).

## 🚀 Key Features
* **Per-Second Streaming Accrual:** High-precision streaming logic using scaling factors on Soroban.
* **Legal Anchoring & Escrow:** Restricts fund streaming until legal documents are cryptographically signed on-chain, alongside an integrated arbitration escrow.
* **Multi-Chain Smart Contracts:** Soroban-based smart contract implementation alongside a Foundry/Solidity implementation supporting ZK proof verification.

## 🛠️ Tech Stack
* **Language/Framework:** Rust / Soroban WASM, Solidity / Foundry
* **Key Dependencies:** `soroban-sdk`, `foundry-rs`

## 📦 Getting Started

### Prerequisites
Ensure you have the required toolchains installed:
* Rust toolchain (cargo, rustc)
* Stellar CLI / Soroban CLI
* Foundry (forge)

### Installation & Local Setup
```bash
# Clone the repository (if running manually)
git clone https://github.com/AgriTrust-Protocol/AgriTrust-Contracts
cd AgriTrust-Contracts

# Check local prerequisites for API, Soroban, and Foundry development
npm run setup:local

# Optionally install Node dependencies and run the JavaScript test suite
npm run setup:local -- --install --verify

# Build Soroban contracts
stellar contract build

# Run cargo tests
cargo test

# Build Solidity contracts
forge build

# Run foundry tests
forge test
```

The onboarding script performs fast, read-only checks for required Node.js/npm tooling and optional Rust, Stellar CLI, and Foundry tooling. It prints remediation guidance for missing tools and exits non-zero only when required tools are missing. Use `--json` for machine-readable output in CI or support runbooks.

## 🤝 Contributing
Contributions are highly welcome. Please ensure your commits are cryptographically signed using GPG or SSH keys. For major structural changes, please open an issue first to discuss your proposal.
