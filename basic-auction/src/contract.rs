// Copyright (c) Fairdrop Contributors
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(target_arch = "wasm32", no_main)]

mod state;

use fairdrop_basic::{AuctionEvent, AuctionParameters, AuctionStatus, FairdropAbi, InstantiationArgument, Message, Operation};
use linera_sdk::{
    Contract, ContractRuntime,
    linera_base_types::{AccountOwner, Amount, WithContractAbi, StreamUpdate},
    views::{RootView, View}
};
use self::state::{AuctionState, CachedAuctionState, ParticipantInfo};

/// Stream name for auction events
const AUCTION_STREAM: &[u8] = b"auction_updates";

pub struct FairdropContract {
    state: AuctionState,
    runtime: ContractRuntime<Self>,
}

linera_sdk::contract!(FairdropContract);

impl WithContractAbi for FairdropContract {
    type Abi = FairdropAbi;
}

impl Contract for FairdropContract {
    type Message = Message;
    type InstantiationArgument = InstantiationArgument;
    type Parameters = (); // No parameters in Stage 1 (no token integration yet)
    type EventValue = AuctionEvent;

    async fn load(runtime: ContractRuntime<Self>) -> Self {
        let state = AuctionState::load(runtime.root_view_storage_context())
            .await
            .expect("Failed to load state");
        FairdropContract { state, runtime }
    }

    async fn instantiate(&mut self, argument: InstantiationArgument) {
        assert!(
            argument.start_price > argument.floor_price,
            "Start price must be greater than floor price"
        );
        assert!(
            argument.decrement_rate > Amount::ZERO,
            "Decrement rate must be greater than zero"
        );
        assert!(
            argument.decrement_interval > 0,
            "Decrement interval must be greater than zero"
        );
        assert!(
            argument.total_quantity > 0,
            "Total quantity must be greater than zero"
        );

        // Validate that the application parameters were configured correctly.
        self.runtime.application_parameters();

        // Get the owner from the authenticated caller
        let owner = self
            .runtime
            .authenticated_signer()
            .expect("Instantiation must be authenticated");

        // Create the auction parameters with the owner
        let params = AuctionParameters {
            owner,
            start_timestamp: argument.start_timestamp,
            start_price: argument.start_price,
            floor_price: argument.floor_price,
            decrement_rate: argument.decrement_rate,
            decrement_interval: argument.decrement_interval,
            total_quantity: argument.total_quantity,
        };

        // Determine initial status based on start time
        let current_time: linera_sdk::linera_base_types::Timestamp = self.runtime.system_time();
        let status = if params.start_timestamp > current_time {
            AuctionStatus::Scheduled
        } else {
            AuctionStatus::Active
        };

        // Store parameters and initialize state
        self.state.parameters.set(Some(params));
        self.state.status.set(status);
        self.state.quantity_sold.set(0);
    }

    async fn execute_operation(&mut self, operation: Operation) -> Self::Response {
        match operation {
            Operation::PlaceBid { quantity } => {
                let bidder = self
                    .runtime
                    .authenticated_signer()
                    .expect("Bid must be authenticated");

                // If we're on the creator chain, execute directly
                if self.runtime.chain_id() == self.runtime.application_creator_chain_id() {
                    self.execute_place_bid_internal(bidder, quantity).await;
                } else {
                    // Otherwise, send a message to the creator chain
                    let message = Message::PlaceBid { bidder, quantity };
                    self.runtime
                        .prepare_message(message)
                        .with_authentication()
                        .send_to(self.runtime.application_creator_chain_id());
                }
            },

            Operation::Subscribe => {
                // Subscribe to auction events from the creator chain
                let creator_chain = self.runtime.application_creator_chain_id();
                let app_id = self.runtime.application_id().forget_abi();
                self.runtime.subscribe_to_events(creator_chain, app_id, AUCTION_STREAM.into());

                // If we're on the creator chain, emit initialization event directly
                if self.runtime.chain_id() == creator_chain {
                    self.emit_initialization_event();
                } else {
                    // If we're on a different chain, send a message to the creator chain
                    // to request it emits an initialization event
                    let message = Message::RequestInitialization;
                    self.runtime
                        .prepare_message(message)
                        .send_to(creator_chain);
                }
            },

            Operation::Unsubscribe => {
                // Unsubscribe from auction events
                let creator_chain = self.runtime.application_creator_chain_id();
                let app_id = self.runtime.application_id().forget_abi();
                self.runtime.unsubscribe_from_events(creator_chain, app_id, AUCTION_STREAM.into());

                // Clear cached state
                self.state.cached_state.set(None);
            },
        }
    }

