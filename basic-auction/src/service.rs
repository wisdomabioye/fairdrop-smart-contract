// Copyright (c) Fairdrop Contributors
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(target_arch = "wasm32", no_main)]

mod state;

use std::sync::Arc;

use async_graphql::{Context, Object, Request, Response, Schema, Subscription};
use fairdrop_basic::{FairdropAbi, Operation};
use linera_sdk::{
    graphql::GraphQLMutationRoot as _,
    linera_base_types::{Amount, ChainId, WithServiceAbi},
    views::View,
    Service, ServiceRuntime,
};
use state::{AuctionInfo, AuctionState};

pub struct FairdropService {
    state: Arc<AuctionState>,
    runtime: Arc<ServiceRuntime<Self>>,
}

/// Information about where the auction is located
#[derive(async_graphql::SimpleObject)]
pub struct ChainInfo {
    /// The current chain ID
    pub current_chain_id: String,
    /// The chain ID where the auction was created
    pub creator_chain_id: String,
    /// Whether this chain has the auction state
    pub has_state: bool,
}

struct QueryRoot {
    auction_state: Arc<AuctionState>,
}

struct SubscriptionRoot {
    auction_state: Arc<AuctionState>,
    #[allow(dead_code)]
    runtime: Arc<ServiceRuntime<FairdropService>>,
}

linera_sdk::service!(FairdropService);

impl WithServiceAbi for FairdropService {
    type Abi = FairdropAbi;
}

impl Service for FairdropService {
    type Parameters = (); // No parameters in Stage 1

    async fn new(runtime: ServiceRuntime<Self>) -> Self {
        let state = AuctionState::load(runtime.root_view_storage_context())
            .await
            .expect("Failed to load state");
        FairdropService {
            state: Arc::new(state),
            runtime: Arc::new(runtime),
        }
    }

    async fn handle_query(&self, request: Request) -> Response {
        let runtime = self.runtime.clone();
        let schema = Schema::build(
            QueryRoot {
                auction_state: self.state.clone(),
            },
            Operation::mutation_root(self.runtime.clone()),
            SubscriptionRoot {
                auction_state: self.state.clone(),
                runtime: self.runtime.clone(),
            },
        )
        .data(runtime)
        .finish();
        schema.execute(request).await
    }
}

/// GraphQL query methods
#[Object]
impl QueryRoot {
    /// Get the current price based on elapsed time
    /// Returns None if the auction is not instantiated on this chain
    async fn current_price(&self, ctx: &Context<'_>) -> Option<Amount> {
        // If parameters are not set, this chain doesn't have the auction state
        // User should query the creator chain instead
        let params = self.auction_state.parameters.get().as_ref()?;
        let runtime = ctx.data::<Arc<ServiceRuntime<FairdropService>>>().unwrap();
        let current_time = runtime.system_time();

        // If auction hasn't started, return start price
        if current_time < params.start_timestamp {
            return Some(params.start_price);
        }

        // Calculate time elapsed since start
        let elapsed = current_time.duration_since(params.start_timestamp);
        let elapsed_secs = elapsed.as_secs();

        // Calculate number of intervals that have passed
        let intervals_passed = elapsed_secs / params.decrement_interval;

        // Calculate total decrement
        let total_decrement = params
            .decrement_rate
            .saturating_mul(intervals_passed as u128);

        // Calculate current price, ensuring it doesn't go below floor price
        Some(params
            .start_price
            .saturating_sub(total_decrement)
            .max(params.floor_price))
    }

    /// Get the remaining quantity available for sale
    async fn quantity_remaining(&self) -> Option<u64> {
        let params = self.auction_state.parameters.get().as_ref()?;
        let sold = *self.auction_state.quantity_sold.get();
        Some(params.total_quantity.saturating_sub(sold))
    }

    /// Get the remaining quantity available for sale
    async fn quantity_sold(&self) -> u64 {
        *self.auction_state.quantity_sold.get()
    }

    /// Check if this chain has the auction state
    /// Returns true if the auction was instantiated on this chain
    async fn has_auction_state(&self) -> bool {
        self.auction_state.parameters.get().is_some()
    }

