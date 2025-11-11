// Copyright (c) Fairdrop Contributors
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(target_arch = "wasm32", no_main)]

mod state;

use std::sync::Arc;

use async_graphql::{ComplexObject, Context, EmptySubscription, Request, Response, Schema};
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
        let schema = Schema::build(
            self.state.clone(),
            Operation::mutation_root(self.runtime.clone()),
            EmptySubscription,
        )
        .finish();
        schema.execute(request).await
    }
}

/// GraphQL query methods for AuctionState
#[ComplexObject]
impl AuctionState {
    /// Get the current price based on elapsed time
    /// Returns None if the auction is not instantiated on this chain
    async fn current_price(&self, ctx: &Context<'_>) -> Option<Amount> {
        // If parameters are not set, this chain doesn't have the auction state
        // User should query the creator chain instead
        let params = self.parameters.get().as_ref()?;
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
        let params = self.parameters.get().as_ref()?;
        let sold = *self.quantity_sold.get();
        Some(params.total_quantity.saturating_sub(sold))
    }

    /// Check if this chain has the auction state
    /// Returns true if the auction was instantiated on this chain
    async fn has_auction_state(&self) -> bool {
        self.parameters.get().is_some()
    }

    /// Get information about the auction state
    /// Returns None if the auction is not instantiated on this chain
    async fn auction_info(&self, ctx: &Context<'_>) -> Option<AuctionInfo> {
        let params = self.parameters.get().as_ref()?;
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
            quantity_sold: *self.quantity_sold.get(),
            quantity_remaining: params.total_quantity.saturating_sub(*self.quantity_sold.get()),
            current_price,
            status: *self.status.get(),
            current_time,
            time_until_next_decrement,
        })
    }
    
}
