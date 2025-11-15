// Copyright (c) Fairdrop Contributors
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(target_arch = "wasm32", no_main)]

mod state;

use std::sync::Arc;

use async_graphql::{Context, EmptySubscription, Object, Request, Response, Schema};
use fairdrop_basic::{FairdropAbi, Operation};
use linera_sdk::{
    graphql::GraphQLMutationRoot as _,
    linera_base_types::{Amount, WithServiceAbi},
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
            EmptySubscription,
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
}