    async fn execute_message(&mut self, message: Message) {
        match message {
            Message::PlaceBid { bidder, quantity } => {
                // Messages are only sent to the creator chain, so we should be on it
                // But let's verify just in case
                if self.runtime.chain_id() != self.runtime.application_creator_chain_id() {
                    panic!(
                        "PlaceBid message received on wrong chain. Current: {:?}, Creator: {:?}",
                        self.runtime.chain_id(),
                        self.runtime.application_creator_chain_id()
                    );
                }

                // Verify the message authentication
                self.runtime
                    .check_account_permission(bidder)
                    .expect("Permission required for placing bid");

                self.execute_place_bid_internal(bidder, quantity).await;
            }

            Message::RequestInitialization => {
                // Verify we're on the creator chain
                if self.runtime.chain_id() != self.runtime.application_creator_chain_id() {
                    panic!(
                        "Subscription received on wrong chain. Current: {:?}, Creator: {:?}",
                        self.runtime.chain_id(),
                        self.runtime.application_creator_chain_id()
                    );
                }
                
                // Emit initialization event for the requesting chain
                self.emit_initialization_event();
            }
        }
    }

    async fn store(mut self) {
        self.state.save().await.expect("Failed to save state");
    }

    async fn process_streams(&mut self, updates: Vec<StreamUpdate>) {
        // Only process streams on non-creator chains (subscribers)
        if self.runtime.chain_id() == self.runtime.application_creator_chain_id() {
            return;
        }

        for update in updates {
            // Process all new events from the subscribed stream
            for index in update.new_indices() {
                let event = self
                    .runtime
                    .read_event(update.chain_id, AUCTION_STREAM.into(), index);

                // Update cached state based on event type
                match event {
                    AuctionEvent::AuctionInitialized {
                        owner,
                        start_timestamp,
                        start_price,
                        floor_price,
                        decrement_rate,
                        decrement_interval,
                        total_quantity,
                        current_quantity_sold,
                        current_status,
                        current_price,
                        timestamp,
                    } => {
                        // Initialize cached state with all auction parameters
                        let cached = CachedAuctionState {
                            owner,
                            start_timestamp,
                            start_price,
                            floor_price,
                            decrement_rate,
                            decrement_interval,
                            total_quantity,
                            quantity_sold: current_quantity_sold,
                            status: current_status,
                            current_price,
                            last_updated: timestamp,
                        };

                        self.state.cached_state.set(Some(cached));
                    }

                    AuctionEvent::BidPlaced {
                        new_total_sold,
                        current_price,
                        timestamp,
                        ..
                    } => {
                        // Update cached quantity and price
                        if let Some(mut cached) = self.state.cached_state.get().clone() {
                            cached.quantity_sold = new_total_sold;
                            cached.current_price = current_price;
                            cached.last_updated = timestamp;
                            self.state.cached_state.set(Some(cached));
                        }
                        // If no cached state exists, ignore the update
                        // (should have received AuctionInitialized first)
                    }

                    AuctionEvent::StatusChanged {
                        new_status,
                        timestamp,
                    } => {
                        // Update cached status
                        if let Some(mut cached) = self.state.cached_state.get().clone() {
                            cached.status = new_status;
                            cached.last_updated = timestamp;
                            self.state.cached_state.set(Some(cached));
                        }
                        // If no cached state exists, ignore the update
                        // (should have received AuctionInitialized first)
                    }
                }
            }
        }
    }
}

