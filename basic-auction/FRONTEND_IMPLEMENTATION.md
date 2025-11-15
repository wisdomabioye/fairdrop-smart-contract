# Frontend Implementation Guide

Complete implementation examples for integrating the Fairdrop auction into your web interface.

---

## GraphQL Client Setup

### Using Apollo Client (React)

```typescript
import { ApolloClient, InMemoryCache, gql } from '@apollo/client';

// Initialize Apollo Client pointing to your Linera node
const client = new ApolloClient({
  uri: 'http://localhost:8080/graphql', // Your Linera node GraphQL endpoint
  cache: new InMemoryCache()
});
```

### Using fetch (Vanilla JS)

```javascript
async function queryGraphQL(query, variables = {}) {
  const response = await fetch('http://localhost:8080/graphql', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ query, variables })
  });

  const { data, errors } = await response.json();
  if (errors) throw new Error(errors[0].message);
  return data;
}
```

---

## Core Queries and Mutations

### 1. Check Chain Information

```typescript
const CHAIN_INFO_QUERY = gql`
  query GetChainInfo {
    chainInfo {
      currentChainId
      creatorChainId
      hasState
    }
  }
`;

async function getChainInfo() {
  const { data } = await client.query({
    query: CHAIN_INFO_QUERY
  });
  return data.chainInfo;
}
```

### 2. Subscribe to Auction Updates

```typescript
const SUBSCRIBE_MUTATION = gql`
  mutation Subscribe {
    subscribe
  }
`;

async function subscribeToAuction() {
  await client.mutate({
    mutation: SUBSCRIBE_MUTATION
  });
}
```

### 3. Query Cached Auction State (Non-Creator Chains)

