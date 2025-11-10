# Fairdrop Step 1: Basic Auction MVP

This is the first implementation stage of the Fairdrop auction system on Linera blockchain.

## Overview

Step 1 implements the core auction mechanism without external dependencies:

- ✅ Auction initialization with descending price parameters
- ✅ Bid placement (tracks quantity, no actual payment yet)
- ✅ Automatic price calculation based on elapsed time
- ✅ Query interface for current price and auction state
- ✅ Support for scheduled (upcoming) auctions

## Features

### Auction Parameters

- **Owner**: The account that created the auction
- **Start Timestamp**: When the auction begins (allows scheduling future auctions)
- **Start Price**: Initial price per unit
- **Floor Price**: Minimum price (price won't go below this)
- **Decrement Rate**: Amount to reduce price per interval
- **Decrement Interval**: Time (in seconds) between price reductions
- **Total Quantity**: Total units available for auction

### Operations

- **PlaceBid**: Place a bid for a specified quantity at the current price

## Building

```bash
cd basic-auction

# Build for WASM
cargo build --release --target wasm32-unknown-unknown
```

## Example Scenario

### Create an Auction

Auction for 250,000 tokens:
- Starts at 100 per token
- Decreases by 1 every 600 seconds
- Floor price of 1

```bash
linera project publish-and-create \
  --json-argument '{
  "start_timestamp": 1762598125457000,
  "start_price": "100.",
  "floor_price": "1.",
  "decrement_rate": "1.",
  "decrement_interval": 600,
  "total_quantity": 250000
  }'
```

## Price Calculation

The price decreases automatically based on time elapsed:

```
current_price = max(
    start_price - (decrement_rate × intervals_passed),
    floor_price
)

where:
    intervals_passed = (current_time - start_time) / decrement_interval
```

### Example

- Start Price: $1.00
- Decrement Rate: $0.01
- Decrement Interval: 60 seconds
- Floor Price: $0.10

| Time Elapsed | Intervals Passed | Current Price |
|--------------|------------------|---------------|
| 0 seconds    | 0                | $1.00         |
| 60 seconds   | 1                | $0.99         |
| 120 seconds  | 2                | $0.98         |
| 300 seconds  | 5                | $0.95         |
| 5400 seconds | 90               | $0.10 (floor) |

## Auction States

- **Scheduled**: Auction created but hasn't started yet (current_time < start_timestamp)
- **Active**: Auction is running and accepting bids
- **Ended**: Auction sold out (quantity_sold >= total_quantity)

## Limitations (Stage 1 Only)

⚠️ **This is MVP implementation. The following features are NOT included in Stage 1:**

- ❌ No actual token payments (just tracking quantities)
- ❌ No token/NFT distribution
- ❌ No refunds or clearing price calculation
- ❌ No cross-chain bidding
- ❌ No auction finalization or claims

**These features will be added in subsequent stages:**
- Stage 2: Payment token integration
- Stage 3: Distribution and claiming
- Stage 4: Microchain per auction
- Stage 5: Advanced features

## File Structure

```
basic-auction/
├── Cargo.toml           # Build configuration
├── README.md            # This file
└── src/
    ├── lib.rs          # ABI definitions (Operation, Message, etc.)
    ├── state.rs        # State structures (AuctionState, ParticipantInfo)
    ├── contract.rs     # Contract implementation (state mutations)
    └── service.rs      # GraphQL query service (read-only)
```

## Next Steps

After completing Stage 1:

1. **Test thoroughly** with different auction parameters
2. **Experiment** with price curves (different decrement rates and intervals)
3. **Query** the state to understand how Linera views work
4. **Move to Stage 2** to add actual token payments

## Learning Objectives

By completing Stage 1, we have:

- ✅ Linera application structure (contract + service)
- ✅ State management with `RegisterView` and `MapView`
- ✅ Time-based logic using `runtime.system_time()`
- ✅ GraphQL query interface
- ✅ Operation execution and validation
- ✅ Basic auction mechanics

## Support

For questions or issues:
- Email: xpldevelopers@gmail.com
---