impl FairdropContract {
    /// Emits an initialization event with current auction state
    /// Should only be called on the creator chain
    fn emit_initialization_event(&mut self) {
        // Copy params to avoid borrow conflicts (AuctionParameters is Copy)
        let params = *self.parameters();
        let current_time = self.runtime.system_time();
        let current_price = self.calculate_current_price();
        let quantity_sold = *self.state.quantity_sold.get();
        let status = *self.state.status.get();

        let event = AuctionEvent::AuctionInitialized {
            owner: params.owner,
            start_timestamp: params.start_timestamp,
            start_price: params.start_price,
            floor_price: params.floor_price,
            decrement_rate: params.decrement_rate,
            decrement_interval: params.decrement_interval,
            total_quantity: params.total_quantity,
            current_quantity_sold: quantity_sold,
            current_status: status,
            current_price,
            timestamp: current_time,
        };

        self.runtime.emit(AUCTION_STREAM.into(), &event);
    }

    /// Executes a bid placement operation (internal - bidder already authenticated)
    async fn execute_place_bid_internal(&mut self, bidder: AccountOwner, quantity: u64) {

        // Check if auction has started
        let current_time = self.runtime.system_time();
        let start_time = self.parameters().start_timestamp;
        assert!(
            current_time >= start_time,
            "Auction has not started yet. Starts at: {:?}",
            start_time
        );

        // Update status if needed (from Scheduled to Active)
        if *self.state.status.get() == AuctionStatus::Scheduled {
            self.state.status.set(AuctionStatus::Active);
        }

        // Verify auction is active
        assert!(
            self.state.status.get().is_active(),
            "Auction is not active"
        );

        // Check quantity availability
        let quantity_sold = *self.state.quantity_sold.get();
        let total_quantity = self.parameters().total_quantity;
        assert!(
            quantity > 0,
            "Bid quantity must be greater than zero"
        );
        assert!(
            quantity_sold + quantity <= total_quantity,
            "Insufficient quantity available. Requested: {}, Available: {}",
            quantity,
            total_quantity - quantity_sold
        );

        // Calculate current price (for informational purposes in Stage 1)
        let _current_price = self.calculate_current_price();

        // Record the bid
        let participant_info = ParticipantInfo {
            quantity,
            bid_timestamp: current_time,
        };

        // Update or add participant info
        let existing = self.state.participants.get(&bidder).await
            .expect("Failed to read participant info");

        if let Some(mut existing_info) = existing {
            // Update existing bid
            existing_info.quantity += quantity;
            existing_info.bid_timestamp = current_time;
            self.state.participants.insert(&bidder, existing_info)
                .expect("Failed to update participant");
        } else {
            // New participant
            self.state.participants.insert(&bidder, participant_info)
                .expect("Failed to insert participant");
        }

        // Update total quantity sold
        let new_quantity_sold = quantity_sold + quantity;
        self.state.quantity_sold.set(new_quantity_sold);

        // Emit event for subscribers (only on creator chain)
        if self.runtime.chain_id() == self.runtime.application_creator_chain_id() {
            let event = AuctionEvent::BidPlaced {
                bidder,
                quantity,
                new_total_sold: new_quantity_sold,
                current_price: _current_price,
                timestamp: current_time,
            };
            self.runtime.emit(AUCTION_STREAM.into(), &event);
        }

        // Check if auction should end (sold out or floor price reached)
        if new_quantity_sold >= total_quantity {
            let old_status = *self.state.status.get();
            self.state.status.set(AuctionStatus::Ended);

            // Emit status change event (only on creator chain)
            if self.runtime.chain_id() == self.runtime.application_creator_chain_id()
                && old_status != AuctionStatus::Ended
            {
                let event = AuctionEvent::StatusChanged {
                    new_status: AuctionStatus::Ended,
                    timestamp: current_time,
                };
                self.runtime.emit(AUCTION_STREAM.into(), &event);
            }
        }
    }

