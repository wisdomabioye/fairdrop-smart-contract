// Copyright (c) Fairdrop Contributors
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(target_arch = "wasm32", no_main)]

mod state;

use std::sync::Arc;

use async_graphql::{Context, Object, Request, Response, Schema, Subscription};
use fairdrop_basic::{FairdropAbi, Operation};
use linera_sdk::{
    graphql::GraphQLMutationRoot as _,
    linera_base_types::{AccountOwner, Amount, WithServiceAbi},
    views::View,
    Service, ServiceRuntime,
};
use state::{AuctionInfo, AuctionState, BidHistoryResponse};

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

    /// Get all bids for a specific bidder (O(1) lookup)
    /// Returns all bids (accepted + rejected) for the given bidder
    async fn bids_for_bidder(&self, bidder: AccountOwner) -> Vec<state::BidInfo> {
        self.auction_state
            .bids_by_owner
            .get(&bidder)
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Get aggregated summary statistics for a specific bidder
    /// Calculates totals, refunds, and counts by iterating through their bids
    async fn bidder_summary(&self, bidder: AccountOwner) -> state::BidderSummary {
        let bids = self.auction_state
            .bids_by_owner
            .get(&bidder)
            .await
            .ok()
            .flatten()
            .unwrap_or_default();

        let mut total_quantity = 0u64;
        let mut total_cost = Amount::ZERO;
        let mut total_refund = Amount::ZERO;
        let mut accepted_bids = 0u64;
        let mut rejected_bids = 0u64;

        for bid in bids {
            match bid.status {
                state::BidStatus::Accepted => {
                    accepted_bids += 1;
                    total_quantity += bid.quantity;

                    let cost = bid.bid_price.saturating_mul(bid.quantity as u128);
                    total_cost = total_cost.saturating_add(cost);

                    // Calculate refund if clearing price is set
                    if let Some(clearing) = bid.clearing_price {
                        let refund_per_unit = bid.bid_price.saturating_sub(clearing);
                        let refund = refund_per_unit.saturating_mul(bid.quantity as u128);
                        total_refund = total_refund.saturating_add(refund);
                    }
                }
                state::BidStatus::Rejected => {
                    rejected_bids += 1;
                }
            }
        }

        state::BidderSummary {
            total_quantity,
            total_cost,
            total_refund,
            net_cost: total_cost.saturating_sub(total_refund),
            accepted_bids,
            rejected_bids,
        }
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
            end_timestamp: params.end_timestamp,
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

    /// Get all bid history with filtering and pagination (newest first only)
    /// For querying a specific bidder's bids, use `bids_for_bidder` instead
    ///
    /// # Parameters
    /// - `status`: Optional filter by bid status (Accepted/Rejected)
    /// - `min_price`: Optional minimum bid price filter
    /// - `max_price`: Optional maximum bid price filter
    /// - `offset`: Number of bids to skip (for pagination)
    /// - `limit`: Maximum number of bids to return (default 20, max 100)
    ///
    /// # Note
    /// Only supports DESC order (newest first) for efficient pagination.
    /// Iterates backwards through the log with early termination once limit is reached.
    async fn get_bids(
        &self,
        status: Option<state::BidStatus>,
        min_price: Option<Amount>,
        max_price: Option<Amount>,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> BidHistoryResponse {
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(20).min(100); // Default 20, max 100 per page

        let log_len = self.auction_state.all_bids.count();

        let mut collected = 0;
        let mut skipped = 0;
        let mut filtered_bids = Vec::new();
        let mut total_filtered = 0;

        // Iterate backwards (newest first) with early termination
        for i in (0..log_len).rev() {
            if let Ok(Some(bid)) = self.auction_state.all_bids.get(i).await {
                // Apply filters
                let matches_filter = {
                    let status_match = status.map_or(true, |s| bid.status == s);
                    let min_price_match = min_price.map_or(true, |min| bid.bid_price >= min);
                    let max_price_match = max_price.map_or(true, |max| bid.bid_price <= max);
                    status_match && min_price_match && max_price_match
                };

                if !matches_filter {
                    continue;
                }

                total_filtered += 1;

                // Skip offset items
                if skipped < offset {
                    skipped += 1;
                    continue;
                }

                // Collect up to limit
                if collected < limit {
                    filtered_bids.push(bid);
                    collected += 1;
                }
                // Note: Can't determine has_more yet, continue counting
            }
        }

        let has_more = total_filtered > offset + collected;

        BidHistoryResponse {
            bids: filtered_bids,
            total_count: total_filtered,
            has_more,
        }
    }

}

/// GraphQL subscription methods
#[Subscription]
impl SubscriptionRoot {
    /// Subscribe to real-time auction state updates
    /// Returns current cached state periodically
    async fn auction_state_updates(
        &self,
    ) -> impl async_graphql::futures_util::Stream<Item = async_graphql::Result<Option<state::CachedAuctionState>>> {
        use async_graphql::futures_util::stream;

        let auction_state = Arc::clone(&self.auction_state);

        // Create channel for sending state updates
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        // Spawn background task to poll for state changes
        tokio::spawn(async move {
            let mut last_update: Option<linera_sdk::linera_base_types::Timestamp> = None;

            loop {
                // Get current cached state
                if let Some(cached) = auction_state.cached_state.get().clone() {
                    // Only send if state has changed
                    if last_update.is_none() || last_update != Some(cached.last_updated) {
                        last_update = Some(cached.last_updated);

                        if tx.send(Ok(Some(cached))).await.is_err() {
                            return; // Client disconnected
                        }
                    }
                }

                // Poll every 2 seconds
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        });

        // Convert channel receiver to async stream
        stream::unfold(rx, |mut rx| async move {
            match rx.recv().await {
                Some(state) => Some((state, rx)),
                None => None,
            }
        })
    }
}
