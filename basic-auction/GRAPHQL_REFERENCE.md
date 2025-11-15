# GraphQL API Quick Reference

Quick reference for all Fairdrop auction GraphQL queries and mutations.

---

## Queries

### `chainInfo`
Get information about which chain has the auction state.

```graphql
query {
  chainInfo {
    currentChainId    # The chain you're currently on
    creatorChainId    # The chain where auction was created
    hasState          # Whether current chain has auction state
  }
}
```

**Use case:** Determine if you need to subscribe before querying auction data.

---

### `auctionInfo` (Creator Chain Only)
Get complete auction information with real-time calculations.

```graphql
query {
  auctionInfo {
    owner                      # AccountOwner
    startTimestamp             # Timestamp
    startPrice                 # Amount (string)
    floorPrice                 # Amount (string)
    decrementRate              # Amount (string)
    decrementInterval          # u64
    totalQuantity              # u64
    quantitySold               # u64
    quantityRemaining          # u64
    currentPrice               # Amount (calculated in real-time)
    status                     # SCHEDULED | ACTIVE | ENDED
    currentTime                # Timestamp
    timeUntilNextDecrement     # u64 (seconds) or null
  }
}
```

**Returns:** `null` if not on creator chain.

---

### `cachedAuctionState` (Non-Creator Chains)
Get cached auction information from subscribed updates.

```graphql
query {
  cachedAuctionState {
    # Static Parameters
    owner                      # AccountOwner
    startTimestamp             # Timestamp
    startPrice                 # Amount (string)
    floorPrice                 # Amount (string)
    decrementRate              # Amount (string)
    decrementInterval          # u64
    totalQuantity              # u64

    # Dynamic State
    quantitySold               # u64
    status                     # SCHEDULED | ACTIVE | ENDED
    currentPrice               # Amount (from last event)
    lastUpdated                # Timestamp
  }
}
```

**Returns:** `null` if not subscribed or no events received yet.

---

### `currentPrice`
Get the current price only (creator chain).

```graphql
query {
  currentPrice    # Amount (string) or null
}
```

---

### `quantityRemaining`
Get the remaining quantity available (creator chain).

```graphql
query {
  quantityRemaining    # u64 or null
}
```

---

### `hasAuctionState`
Quick check if current chain has auction state.

```graphql
query {
  hasAuctionState    # boolean
}
```

---

## Mutations

### `placeBid`
Place a bid for a specified quantity.

```graphql
mutation {
  placeBid(quantity: 10)
}
```

**Parameters:**
- `quantity: u64` - Number of units to bid for

**Behavior:**
- On creator chain: Executes immediately
- On other chains: Sends cross-chain message to creator chain

**Validation:**
- Auction must be `ACTIVE`
- Quantity must be > 0
- Quantity must be ≤ remaining quantity
- Caller must be authenticated

---

### `subscribe`
Subscribe to auction event updates from creator chain.

```graphql
mutation {
  subscribe
}
```

**What it does:**
1. Subscribes to `auction_updates` stream from creator chain
2. Immediately receives `AuctionInitialized` event with current state
3. Continues receiving `BidPlaced` and `StatusChanged` events

**After subscribing:** Wait ~1 second for initialization event to process, then query `cachedAuctionState`.

---

### `unsubscribe`
Unsubscribe from auction updates and clear cached state.

```graphql
mutation {
  unsubscribe
}
```

**What it does:**
1. Unsubscribes from the event stream
2. Clears `cachedAuctionState` on current chain

---

## Types

### `Amount`
Represented as a string containing a large integer (e.g., "1000000000000000000" for 1 token with 18 decimals).

```typescript
type Amount = string;

// Example: 1.5 tokens (with 18 decimals)
const amount: Amount = "1500000000000000000";
```

### `Timestamp`
Unix timestamp in microseconds.

```typescript
type Timestamp = number;

// Example: Convert to Date
const date = new Date(timestamp / 1000);
```

### `AccountOwner`
Hex-encoded account owner address.

```typescript
type AccountOwner = string;

// Example: "e476187f6ddfeb9d588c7b45d3df334d5501d6499b3f9ad647c7397a65"
```

### `AuctionStatus`
Enum representing auction state.