    /// Get information about which chain has the auction state
    /// Useful for directing queries to the correct chain
    async fn chain_info(&self, ctx: &Context<'_>) -> ChainInfo {
        let runtime = ctx.data::<Arc<ServiceRuntime<FairdropService>>>().unwrap();
        ChainInfo {
            current_chain_id: runtime.chain_id().to_string(),
            creator_chain_id: runtime.application_creator_chain_id().to_string(),
            has_state: self.auction_state.parameters.get().is_some(),
        }
    }

    /// Get cached auction state from subscribed updates
    /// This is available on non-creator chains that have subscribed to auction events
    /// Returns None if this chain hasn't subscribed or received any updates yet
    async fn cached_auction_state(&self) -> Option<state::CachedAuctionState> {
        self.auction_state.cached_state.get().clone()
    }

    /// Get stream events received from subscriptions
    /// Returns a list of stored stream events with metadata
    /// Optionally filter by chain_id
    async fn stream_events(&self, chain_id: Option<ChainId>) -> Vec<state::StoredStreamEvent> {
        let mut events = Vec::new();

        // Iterate through all stored stream events
        if let Ok(all_event_keys) = self.auction_state.stream_events.indices().await {
            for event_key in all_event_keys {
                if let Ok(Some(event_json)) = self.auction_state.stream_events.get(&event_key).await {
                    // Deserialize the stored event wrapper
                    if let Ok(stored_event) = serde_json::from_str::<state::StoredStreamEvent>(&event_json) {
                        // Apply chain_id filter if provided
                        if let Some(filter_chain_id) = chain_id {
                            if stored_event.chain_id == filter_chain_id {
                                events.push(stored_event);
                            }
                        } else {
                            events.push(stored_event);
                        }
                    }
                }
            }
        }

        // Sort by timestamp descending (newest first)
        events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        events
    }

    /// Get stream events as JSON strings (for easier frontend parsing)
    /// Optionally filter by chain_id
    async fn stream_events_json(&self, chain_id: Option<ChainId>) -> Vec<String> {
        let mut events = Vec::new();

        // Iterate through all stored stream events
        if let Ok(all_event_keys) = self.auction_state.stream_events.indices().await {
            for event_key in all_event_keys {
                if let Ok(Some(event_json)) = self.auction_state.stream_events.get(&event_key).await {
                    // Deserialize to check chain_id if filtering
                    if let Some(filter_chain_id) = chain_id {
                        if let Ok(stored_event) = serde_json::from_str::<state::StoredStreamEvent>(&event_json) {
                            if stored_event.chain_id == filter_chain_id {
                                events.push((stored_event.timestamp, event_json));
                            }
                        }
                    } else {
                        // No filter, include all events
                        if let Ok(stored_event) = serde_json::from_str::<state::StoredStreamEvent>(&event_json) {
                            events.push((stored_event.timestamp, event_json));
                        }
                    }
                }
            }
        }

        // Sort by timestamp descending (newest first)
        events.sort_by(|a, b| b.0.cmp(&a.0));

        // Return just the JSON strings
        events.into_iter().map(|(_, json)| json).collect()
    }

    /// Get information about the auction state
    /// Returns None if the auction is not instantiated on this chain
    ///
    /// NOTE: This query only works on the creator chain.
    /// For non-creator chains, use `cached_auction_state()` if you've subscribed,
    /// or query the creator chain directly (find it via `chain_info()`).
    async fn auction_info(&self, ctx: &Context<'_>) -> Option<AuctionInfo> {
        let params = self.auction_state.parameters.get().as_ref()?;
        let runtime = ctx.data::<Arc<ServiceRuntime<FairdropService>>>().unwrap();
        let current_time = runtime.system_time();

        // Calculate current price
        let current_price = if current_time < params.start_timestamp {
            params.start_price
        } else {
            let elapsed = current_time.duration_since(params.start_timestamp);
            let elapsed_secs = elapsed.as_secs();
            let intervals_passed = elapsed_secs / params.decrement_interval;
            let total_decrement = params
                .decrement_rate
                .saturating_mul(intervals_passed as u128);
            params
                .start_price
                .saturating_sub(total_decrement)
                .max(params.floor_price)
        };

        // Calculate time until next price decrement
        let time_until_next_decrement = if current_time >= params.start_timestamp {
            let elapsed = current_time.duration_since(params.start_timestamp);
            let elapsed_secs = elapsed.as_secs();
            let secs_in_current_interval = elapsed_secs % params.decrement_interval;
            Some(params.decrement_interval - secs_in_current_interval)
        } else {
            None
        };

        Some(AuctionInfo {
            owner: params.owner,
            start_timestamp: params.start_timestamp,
            start_price: params.start_price,
            floor_price: params.floor_price,
            decrement_rate: params.decrement_rate,
            decrement_interval: params.decrement_interval,
            total_quantity: params.total_quantity,
            quantity_sold: *self.auction_state.quantity_sold.get(),
            quantity_remaining: params.total_quantity.saturating_sub(*self.auction_state.quantity_sold.get()),
            current_price,
            status: *self.auction_state.status.get(),
            current_time,
            time_until_next_decrement,
        })
    }

