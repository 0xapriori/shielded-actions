# Shielded Actions

Privacy-preserving token swaps on Ethereum using the Anoma Protocol Adapter.

## Overview

This project demonstrates how to build privacy-preserving DeFi applications using the Anoma Resource Machine on Ethereum. Users can:

1. **Shield** ERC20 tokens (convert to private Anoma resources)
2. **Swap** tokens privately via Uniswap V3 without revealing trade details
3. **Unshield** tokens (convert back to standard ERC20)

## Quick Start

See the full documentation in [shielded-actions/README.md](./shielded-actions/README.md).

### Run the Backend

```bash
cd shielded-actions/backend
mix deps.get
mix run --no-halt
```

### Run the Frontend

```bash
cd shielded-actions/frontend
npm install --legacy-peer-deps
npm run dev
```

## Deployed Contracts (Sepolia)

| Contract | Address |
|----------|---------|
| Protocol Adapter | `0x08c3bdc46B115cDc71Df076d9De96EeEBaa98525` |
| WETH Forwarder | `0xD5307D777dC60b763b74945BF5A42ba93ce44e4b` |
| USDC Forwarder | `0x5256b82cB889f8845570b3a2f1C2af7d2F1567fE` |
| Uniswap V3 Forwarder | `0x9335Fa4A31E552378Ed29b94704c52b5635cd1AA` |

## License

MIT
