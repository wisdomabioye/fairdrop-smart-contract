// Copyright (c) Fairdrop Contributors
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(target_arch = "wasm32", no_main)]

mod state;

use std::sync::Arc;

use async_graphql::{EmptySubscription, Object, Request, Response, Schema, SimpleObject};
use fairdrop_basic::{FairdropAbi, AuctionParameters, Operation};
use linera_sdk::{
    graphql::GraphQLMutationRoot,
    linera_base_types::{Amount, Timestamp, WithServiceAbi},
    views::View,
    Service, ServiceRuntime,
};
use state::{AuctionState, AuctionStatus};

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
            QueryRoot {
                state: self.state.clone(),
                runtime: self.runtime.clone(),
            },
            Operation::mutation_root(self.runtime.clone()),
            EmptySubscription,
        )
        .finish();
        schema.execute(request).await
    }
}

struct QueryRoot {
    state: Arc<AuctionState>,
    runtime: Arc<ServiceRuntime<FairdropService>>,
}

#[Object]
impl QueryRoot {
    /// Get the auction configuration parameters
    async fn auction_parameters(&self) -> Option<AuctionParameters> {
        self.state.parameters.get().clone()
    }

    /// Get the current price based on elapsed time
    async fn current_price(&self) -> Amount {
        let params = self.state.parameters.get().expect("Auction not instantiated");
        let current_time = self.runtime.system_time();

        // If auction hasn't started, return start price
        if current_time < params.start_timestamp {
            return params.start_price;
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
        params
            .start_price
            .saturating_sub(total_decrement)
            .max(params.floor_price)
    }

    /// Get the current auction status
    async fn status(&self) -> AuctionStatus {
        *self.state.status.get()
    }

    /// Get the total quantity sold so far
    async fn quantity_sold(&self) -> u64 {
        *self.state.quantity_sold.get()
    }

    /// Get the remaining quantity available for sale
    async fn quantity_remaining(&self) -> u64 {
        let params = self.state.parameters.get().expect("Auction not instantiated");
        let sold = *self.state.quantity_sold.get();
        params.total_quantity.saturating_sub(sold)
    }

    /// Get information about the auction state
    async fn auction_info(&self) -> AuctionInfo {
        let params = self.state.parameters.get().expect("Auction not instantiated");
        let current_time = self.runtime.system_time();

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

        AuctionInfo {
            owner: params.owner,
            start_timestamp: params.start_timestamp,
            start_price: params.start_price,
            floor_price: params.floor_price,
            decrement_rate: params.decrement_rate,
            decrement_interval: params.decrement_interval,
            total_quantity: params.total_quantity,
            quantity_sold: *self.state.quantity_sold.get(),
            quantity_remaining: params.total_quantity.saturating_sub(*self.state.quantity_sold.get()),
            current_price,
            status: *self.state.status.get(),
            current_time,
            time_until_next_decrement,
        }
    }

    /// Get the complete auction state (for advanced queries)
    async fn state(&self) -> &AuctionState {
        &self.state
    }
}

/// Comprehensive auction information
#[derive(SimpleObject)]
struct AuctionInfo {
    owner: linera_sdk::linera_base_types::AccountOwner,
    start_timestamp: Timestamp,
    start_price: Amount,
    floor_price: Amount,
    decrement_rate: Amount,
    decrement_interval: u64,
    total_quantity: u64,
    quantity_sold: u64,
    quantity_remaining: u64,
    current_price: Amount,
    status: AuctionStatus,
    current_time: Timestamp,
    time_until_next_decrement: Option<u64>,
}
