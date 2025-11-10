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

### Queries

- `auctionParameters`: Get auction configuration
- `currentPrice`: Get the current price (calculated dynamically)
- `status`: Get auction status (Scheduled, Active, or Ended)
- `quantitySold`: Get total quantity sold so far
- `quantityRemaining`: Get quantity still available
- `auctionInfo`: Get comprehensive auction information including time until next price decrement
- `state`: Get complete auction state including participants

## Building

```bash
cd smart-contract/basic-auction

# Build for WASM
cargo build --release --target wasm32-unknown-unknown
```

## Testing Locally

### 1. Start a Local Linera Network

```bash
# From the linera-protocol root directory
linera net up
```

### 2. Publish the Application

```bash
# Navigate to step1-basic-auction directory
cd smart-contract/basic-auction

# Publish and create an instance of the auction
linera project publish-and-create \
  --instantiation-arg '{
    "owner": "User:...",
    "start_timestamp": 1234567890000000,
    "start_price": "1000000",
    "floor_price": "100000",
    "decrement_rate": "10000",
    "decrement_interval": 60,
    "total_quantity": 1000
  }'
```

### 3. Place a Bid

```bash
linera project run-operation PlaceBid '{"quantity": 100}'
```

### 4. Query Auction State

```bash
# Get current price
linera project query '{
  currentPrice
}'

# Get comprehensive auction info
linera project query '{
  auctionInfo {
    owner
    startTimestamp
    startPrice
    floorPrice
    currentPrice
    quantitySold
    quantityRemaining
    status
    timeUntilNextDecrement
  }
}'

# Get auction status
linera project query '{
  status
}'
```

## Example Scenario

### Create an Auction

Auction for 1,000 tokens:
- Starts at $1.00 per token
- Decreases by $0.01 every 60 seconds
- Floor price of $0.10

```bash
linera project publish-and-create \
  --instantiation-arg '{
    "owner": "User:7136460f0c87ae46f966f898d494c4b40c4ae8c527f4d1c0b1fa0f7cff91d20f",
    "start_timestamp": 1735689600000000,
    "start_price": "1000000",
    "floor_price": "100000",
    "decrement_rate": "10000",
    "decrement_interval": 60,
    "total_quantity": 1000
  }'
```

### Place Bids

User A buys 300 tokens:
```bash
linera project run-operation PlaceBid '{"quantity": 300}'
```

User B buys 500 tokens:
```bash
linera project run-operation PlaceBid '{"quantity": 500}'
```

### Query Current State

```bash
linera project query '{
  auctionInfo {
    currentPrice
    quantitySold
    quantityRemaining
    status
    timeUntilNextDecrement
  }
}'
```

Expected output:
```json
{
  "auctionInfo": {
    "currentPrice": "980000",
    "quantitySold": 800,
    "quantityRemaining": 200,
    "status": "Active",
    "timeUntilNextDecrement": 45
  }
}
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

1. **Tested thoroughly** with different auction parameters
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
- Documentation: See `FAIRDROP_IMPLEMENTATION_GUIDE.md` in the repository root

---

**Next**: Proceed to Step 2 to add payment token integration!