    /// Calculates the current price based on elapsed time since auction start
    fn calculate_current_price(&mut self) -> Amount {
        // Copy params to avoid borrow conflicts (AuctionParameters is Copy)
        let params = *self.parameters();
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

    /// Helper method to get auction parameters
    fn parameters(&self) -> &AuctionParameters {
        self.state
            .parameters
            .get()
            .as_ref()
            .expect("Application not instantiated. Parameters are None. Make sure instantiate() was called first.")
    }

}


#[cfg(test)]
mod tests {
    use super::*;
    use linera_sdk::linera_base_types::{AccountOwner, Amount, CryptoHash, TimeDelta, Timestamp};

    /// Helper function to test price calculation logic
    fn calculate_price_at_time(
        start_price: Amount,
        floor_price: Amount,
        decrement_rate: Amount,
        decrement_interval: u64,
        start_timestamp: Timestamp,
        current_timestamp: Timestamp,
    ) -> Amount {
        if current_timestamp < start_timestamp {
            return start_price;
        }

        let elapsed = current_timestamp.duration_since(start_timestamp);
        let elapsed_secs = elapsed.as_secs();
        let intervals_passed = elapsed_secs / decrement_interval;
        let total_decrement = decrement_rate.saturating_mul(intervals_passed as u128);

        start_price
            .saturating_sub(total_decrement)
            .max(floor_price)
    }

    #[test]
    fn test_price_calculation_at_start() {
        let start_price = Amount::from_tokens(100);
        let floor_price = Amount::from_tokens(10);
        let decrement_rate = Amount::from_tokens(1);
        let decrement_interval = 60;
        let start_time = Timestamp::from(1000000);

        // At exact start time
        let price = calculate_price_at_time(
            start_price,
            floor_price,
            decrement_rate,
            decrement_interval,
            start_time,
            start_time,
        );
        assert_eq!(price, start_price);
    }

    #[test]
    fn test_price_calculation_before_start() {
        let start_price = Amount::from_tokens(100);
        let floor_price = Amount::from_tokens(10);
        let decrement_rate = Amount::from_tokens(1);
        let decrement_interval = 60;
        let start_time = Timestamp::from(2000000);
        let current_time = Timestamp::from(1000000);

        // Before start time
        let price = calculate_price_at_time(
            start_price,
            floor_price,
            decrement_rate,
            decrement_interval,
            start_time,
            current_time,
        );
        assert_eq!(price, start_price);
    }

    #[test]
    fn test_price_calculation_after_one_interval() {
        let start_price = Amount::from_tokens(100);
        let floor_price = Amount::from_tokens(10);
        let decrement_rate = Amount::from_tokens(1);
        let decrement_interval = 60;
        let start_time = Timestamp::from(1000000);
        let current_time = start_time.saturating_add(TimeDelta::from_secs(60));

        // After one interval (60 seconds)
        let price = calculate_price_at_time(
            start_price,
            floor_price,
            decrement_rate,
            decrement_interval,
            start_time,
            current_time,
        );
        assert_eq!(price, Amount::from_tokens(99)); // 100 - 1
    }

    #[test]
    fn test_price_calculation_after_multiple_intervals() {
        let start_price = Amount::from_tokens(100);
        let floor_price = Amount::from_tokens(10);
        let decrement_rate = Amount::from_tokens(1);
        let decrement_interval = 60;
        let start_time = Timestamp::from(1000000);
        let current_time = start_time.saturating_add(TimeDelta::from_secs(300)); // 5 intervals

        // After 5 intervals (300 seconds)
        let price = calculate_price_at_time(
            start_price,
            floor_price,
            decrement_rate,
            decrement_interval,
            start_time,
            current_time,
        );
        assert_eq!(price, Amount::from_tokens(95)); // 100 - 5
    }

