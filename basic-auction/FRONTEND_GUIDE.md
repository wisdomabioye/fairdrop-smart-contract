# Frontend Guide: Querying Fairdrop Auction Across Chains

This guide shows how to query auction state from different chains in your frontend application.

## Understanding the Architecture

### Hybrid Approach
1. **Creator Chain** - Source of truth, has full auction state
2. **Subscribed Chains** - Can cache auction updates via event streaming
3. **Other Chains** - Must query creator chain directly

## GraphQL Endpoint Structure

The key to querying specific chains is the endpoint URL format:

```
http://localhost:{port}/chains/{chainId}/applications/{applicationId}
```

**You don't need to open/claim a chain to query it!** Just specify the chainId in the URL.

## Usage Patterns

### Pattern 1: Find and Query Creator Chain (No Subscription)

This is the simplest approach - always query the creator chain for live data.

#### JavaScript Example

```javascript
// Configuration
const PORT = 8080;
const APP_ID = "your_application_id_here";

// Helper function to query a specific chain
async function queryChain(chainId, query) {
  const endpoint = `http://localhost:${PORT}/chains/${chainId}/applications/${APP_ID}`;

  const response = await fetch(endpoint, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ query })
  });

  const result = await response.json();
  return result.data;
}

// Step 1: Find where the auction lives (query from any chain)
async function findAuctionChain(currentChainId) {
  const data = await queryChain(currentChainId, `
    query {
      chainInfo {
        creatorChainId
        hasState
      }
    }
  `);

  return data.chainInfo.creatorChainId;
}

// Step 2: Query the creator chain for auction data
async function getAuctionInfo(creatorChainId) {
  const data = await queryChain(creatorChainId, `
    query {
      auctionInfo {
        owner
        currentPrice
        quantityRemaining
        quantitySold
        status
        startTimestamp
        floorPrice
        timeUntilNextDecrement
      }
    }
  `);

  return data.auctionInfo;
}

// Complete flow
async function fetchAuctionData(myChainId) {
  // Find creator chain
  const creatorChainId = await findAuctionChain(myChainId);

  // Query creator chain for live data
  const auctionInfo = await getAuctionInfo(creatorChainId);

  return auctionInfo;
}

// Usage
fetchAuctionData("my_current_chain_id")
  .then(auction => {
    console.log(`Current Price: ${auction.currentPrice}`);
    console.log(`Remaining: ${auction.quantityRemaining}`);
    console.log(`Status: ${auction.status}`);
  });
```

#### React Example

```jsx
import { ApolloClient, InMemoryCache, HttpLink, ApolloProvider, useQuery, gql } from '@apollo/client';
import { useState, useEffect } from 'react';

// Create a client for a specific chain
function createChainClient(chainId, applicationId, port = 8080) {
  return new ApolloClient({
    link: new HttpLink({
      uri: `http://localhost:${port}/chains/${chainId}/applications/${applicationId}`,
    }),
    cache: new InMemoryCache(),
  });
}

