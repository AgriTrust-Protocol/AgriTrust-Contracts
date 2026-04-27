# Grant Stream Contracts

Smart contracts for managing grant streams with milestone completion proof hashing and integrated dispute resolution system.

## Overview

This repository contains multiple implementations for grant management:
1. **Soroban (Stellar) Implementation**: High-precision per-second streaming with legal anchoring and cross-chain interoperability.
2. **Solidity (Ethereum/L2) Implementation**: Milestone-based releasing with integrated dispute resolution and ZK proofs.

---

## Soroban Implementation (Stellar)

Located in `contracts/grant_stream/`.

### Key Features
- **Per-Second Accrual**: High-precision streaming logic using scaling factors.
- **Legal Anchoring**: Prevent fund streaming until legal documents are cryptographically signed on-chain.
- **Cross-Chain Interoperability**: Compact byte-array status emission for bridge monitoring.
- **Wrapped Asset Security**: Emergency halts and security buffers for non-native assets.
- **Yield Treasury**: Integration with external yield aggregators for capital efficiency.

### Development (Soroban)
```bash
# Build
stellar contract build

# Test
cargo test
```

---

## Solidity Implementation

Located in `foundry/`.

### Key Features
- **Arbitration Escrow**: Secure fund holding during disputes with third-party arbitrators.
- **ZK Proof Verification**: Anonymous verification of grant conditions.
- **Milestone Hashing**: SHA-256 proof submission for deliverables.

### Development (Solidity)
```bash
# Build
forge build

# Test
forge test
```

## Contributing
Please follow the contribution guidelines and ensure all tests pass before submitting pull requests.

## License
MIT License

## 💰 Bounty Contribution

- **Task:** Built-in Tax Withholding Hook for Institutional Grants
- **Reward:** $100
- **Source:** GitHub-Paid
- **Date:** 2026-04-28

