# Cross-Chain Query Guide for Fairdrop Auction

## Problem Overview

When you deploy the auction application from your owner chain, the auction state only exists on that chain. When you connect from the web interface using a different chain (via MetaMask), that new chain has no auction state, resulting in empty queries.

## Solution: Subscribe Pattern

The solution uses Linera's event streaming to propagate auction state to other chains. Here's how it works:

---

## How to Query from a Different Chain

### Step 1: Check Chain Information

First, determine which chain you're on and which chain has the auction state:

```graphql
query {
  chainInfo {
    currentChainId
    creatorChainId
    hasState
  }
}
```

**Response:**
```json
{
  "chainInfo": {
    "currentChainId": "e476...",
    "creatorChainId": "a1b2...",  // The chain where auction was created
    "hasState": false              // This chain doesn't have state yet
  }
}
```

### Step 2: Subscribe to Auction Updates

If `hasState` is `false`, subscribe to receive auction events from the creator chain:

```graphql
mutation {
  subscribe
}
```

**What happens:**
1. Your chain subscribes to the `auction_updates` stream from the creator chain
2. The creator chain immediately emits an `AuctionInitialized` event with all current auction data
3. Your chain processes this event and populates its cached state
4. Your chain will continue receiving `BidPlaced` and `StatusChanged` events

### Step 3: Query the Cached Auction State

Now you can query the complete auction information:

```graphql
query {
  cachedAuctionState {
    # Auction parameters (static configuration)
    owner
    startTimestamp
    startPrice
    floorPrice
    decrementRate
    decrementInterval
    totalQuantity

    # Dynamic state (updated via events)
    quantitySold
    status
    currentPrice
    lastUpdated
  }
}
```

**Response:**
```json
{
  "cachedAuctionState": {
    "owner": "a1b2c3...",
    "startTimestamp": "2025-01-15T10:00:00Z",
    "startPrice": "1000000000000000000",
    "floorPrice": "100000000000000000",
    "decrementRate": "10000000000000000",
    "decrementInterval": 60,
    "totalQuantity": 1000,
    "quantitySold": 150,
    "status": "ACTIVE",
    "currentPrice": "850000000000000000",
    "lastUpdated": "2025-01-15T10:15:30Z"
  }
}
```

---

## Querying from the Creator Chain

If you're querying from the chain where the auction was created (`hasState: true`), you can use either:

### Option 1: Full Auction Info (Creator Chain Only)
```graphql
query {
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
    currentTime
    timeUntilNextDecrement
  }
}
```

This provides real-time calculated data, including time calculations.

### Option 2: Individual Queries
```graphql
query {
  currentPrice
  quantityRemaining
}
```

---

## Placing Bids from Any Chain

The great news is that **you can place bids from any chain**, even if you don't subscribe:

```graphql
mutation {
  placeBid(quantity: 10)
}
```

**How it works:**
- If you're on the creator chain → bid executes directly
- If you're on a different chain → a cross-chain message is sent to the creator chain
- The creator chain processes the bid and updates its state
- If you've subscribed, you'll receive a `BidPlaced` event with the update

---

## Event Streaming Details

### Events You'll Receive After Subscribing:

1. **AuctionInitialized** (sent immediately on subscription)
   - Contains all auction parameters
   - Current state snapshot (quantity sold, status, price)

2. **BidPlaced** (whenever anyone places a bid)
   - Bidder address
   - Quantity purchased
   - New total quantity sold
   - Current price at time of bid

3. **StatusChanged** (when auction status changes)
   - New status (Scheduled → Active → Ended)

---

## Unsubscribing

To stop receiving updates and clear cached state:

```graphql
mutation {
  unsubscribe
}
```

This will:
- Unsubscribe from the event stream
- Clear the cached state on your chain

---

## Frontend Implementation Recommendations

### Recommended Flow for Web Interface:

```javascript
// 1. On page load, check chain info
const chainInfo = await queryChainInfo();

if (!chainInfo.hasState) {
  // 2. Subscribe to get auction data
  await subscribeToAuction();

  // 3. Wait a moment for the initialization event to process
  await sleep(1000);
}

// 4. Query the appropriate endpoint
const auctionData = chainInfo.hasState
  ? await queryAuctionInfo()          // Creator chain - full info
  : await queryCachedAuctionState();  // Other chains - cached state

// 5. Display auction data
displayAuction(auctionData);
```

### Handling the 404 Error

The 404 error when trying to query the owner chain directly is likely because:
1. Cross-chain GraphQL queries aren't directly supported
2. You need to be "on" the chain to query it (hence the subscribe pattern)

The subscribe pattern **is the correct approach** for cross-chain queries in Linera.

---

## Testing Your Implementation

### Test Scenario 1: Query from Owner Chain
1. Use the terminal/CLI with your owner chain
2. Query `auctionInfo` → should work immediately
3. Place a bid → should work

### Test Scenario 2: Query from New Chain (Web Interface)
1. Connect with MetaMask (new chain)
2. Query `chainInfo` → should show `hasState: false`
3. Call `subscribe` mutation
4. Query `cachedAuctionState` → should now return full data
5. Place a bid → should send cross-chain message and work

### Test Scenario 3: Multiple Subscribers
1. Subscribe from Chain A and Chain B
2. Place a bid from Chain C
3. Both Chain A and B should receive the `BidPlaced` event
4. All chains' `cachedAuctionState` should be in sync

---

## Summary

**The key insight:** In Linera, state is local to each chain. Cross-chain state synchronization happens via:
1. **Event streaming** (for queries) ← Your solution
2. **Cross-chain messages** (for mutations) ← Already working

Your implementation already has all the pieces! You just need to:
1. Update your frontend to call `subscribe` before querying
2. Use `cachedAuctionState` instead of `auctionInfo` on non-creator chains

The changes I made enhance the cached state to include all auction parameters, so subscribers get complete information.