```typescript
enum AuctionStatus {
  SCHEDULED = "SCHEDULED",  // Auction scheduled for future
  ACTIVE = "ACTIVE",        // Auction is active
  ENDED = "ENDED"           // Auction has ended
}
```

---

## Event Types (Internal)

These are the events streamed via the `auction_updates` channel:

### `AuctionInitialized`
Sent when a chain subscribes. Contains all auction parameters and current state.

```rust
AuctionInitialized {
    owner: AccountOwner,
    start_timestamp: Timestamp,
    start_price: Amount,
    floor_price: Amount,
    decrement_rate: Amount,
    decrement_interval: u64,
    total_quantity: u64,
    current_quantity_sold: u64,
    current_status: AuctionStatus,
    current_price: Amount,
    timestamp: Timestamp,
}
```

### `BidPlaced`
Emitted whenever a bid is placed.

```rust
BidPlaced {
    bidder: AccountOwner,
    quantity: u64,
    new_total_sold: u64,
    current_price: Amount,
    timestamp: Timestamp,
}
```

### `StatusChanged`
Emitted when auction status changes.

```rust
StatusChanged {
    new_status: AuctionStatus,
    timestamp: Timestamp,
}
```

---

## Common Query Patterns

### Pattern 1: Load Auction (Any Chain)

```graphql
# Step 1: Check which chain we're on
query {
  chainInfo {
    hasState
  }
}

# Step 2: Subscribe if needed
mutation {
  subscribe
}

# Step 3: Query appropriate endpoint
query {
  cachedAuctionState {
    currentPrice
    quantitySold
    totalQuantity
    status
  }
}
```

---

### Pattern 2: Monitor Price Changes

```graphql
# Poll every 10 seconds
query PriceMonitor {
  chainInfo {
    hasState
  }
  cachedAuctionState {
    currentPrice
    lastUpdated
  }
}
```

---

### Pattern 3: Place Bid with Validation

```graphql
# Step 1: Check available quantity
query {
  cachedAuctionState {
    totalQuantity
    quantitySold
    status
  }
}

# Step 2: Place bid
mutation {
  placeBid(quantity: 5)
}

# Step 3: Verify (after 1 second delay)
query {
  cachedAuctionState {
    quantitySold
  }
}
```

---

## Error Handling

Common GraphQL errors and their meanings:

| Error Message | Cause | Solution |
|--------------|-------|----------|
| `"Application not instantiated"` | Querying `auctionInfo` on non-creator chain | Use `cachedAuctionState` instead |
| `"Auction has not started yet"` | Trying to bid before start time | Wait for auction to become `ACTIVE` |
| `"Auction is not active"` | Trying to bid when status is not `ACTIVE` | Check `status` field |
| `"Insufficient quantity available"` | Bid quantity exceeds remaining | Reduce bid quantity |
| `"Bid must be authenticated"` | Not signed in with wallet | Connect wallet |

---

## Best Practices

1. **Always check `chainInfo` first** before deciding which query to use
2. **Subscribe early** on non-creator chains to start receiving updates
3. **Use `fetchPolicy: 'network-only'`** to avoid stale cached data
4. **Add delays** after mutations before querying (allow time for events to propagate)
5. **Handle `null` responses** gracefully (especially for `cachedAuctionState`)
6. **Poll periodically** or use WebSocket subscriptions for real-time updates
7. **Display `lastUpdated`** timestamp to show data freshness

---

## Full Example Query

```graphql
query CompleteAuctionView {
  chainInfo {
    currentChainId
    creatorChainId
    hasState
  }

  # Try creator chain query (will be null if not on creator chain)
  auctionInfo {
    owner
    startTimestamp
    startPrice
    floorPrice
    decrementRate
    decrementInterval
    totalQuantity
    quantitySold
    quantityRemaining
    currentPrice
    status
    timeUntilNextDecrement
  }

  # Try cached state query (will be null if not subscribed)
  cachedAuctionState {
    owner
    startTimestamp
    startPrice
    floorPrice
    decrementRate
    decrementInterval
    totalQuantity
    quantitySold
    status
    currentPrice
    lastUpdated
  }
}
```

Then in your code:

```typescript
const data = response.auctionInfo ?? response.cachedAuctionState;
```

---

That's everything you need to interact with the Fairdrop auction via GraphQL!