```typescript
const CACHED_AUCTION_QUERY = gql`
  query GetCachedAuction {
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
`;

async function getCachedAuctionState() {
  const { data } = await client.query({
    query: CACHED_AUCTION_QUERY,
    fetchPolicy: 'network-only' // Always get fresh data
  });
  return data.cachedAuctionState;
}
```

### 4. Query Full Auction Info (Creator Chain Only)

```typescript
const AUCTION_INFO_QUERY = gql`
  query GetAuctionInfo {
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
`;

async function getAuctionInfo() {
  const { data } = await client.query({
    query: AUCTION_INFO_QUERY,
    fetchPolicy: 'network-only'
  });
  return data.auctionInfo;
}
```

### 5. Place a Bid

```typescript
const PLACE_BID_MUTATION = gql`
  mutation PlaceBid($quantity: Int!) {
    placeBid(quantity: $quantity)
  }
`;

async function placeBid(quantity: number) {
  await client.mutate({
    mutation: PLACE_BID_MUTATION,
    variables: { quantity }
  });
}
```

---

## Complete React Hook Implementation

```typescript
import { useState, useEffect, useCallback } from 'react';
import { useApolloClient } from '@apollo/client';

interface AuctionData {
  owner: string;
  startTimestamp: string;
  startPrice: string;
  floorPrice: string;
  decrementRate: string;
  decrementInterval: number;
  totalQuantity: number;
  quantitySold: number;
  quantityRemaining?: number;
  currentPrice: string;
  status: 'SCHEDULED' | 'ACTIVE' | 'ENDED';
  lastUpdated?: string;
  currentTime?: string;
  timeUntilNextDecrement?: number;
}

interface ChainInfo {
  currentChainId: string;
  creatorChainId: string;
  hasState: boolean;
}

export function useAuction() {
  const client = useApolloClient();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  const [auctionData, setAuctionData] = useState<AuctionData | null>(null);
  const [chainInfo, setChainInfo] = useState<ChainInfo | null>(null);
  const [subscribed, setSubscribed] = useState(false);

  // Initialize: Check chain and load auction data
  const initialize = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);

      // 1. Check which chain we're on
      const info = await getChainInfo();
      setChainInfo(info);

      // 2. If not on creator chain and not subscribed, subscribe
      if (!info.hasState && !subscribed) {
        await subscribeToAuction();
        setSubscribed(true);

        // Wait for initialization event to process
        await new Promise(resolve => setTimeout(resolve, 1000));
      }

      // 3. Load auction data
      const data = info.hasState
        ? await getAuctionInfo()
        : await getCachedAuctionState();

      setAuctionData(data);
    } catch (err) {
      setError(err as Error);
      console.error('Error initializing auction:', err);
    } finally {
      setLoading(false);
    }
  }, [subscribed]);

  // Refresh auction data
  const refresh = useCallback(async () => {
    if (!chainInfo) return;

    try {
      const data = chainInfo.hasState
        ? await getAuctionInfo()
        : await getCachedAuctionState();

      setAuctionData(data);
    } catch (err) {
      setError(err as Error);
    }
  }, [chainInfo]);

  // Place bid
  const bid = useCallback(async (quantity: number) => {
    try {
      await placeBid(quantity);

      // Refresh data after bid
      await new Promise(resolve => setTimeout(resolve, 1000));
      await refresh();

      return true;
    } catch (err) {
      setError(err as Error);
      return false;
    }
  }, [refresh]);

  // Initialize on mount
  useEffect(() => {
    initialize();
  }, [initialize]);

  // Auto-refresh every 10 seconds
  useEffect(() => {
    const interval = setInterval(refresh, 10000);
    return () => clearInterval(interval);
  }, [refresh]);

  return {
    loading,
    error,
    auctionData,
    chainInfo,
    isCreatorChain: chainInfo?.hasState ?? false,
    bid,
    refresh
  };
}
```

---

## React Component Example

```tsx
import React, { useState } from 'react';
import { useAuction } from './hooks/useAuction';

export function AuctionView() {
  const { loading, error, auctionData, chainInfo, isCreatorChain, bid, refresh } = useAuction();
  const [bidQuantity, setBidQuantity] = useState(1);
  const [bidding, setBidding] = useState(false);

  const handleBid = async () => {
    setBidding(true);
    const success = await bid(bidQuantity);
    if (success) {
      alert(`Successfully bid for ${bidQuantity} units!`);
      setBidQuantity(1);
    } else {
      alert('Failed to place bid. See console for details.');
    }
    setBidding(false);
  };

  if (loading) {
    return <div>Loading auction data...</div>;
  }

  if (error) {
    return (
      <div className="error">
        <h3>Error loading auction</h3>
        <p>{error.message}</p>
        <button onClick={refresh}>Retry</button>
      </div>
    );
  }

  if (!auctionData) {
    return <div>No auction data available</div>;
  }

  const quantityRemaining = auctionData.quantityRemaining ??
    (auctionData.totalQuantity - auctionData.quantitySold);

  return (
    <div className="auction-view">
      <div className="chain-info">
        <small>
          {isCreatorChain ? '🟢 Creator Chain' : '🔵 Subscribed Chain'}
          {' | Chain ID: '}
          {chainInfo?.currentChainId.slice(0, 8)}...
        </small>
      </div>

      <h1>Fairdrop Auction</h1>

      <div className="auction-status">
        <span className={`status-badge ${auctionData.status.toLowerCase()}`}>
          {auctionData.status}
        </span>
      </div>

      <div className="auction-details">
        <div className="detail-row">
          <label>Current Price</label>
          <strong>{formatAmount(auctionData.currentPrice)} tokens</strong>
        </div>

        <div className="detail-row">
          <label>Available</label>
          <strong>{quantityRemaining} / {auctionData.totalQuantity} units</strong>
        </div>

        <div className="detail-row">
          <label>Floor Price</label>
          <span>{formatAmount(auctionData.floorPrice)} tokens</span>
        </div>

        <div className="detail-row">
          <label>Price Decrement</label>
          <span>{formatAmount(auctionData.decrementRate)} every {auctionData.decrementInterval}s</span>
        </div>

        {auctionData.timeUntilNextDecrement !== undefined && (
          <div className="detail-row">
            <label>Next Price Drop</label>
            <span>{auctionData.timeUntilNextDecrement}s</span>
          </div>
        )}
      </div>

      {auctionData.status === 'ACTIVE' && (
        <div className="bid-section">
          <h3>Place Bid</h3>

          <div className="bid-form">
            <input
              type="number"
              min="1"
              max={quantityRemaining}
              value={bidQuantity}
              onChange={(e) => setBidQuantity(Number(e.target.value))}
              disabled={bidding}
            />

            <button
              onClick={handleBid}
              disabled={bidding || bidQuantity < 1 || bidQuantity > quantityRemaining}
            >
              {bidding ? 'Placing Bid...' : `Bid for ${bidQuantity} units`}
            </button>
          </div>

          <p className="bid-total">
            Total: {formatAmount(BigInt(auctionData.currentPrice) * BigInt(bidQuantity))} tokens
          </p>
        </div>
      )}

      <button onClick={refresh} className="refresh-button">
        ↻ Refresh
      </button>
    </div>
  );
}

// Helper function to format Amount (assuming 18 decimals like ETH)
function formatAmount(amount: string | bigint): string {
  const amt = typeof amount === 'string' ? BigInt(amount) : amount;
  const divisor = BigInt(10 ** 18);
  const whole = amt / divisor;
  const fraction = amt % divisor;

  // Format to 4 decimal places
  const fractionStr = fraction.toString().padStart(18, '0').slice(0, 4);

  return `${whole}.${fractionStr}`;
}
```

---

## Vanilla JavaScript Example (No Framework)

```html
<!DOCTYPE html>
<html>
<head>
  <title>Fairdrop Auction</title>
</head>
<body>
  <div id="app">
    <div id="loading">Loading...</div>
    <div id="auction-view" style="display: none;">
      <h1>Fairdrop Auction</h1>
      <div id="status"></div>
      <div id="details"></div>
      <div id="bid-form"></div>
    </div>
  </div>

  <script>
    const GRAPHQL_URL = 'http://localhost:8080/graphql';

    async function query(gql, variables = {}) {
      const res = await fetch(GRAPHQL_URL, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ query: gql, variables })
      });
      const { data, errors } = await res.json();
      if (errors) throw new Error(errors[0].message);
      return data;
    }

    async function initialize() {
      try {
        // Check chain
        const { chainInfo } = await query(`
          query { chainInfo { currentChainId creatorChainId hasState } }
        `);

        // Subscribe if needed
        if (!chainInfo.hasState) {
          await query(`mutation { subscribe }`);
          await new Promise(r => setTimeout(r, 1000));
        }

        // Load auction data
        const queryStr = chainInfo.hasState
          ? `query { auctionInfo { currentPrice quantitySold totalQuantity status } }`
          : `query { cachedAuctionState { currentPrice quantitySold totalQuantity status } }`;

        const auctionData = await query(queryStr);
        const auction = chainInfo.hasState ? auctionData.auctionInfo : auctionData.cachedAuctionState;

        // Display
        document.getElementById('loading').style.display = 'none';
        document.getElementById('auction-view').style.display = 'block';

        document.getElementById('status').innerHTML = `
          <strong>Status:</strong> ${auction.status}
        `;

        document.getElementById('details').innerHTML = `
          <p><strong>Current Price:</strong> ${auction.currentPrice}</p>
          <p><strong>Available:</strong> ${auction.totalQuantity - auction.quantitySold} / ${auction.totalQuantity}</p>
        `;

        if (auction.status === 'ACTIVE') {
          document.getElementById('bid-form').innerHTML = `
            <input type="number" id="quantity" value="1" min="1" />
            <button onclick="placeBidHandler()">Place Bid</button>
          `;
        }
      } catch (err) {
        document.getElementById('loading').innerHTML = 'Error: ' + err.message;
      }
    }

    async function placeBidHandler() {
      const quantity = document.getElementById('quantity').value;
      try {
        await query(`mutation { placeBid(quantity: ${quantity}) }`);
        alert('Bid placed successfully!');
        initialize(); // Refresh
      } catch (err) {
        alert('Failed to place bid: ' + err.message);
      }
    }

    // Initialize on load
    initialize();
  </script>
</body>
</html>
```

---

## Error Handling Best Practices

```typescript
enum AuctionErrorCode {
  NOT_INITIALIZED = 'NOT_INITIALIZED',
  SUBSCRIPTION_FAILED = 'SUBSCRIPTION_FAILED',
  INSUFFICIENT_QUANTITY = 'INSUFFICIENT_QUANTITY',
  AUCTION_NOT_ACTIVE = 'AUCTION_NOT_ACTIVE',
  NETWORK_ERROR = 'NETWORK_ERROR'
}

class AuctionError extends Error {
  constructor(public code: AuctionErrorCode, message: string) {
    super(message);
    this.name = 'AuctionError';
  }
}

function handleAuctionError(error: Error) {
  if (error instanceof AuctionError) {
    switch (error.code) {
      case AuctionErrorCode.NOT_INITIALIZED:
        return 'Auction data not available. Try refreshing.';
      case AuctionErrorCode.SUBSCRIPTION_FAILED:
        return 'Failed to subscribe to auction updates. Check your connection.';
      case AuctionErrorCode.INSUFFICIENT_QUANTITY:
        return 'Not enough units available for your bid.';
      case AuctionErrorCode.AUCTION_NOT_ACTIVE:
        return 'This auction is not currently active.';
      default:
        return 'An error occurred. Please try again.';
    }
  }
  return error.message;
}
```

---

## Testing Your Integration

### Manual Test Checklist

1. ✅ **Load on Creator Chain**
   - Should load immediately without subscription
   - Should show real-time price calculations

2. ✅ **Load on Different Chain**
   - Should auto-subscribe
   - Should load cached data after ~1 second
   - Should receive updates when bids are placed

3. ✅ **Place Bid from Creator Chain**
   - Should execute immediately
   - Should update UI

4. ✅ **Place Bid from Different Chain**
   - Should send cross-chain message
   - Should update cached state after event received

5. ✅ **Multiple Chains**
   - Subscribe from Chain A and B
   - Place bid from Chain C
   - Verify A and B both receive updates

---

## Performance Optimization Tips

1. **Debounce Refresh Calls**
   ```typescript
   const debouncedRefresh = debounce(refresh, 300);
   ```

2. **Cache Chain Info**
   ```typescript
   const chainInfo = useMemo(() => getChainInfo(), []);
   ```

3. **Batch State Updates**
   ```typescript
   setState(prev => ({ ...prev, loading: false, data: newData }));
   ```

4. **Use WebSocket for Real-Time Updates** (if supported by your node)
   ```typescript
   const ws = new WebSocket('ws://localhost:8080/subscriptions');
   ```

---

## Common Issues and Solutions

| Issue | Solution |
|-------|----------|
| `cachedAuctionState` returns `null` | Call `subscribe` mutation first and wait ~1s |
| 404 when querying creator chain | Don't query directly; use subscribe pattern |
| Bid fails with "not authenticated" | Ensure wallet is connected and signed |
| Data not updating after bid | Add delay before refreshing, or poll more frequently |
| "Application not instantiated" error | You're on wrong chain; check `chainInfo.creatorChainId` |

---

This guide provides everything you need to integrate the Fairdrop auction into your frontend application!
