# Fairdrop Implementation Guide
---

## Overview

This is a stage-by-stage approach to implementing the Fairdrop smart contract on Linera blockchain. Each Stage builds incrementally on the previous one to build a production-ready auction system.

---

## Quick Start

### Prerequisites

1. **Install Linera SDK**
   ```bash
   # Follow instructions at https://linera.io
   cargo install linera-service
   ```

---

## Implementation Stages

### STAGE 1: Basic Auction MVP

**Goal**: Core auction logic without external dependencies

**Features**:
- Auction initialization with parameters (start_price, decrement_rate, floor_price)
- Bid placement (tracks quantity, no actual payment yet)
- Dynamic price calculation based on elapsed time
- Query interface for current price and state

**Testing Stage 1**:
```bash
# Build the contract
cd basic-auction
cargo build --release --target wasm32-unknown-unknown

# Start local Linera network
linera net up

```

---

### Stage 2: Payment Token Integration

**Changes**:
1. Copy all files from basic-auction
2. Add `ApplicationId<FungibleTokenAbi>` as Parameters
3. Call fungible token for actual payments
4. Track `amount_paid` in ParticipantInfo

---

### Stage 3: Distribution & Claiming

**Changes**:
1. Add `FinalizeAuction` operation
2. Add `Claim` operation
3. Calculate clearing price
4. Handle refunds (paid at bid price, refund difference)

---

### Stage 4: Microchain Per Auction

**Architecture**:
```
factory/          # Creates new chains for auctions
auction/          # Auction app (from Stage3 + cross-chain messages)
```

**Factory creates new chains**:
```rust
self.runtime.open_chain(
    ChainOwnership::single(params.owner),
    auction_app_id,
    params,
);
```

**Cross-chain bidding**:
- Users send messages from their chain to auction chain
- Auction processes bids via `execute_message()`

---

### Stage 5: Enhancements

**Optional Features**:
- Whitelist/access control
- Pro-rata allocation for oversold auctions
- NFT support
- Analytics dashboard
- Event subscriptions
- Auction templates

---


### 4. Testing
```bash
# Unit tests
cargo test

# Integration tests with local network
linera net up
linera project publish-and-create
# Run operations and verify state
```

### 5. Code Organization
```
src/
├── lib.rs         # Public API (ABI, types)
├── state.rs       # State structures only
├── contract.rs    # State mutation logic
└── service.rs     # Read-only queries
```

---

## Resources

- **Linera Documentation**: https://linera.dev
- **Example Applications**: `/linera-protocol/examples/`
- **Specification**: `FAIRDROP_SMART_CONTRACT_SPEC.md`

---

## Support

For questions: xpldevelopers@gmail.com

---

## Summary

Each Stage is designed to be:
- **Self-contained**: Built and tested independently
- **Incremental**: Builds on previous Stage's knowledge
- **Practical**: Test-run and modify