    // Future: Get all created application IDs
    // async fn created_applications(&self) -> Vec<ApplicationId> { ... }

    // Future: Get the count of created applications
    // async fn created_applications_count(&self) -> u64 { ... }

    /// Get all bids placed by this chain (bidder-side state)
    /// Returns list of bids with their status, prices, and refund info
    async fn my_bids(&self) -> Vec<state::UserBid> {
        let mut bids = Vec::new();
        let bid_counter = *self.auction_state.bid_counter.get();

        for bid_id in 0..bid_counter {
            if let Ok(Some(bid)) = self.auction_state.my_bids.get(&bid_id).await {
                bids.push(bid);
            }
        }

        bids
    }

    /// Get total refund owed to this chain (bidder-side state)
    /// This is calculated when BidAccepted messages are received with clearing_price
    async fn my_refund(&self) -> Amount {
        *self.auction_state.refund_owed.get()
    }

    /// Get count of bids placed by this chain
    async fn my_bids_count(&self) -> u64 {
        *self.auction_state.bid_counter.get()
    }
}

/// GraphQL subscription methods
#[Subscription]
impl SubscriptionRoot {
    /// Subscribe to real-time auction events
    /// Returns a live stream of auction events filtered by chain ID
    async fn auction_notifications(
        &self,
        #[graphql(desc = "Chain ID to monitor for events")] chain_id: ChainId,
    ) -> impl async_graphql::futures_util::Stream<Item = async_graphql::Result<String>> {
        use async_graphql::futures_util::stream;

        let auction_state = Arc::clone(&self.auction_state);
        let chain_id_clone = chain_id;

        // Create channel for sending events to the stream
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        // Spawn background task to continuously poll for new events
        tokio::spawn(async move {
            let mut last_counter = 0u64;
            let mut has_sent_initial = false;

            loop {
                // Get all event keys from storage
                if let Ok(all_keys) = auction_state.stream_events.indices().await {
                    let mut new_events = Vec::new();

                    // Collect new events we haven't seen yet
                    for key in all_keys {
                        // Only process events with counter > last_counter
                        if key > last_counter || !has_sent_initial {
                            if let Ok(Some(event_json)) = auction_state.stream_events.get(&key).await {
                                if let Ok(stored_event) = serde_json::from_str::<state::StoredStreamEvent>(&event_json) {
                                    // Filter by chain_id
                                    if stored_event.chain_id == chain_id_clone {
                                        new_events.push((key, event_json));
                                    }
                                }
                            }
                        }
                    }

                    // Sort events by key (chronological order)
                    new_events.sort_by_key(|(key, _)| *key);

                    // Send new events to the stream
                    let has_new_events = !new_events.is_empty();
                    for (key, event_json) in new_events {
                        if tx.send(Ok(event_json)).await.is_err() {
                            // Client disconnected, stop the task
                            return;
                        }
                        last_counter = last_counter.max(key);
                    }

                    if has_new_events {
                        has_sent_initial = true;
                    }

                    // Send heartbeat if no new events (keeps connection alive)
                    if !has_new_events && has_sent_initial {
                        let heartbeat = serde_json::json!({
                            "type": "heartbeat",
                            "timestamp": std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_micros() as u64,
                            "message": "Subscription active, waiting for new auction events"
                        });

                        if let Ok(heartbeat_json) = serde_json::to_string(&heartbeat) {
                            if tx.send(Ok(heartbeat_json)).await.is_err() {
                                return; // Client disconnected
                            }
                        }
                    }
                }

                // Poll every 2 seconds for new events
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        });

        // Convert channel receiver to async stream
        stream::unfold(rx, |mut rx| async move {
            match rx.recv().await {
                Some(event) => Some((event, rx)),
                None => None,
            }
        })
    }
}