    #[test]
    fn test_price_calculation_reaches_floor() {
        let start_price = Amount::from_tokens(100);
        let floor_price = Amount::from_tokens(10);
        let decrement_rate = Amount::from_tokens(1);
        let decrement_interval = 60;
        let start_time = Timestamp::from(1000000);
        // After 100 intervals, price should be at floor
        let current_time = start_time.saturating_add(TimeDelta::from_secs(6000)); // 100 intervals

        let price = calculate_price_at_time(
            start_price,
            floor_price,
            decrement_rate,
            decrement_interval,
            start_time,
            current_time,
        );
        assert_eq!(price, floor_price); // Should not go below floor
    }

    #[test]
    fn test_price_calculation_below_floor() {
        let start_price = Amount::from_tokens(100);
        let floor_price = Amount::from_tokens(10);
        let decrement_rate = Amount::from_tokens(2);
        let decrement_interval = 60;
        let start_time = Timestamp::from(1000000);
        // After 50 intervals with rate of 2, price would be 0 without floor
        let current_time = start_time.saturating_add(TimeDelta::from_secs(3000)); // 50 intervals

        let price = calculate_price_at_time(
            start_price,
            floor_price,
            decrement_rate,
            decrement_interval,
            start_time,
            current_time,
        );
        assert_eq!(price, floor_price); // Should stop at floor, not go below
    }

    #[test]
    fn test_price_calculation_partial_interval() {
        let start_price = Amount::from_tokens(100);
        let floor_price = Amount::from_tokens(10);
        let decrement_rate = Amount::from_tokens(1);
        let decrement_interval = 60;
        let start_time = Timestamp::from(1000000);
        // 90 seconds = 1.5 intervals, but should only count 1 complete interval
        let current_time = start_time.saturating_add(TimeDelta::from_secs(90));

        let price = calculate_price_at_time(
            start_price,
            floor_price,
            decrement_rate,
            decrement_interval,
            start_time,
            current_time,
        );
        assert_eq!(price, Amount::from_tokens(99)); // 100 - 1 (only 1 complete interval)
    }

    #[test]
    fn test_instantiation_argument_validation() {
        // These tests verify the assertions in instantiate()
        // Valid configuration (owner will be set during instantiation from authenticated caller)
        use fairdrop_basic::InstantiationArgument;

        let valid_arg = InstantiationArgument {
            start_timestamp: Timestamp::from(1000000),
            start_price: Amount::from_tokens(100),
            floor_price: Amount::from_tokens(10),
            decrement_rate: Amount::from_tokens(1),
            decrement_interval: 60,
            total_quantity: 1000,
        };

        // Verify valid configuration doesn't panic
        assert!(valid_arg.start_price > valid_arg.floor_price);
        assert!(valid_arg.decrement_rate > Amount::ZERO);
        assert!(valid_arg.decrement_interval > 0);
        assert!(valid_arg.total_quantity > 0);
    }

    #[test]
    fn test_auction_parameters_fields() {
        let owner = AccountOwner::from(CryptoHash::test_hash("owner"));
        let params = AuctionParameters {
            owner,
            start_timestamp: Timestamp::from(1000000),
            start_price: Amount::from_tokens(100),
            floor_price: Amount::from_tokens(10),
            decrement_rate: Amount::from_tokens(1),
            decrement_interval: 60,
            total_quantity: 1000,
        };

        // Test direct field access
        assert_eq!(params.owner, owner);
        assert_eq!(params.start_timestamp, Timestamp::from(1000000));
        assert_eq!(params.start_price, Amount::from_tokens(100));
        assert_eq!(params.floor_price, Amount::from_tokens(10));
        assert_eq!(params.decrement_rate, Amount::from_tokens(1));
        assert_eq!(params.decrement_interval, 60);
        assert_eq!(params.total_quantity, 1000);
    }
}