// Component to display auction info
function AuctionDisplay({ chainId, appId }) {
  const [creatorChainId, setCreatorChainId] = useState(null);
  const [auctionData, setAuctionData] = useState(null);

  useEffect(() => {
    // Step 1: Find creator chain
    const currentClient = createChainClient(chainId, appId);

    currentClient.query({
      query: gql`
        query {
          chainInfo {
            creatorChainId
            hasState
          }
        }
      `
    }).then(result => {
      const creator = result.data.chainInfo.creatorChainId;
      setCreatorChainId(creator);

      // Step 2: Query creator chain
      const creatorClient = createChainClient(creator, appId);

      return creatorClient.query({
        query: gql`
          query {
            auctionInfo {
              currentPrice
              quantityRemaining
              quantitySold
              status
            }
          }
        `
      });
    }).then(result => {
      setAuctionData(result.data.auctionInfo);
    });
  }, [chainId, appId]);

  if (!auctionData) return <div>Loading...</div>;

  return (
    <div>
      <h2>Auction Info</h2>
      <p>Creator Chain: {creatorChainId}</p>
      <p>Current Price: {auctionData.currentPrice}</p>
      <p>Available: {auctionData.quantityRemaining}</p>
      <p>Status: {auctionData.status}</p>
    </div>
  );
}
```

### Pattern 2: Subscribe and Use Cached Data

For better performance, chains can subscribe to auction updates and query locally cached data.

#### Step 1: Subscribe to Auction Updates

```javascript
// Subscribe to updates (one-time operation)
async function subscribeToAuction(myChainId, appId) {
  const endpoint = `http://localhost:8080/chains/${myChainId}/applications/${appId}`;

  const response = await fetch(endpoint, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      query: `
        mutation {
          subscribe
        }
      `
    })
  });

  return await response.json();
}
```

#### Step 2: Query Cached State (Fast, Local)

```javascript
// Query local cached state (no cross-chain query needed!)
async function getCachedAuctionState(myChainId, appId) {
  const endpoint = `http://localhost:8080/chains/${myChainId}/applications/${appId}`;

  const response = await fetch(endpoint, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      query: `
        query {
          cachedAuctionState {
            quantitySold
            status
            currentPrice
            lastUpdated
          }
        }
      `
    })
  });

  const result = await response.json();
  return result.data.cachedAuctionState;
}
```

#### Complete React Component with Subscription

```jsx
function AuctionWithSubscription({ chainId, appId }) {
  const [isSubscribed, setIsSubscribed] = useState(false);
  const [cachedData, setCachedData] = useState(null);
  const client = createChainClient(chainId, appId);

  // Subscribe once on mount
  useEffect(() => {
    client.mutate({
      mutation: gql`
        mutation {
          subscribe
        }
      `
    }).then(() => {
      setIsSubscribed(true);
    });
  }, []);

  // Poll cached data (or use subscriptions)
  useEffect(() => {
    if (!isSubscribed) return;

    const interval = setInterval(() => {
      client.query({
        query: gql`
          query {
            cachedAuctionState {
              quantitySold
              currentPrice
              status
              lastUpdated
            }
          }
        `,
        fetchPolicy: 'network-only'
      }).then(result => {
        setCachedData(result.data.cachedAuctionState);
      });
    }, 1000);

    return () => clearInterval(interval);
  }, [isSubscribed]);

  if (!isSubscribed) return <div>Subscribing...</div>;
  if (!cachedData) return <div>Waiting for updates...</div>;

  return (
    <div>
      <h2>Auction Info (Cached)</h2>
      <p>Sold: {cachedData.quantitySold}</p>
      <p>Current Price: {cachedData.currentPrice}</p>
      <p>Status: {cachedData.status}</p>
      <p>Last Update: {new Date(cachedData.lastUpdated).toLocaleString()}</p>
    </div>
  );
}
```

### Pattern 3: Hybrid Approach (Best UX)

Combine both patterns for optimal user experience:

```javascript
async function getAuctionData(myChainId, appId) {
  // First, try cached data (fast)
  const cached = await getCachedAuctionState(myChainId, appId);

  if (cached) {
    // We have cached data, use it immediately
    return {
      source: 'cached',
      data: cached,
      stale: false // Could calculate staleness based on lastUpdated
    };
  }

  // No cached data, query creator chain (slower but authoritative)
  const creatorChainId = await findAuctionChain(myChainId);
  const liveData = await getAuctionInfo(creatorChainId);

  return {
    source: 'live',
    data: liveData,
    stale: false
  };
}
```

## Place a Bid from Any Chain

Bidding works from any chain (operation is automatically forwarded to creator chain):

```javascript
async function placeBid(myChainId, appId, quantity) {
  const endpoint = `http://localhost:${PORT}/chains/${myChainId}/applications/${appId}`;

  const response = await fetch(endpoint, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      query: `
        mutation {
          placeBid(quantity: ${quantity})
        }
      `
    })
  });

  return await response.json();
}
```

## GraphQL Queries Reference

### Query Chain Info (Works on any chain)
```graphql
query {
  chainInfo {
    currentChainId
    creatorChainId
    hasState
  }
}
```

### Query Full Auction Info (Creator chain only)
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

### Query Cached State (Subscribed chains only)
```graphql
query {
  cachedAuctionState {
    quantitySold
    status
    currentPrice
    lastUpdated
  }
}
```

### Query Current Price (Creator chain only)
```graphql
query {
  currentPrice
}
```

### Query Quantity Remaining (Creator chain only)
```graphql
query {
  quantityRemaining
}
```

## Mutations

### Subscribe to Updates
```graphql
mutation {
  subscribe
}
```

### Unsubscribe
```graphql
mutation {
  unsubscribe
}
```

### Place a Bid (Works from any chain)
```graphql
mutation {
  placeBid(quantity: 10)
}
```

## Best Practices

1. **Always check `chainInfo` first** - Find the creator chain before querying auction data
2. **Subscribe for high-traffic scenarios** - If you'll be querying frequently, subscribe to reduce latency
3. **Use cached data for UI updates** - Show cached data immediately, refresh from creator chain as needed
4. **Handle None/null responses** - Queries return `None` if data isn't available on that chain
5. **Poll or use GraphQL subscriptions** - For real-time updates of cached data

## Error Handling

```javascript
async function safeQueryAuction(chainId, appId) {
  try {
    const creatorChainId = await findAuctionChain(chainId);
    const auction = await getAuctionInfo(creatorChainId);

    if (!auction) {
      throw new Error('Auction not found');
    }

    return auction;
  } catch (error) {
    console.error('Failed to query auction:', error);

    // Fallback: try cached data
    const cached = await getCachedAuctionState(chainId, appId);
    if (cached) {
      return {
        ...cached,
        warning: 'Using cached data due to query failure'
      };
    }

    throw error;
  }
}
```

## Summary

| Approach | Latency | Requires Subscription | Data Freshness | Best For |
|----------|---------|----------------------|----------------|----------|
| Query creator chain directly | Medium | No | Always fresh | Simple apps, low query frequency |
| Use cached state (subscribed) | Low | Yes | Near real-time | High-traffic apps, dashboards |
| Hybrid (cached + fallback) | Low | Optional | Best of both | Production apps |

Choose the pattern that best fits your application's needs!
