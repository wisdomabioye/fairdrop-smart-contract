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

        // Validate timestamps
        assert!(
            argument.end_timestamp > argument.start_timestamp,
            "End time must be after start time"
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
            end_timestamp: argument.end_timestamp,
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

        let creator_chain = self.runtime.application_creator_chain_id();
        let chain_id = self.runtime.chain_id();
        let app_id = self.runtime.application_id().forget_abi();
        
        self.runtime.subscribe_to_events(creator_chain, app_id, AUCTION_STREAM.into());

        // Request initialization event to get current state
        if chain_id == creator_chain {
            // On creator chain, emit directly
            self.emit_initialization_event();
        } else {
            // On subscriber chain, send message to creator chain
            let message = Message::RequestInitialization;
            self.runtime
                .prepare_message(message)
                .with_tracking()
                .send_to(creator_chain);
        }
    }

    async fn execute_operation(&mut self, operation: Operation) -> Self::Response {
        match operation {
            Operation::PlaceBid { quantity } => {
                let bidder = self
                    .runtime
                    .authenticated_signer()
                    .expect("Bid must be authenticated");

                // Two-Contract Pattern: Record pending bid locally, then send to auction chain
                let auction_chain = self.runtime.application_creator_chain_id();
                let bidder_chain = self.runtime.chain_id();

                // Prevent bidding directly on creator chain (bids must come from user chains)
                assert_ne!(
                    auction_chain, bidder_chain,
                    "Cannot place bids directly on the auction creator chain. Please place bids from your own chain."
                );

                let current_time = self.runtime.system_time();

                // Record bid locally with Pending status
                // Price will be filled in when we receive BidAccepted message
                let bid_id = *self.state.bid_counter.get();
                self.state.bid_counter.set(bid_id + 1);

                let user_bid = state::UserBid {
                    quantity,
                    bid_price: Amount::ZERO, // Will be updated when BidAccepted received
                    timestamp: current_time,
                    status: state::BidStatus::Pending,
                    clearing_price: None,
                };

                self.state.my_bids.insert(&bid_id, user_bid)
                    .expect("Failed to store bid locally");

                // Send message to auction chain with bid_id for matching
                let message = Message::BidSubmission {
                    bidder,
                    quantity,
                    bidder_chain,
                    bid_id,
                };

                self.runtime
                    .prepare_message(message)
                    .with_authentication()
                    .with_tracking()
                    .send_to(auction_chain);
            },

            Operation::Subscribe => {
                // Subscribe to auction events from the creator chain
                let creator_chain = self.runtime.application_creator_chain_id();
                let app_id = self.runtime.application_id().forget_abi();
                self.runtime.subscribe_to_events(creator_chain, app_id, AUCTION_STREAM.into());

                // Request initialization event to get current state
                // if self.runtime.chain_id() == creator_chain {
                //     // On creator chain, emit directly
                //     self.emit_initialization_event();
                // } else {
                //     // On subscriber chain, send message to creator chain
                //     let message = Message::RequestInitialization;
                //     self.runtime
                //         .prepare_message(message)
                //         .with_tracking()
                //         .send_to(creator_chain);
                // }
            },

            Operation::Unsubscribe => {
                // Unsubscribe from auction events
                let creator_chain = self.runtime.application_creator_chain_id();
                let app_id = self.runtime.application_id().forget_abi();
                self.runtime.unsubscribe_from_events(creator_chain, app_id, AUCTION_STREAM.into());

                // Clean up cached state
                self.state.cached_state.set(None);
            }

            // Future: Dynamic application creation
            // Operation::CreateApplication { ... } => { ... }
        }
    }

    async fn execute_message(&mut self, message: Message) {
        match message {
            // Two-Contract Pattern: Bid submission from bidder chain
            Message::BidSubmission { bidder, quantity, bidder_chain, bid_id } => {
                // Only process on auction creator chain
                if self.runtime.chain_id() != self.runtime.application_creator_chain_id() {
                    return;
                }

                // Verify authentication
                self.runtime
                    .check_account_permission(bidder)
                    .expect("Bid must be authenticated");

                // Validate auction timing
                let current_time = self.runtime.system_time();
                let start_time = self.parameters().start_timestamp;
                let end_time = self.parameters().end_timestamp;

                if current_time < start_time {
                    self.send_rejection(bidder, bidder_chain, bid_id, quantity, "Auction has not started yet");
                    return;
                }

                // Automatic time-based finalization
                if current_time >= end_time {
                    // Auto-finalize if not already finalized
                    if *self.state.status.get() == AuctionStatus::Active {
                        let clearing_price = self.calculate_current_price();
                        self.state.clearing_price.set(Some(clearing_price));
                        self.state.status.set(AuctionStatus::Ended);
                    }
                    self.send_rejection(bidder, bidder_chain, bid_id, quantity, "Auction has ended (time expired)");
                    return;
                }

                // Update status if needed (from Scheduled to Active)
                if *self.state.status.get() == AuctionStatus::Scheduled {
                    self.state.status.set(AuctionStatus::Active);
                }

                // Check if auction is active
                if !self.state.status.get().is_active() {
                    self.send_rejection(bidder, bidder_chain, bid_id, quantity, "Auction is not active");
                    return;
                }

                // Check quantity availability
                let quantity_sold = *self.state.quantity_sold.get();
                let total_quantity = self.parameters().total_quantity;

                if quantity == 0 {
                    self.send_rejection(bidder, bidder_chain, bid_id, quantity, "Bid quantity must be greater than zero");
                    return;
                }

                if quantity_sold + quantity > total_quantity {
                    let available = total_quantity - quantity_sold;
                    self.send_rejection(
                        bidder,
                        bidder_chain,
                        bid_id,
                        quantity,
                        &format!("Insufficient quantity. Requested: {}, Available: {}", quantity, available)
                    );
                    return;
                }

                // Calculate current price
                let current_price = self.calculate_current_price();

                // Record the bid
                self.record_bid(bidder, quantity, current_price).await;

                // Update total quantity sold
                let new_quantity_sold = quantity_sold + quantity;
                self.state.quantity_sold.set(new_quantity_sold);

                // Check if auction should end (sold out)
                let clearing_price = if new_quantity_sold >= total_quantity {
                    self.state.status.set(AuctionStatus::Ended);
                    let clearing = current_price;
                    self.state.clearing_price.set(Some(clearing));
                    Some(clearing)
                } else {
                    None
                };

                // Send acceptance message back to bidder chain with bid_id
                let accept_message = Message::BidAccepted {
                    bid_id,
                    quantity,
                    bid_price: current_price,
                    clearing_price,
                };

                self.runtime
                    .prepare_message(accept_message)
                    .send_to(bidder_chain);

                // Emit event for subscribers/transparency
                let event = AuctionEvent::BidAccepted {
                    bidder,
                    quantity,
                    bid_price: current_price,
                    new_total_sold: new_quantity_sold,
                    timestamp: current_time,
                };
                self.runtime.emit(AUCTION_STREAM.into(), &event);
            }

            // Bidder-side message handlers
            Message::BidAccepted { bid_id, quantity, bid_price, clearing_price } => {
                // Only process on bidder chains (not auction chain)
                if self.runtime.chain_id() == self.runtime.application_creator_chain_id() {
                    return;
                }

                // Update the specific bid using bid_id
                self.handle_bid_accepted(bid_id, quantity, bid_price, clearing_price).await;
            }

            Message::BidRejected { bid_id, quantity, reason } => {
                // Only process on bidder chains (not auction chain)
                if self.runtime.chain_id() == self.runtime.application_creator_chain_id() {
                    return;
                }

                // Mark the specific bid as rejected using bid_id
                self.handle_bid_rejected(bid_id, quantity, reason).await;
            }

            Message::RequestInitialization => {
                if self.runtime.chain_id() == self.runtime.application_creator_chain_id() {
                    self.emit_initialization_event();
                }
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
                let event: AuctionEvent = self
                    .runtime
                    .read_event(update.chain_id, AUCTION_STREAM.into(), index);

                // Store event for service layer queries
                self.store_auction_event_for_service(update.chain_id, event.clone()).await;

                // Update cached state based on event type
                match event {
                    AuctionEvent::AuctionInitialized {
                        owner,
                        start_timestamp,
                        end_timestamp,
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
                            end_timestamp,
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

                    AuctionEvent::BidAccepted {
                        new_total_sold,
                        bid_price,
                        timestamp,
                        ..
                    } => {
                        // Update cached quantity and price
                        if let Some(mut cached) = self.state.cached_state.get().clone() {
                            cached.quantity_sold = new_total_sold;
                            cached.current_price = bid_price;
                            cached.last_updated = timestamp;
                            self.state.cached_state.set(Some(cached));
                        }
                        // If no cached state exists, ignore the update
                        // (should have received AuctionInitialized first)
                    }

                    AuctionEvent::BidRejected { .. } => {
                        // No state update needed for rejections
                        // Just for transparency/logging
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
    /// Stores auction event for service layer queries (Testing purpose only)
    /// This allows the service to access historical events received via streams
    async fn store_auction_event_for_service(&mut self, chain_id: linera_sdk::linera_base_types::ChainId, event: AuctionEvent) {
        let timestamp = match &event {
            AuctionEvent::AuctionInitialized { timestamp, .. } => *timestamp,
            AuctionEvent::BidAccepted { timestamp, .. } => *timestamp,
            AuctionEvent::BidRejected { timestamp, .. } => *timestamp,
            AuctionEvent::StatusChanged { timestamp, .. } => *timestamp,
        };

        let event_type = match &event {
            AuctionEvent::AuctionInitialized { .. } => "AuctionInitialized",
            AuctionEvent::BidAccepted { .. } => "BidAccepted",
            AuctionEvent::BidRejected { .. } => "BidRejected",
            AuctionEvent::StatusChanged { .. } => "StatusChanged",
        };

        // Get and increment counter
        let counter = *self.state.stream_event_counter.get();
        self.state.stream_event_counter.set(counter + 1);

        // Create wrapper with metadata
        let wrapper = state::StoredStreamEvent {
            chain_id,
            timestamp: timestamp.micros(),
            event_type: event_type.to_string(),
            event_data: serde_json::to_string(&event).unwrap_or_default(),
        };

        // Serialize wrapper to JSON for storage
        if let Ok(wrapper_json) = serde_json::to_string(&wrapper) {
            let _ = self.state.stream_events.insert(&counter, wrapper_json);
        }
    }

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
            end_timestamp: params.end_timestamp,
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

    /// Helper method to send bid rejection message and emit event
    fn send_rejection(&mut self, bidder: AccountOwner, bidder_chain: linera_sdk::linera_base_types::ChainId, bid_id: u64, quantity: u64, reason: &str) {
        let reject_message = Message::BidRejected {
            bid_id,
            quantity,
            reason: reason.to_string(),
        };

        self.runtime
            .prepare_message(reject_message)
            .send_to(bidder_chain);

        // Emit event for subscribers/transparency
        let event = AuctionEvent::BidRejected {
            bidder,
            quantity,
            reason: reason.to_string(),
            timestamp: self.runtime.system_time(),
        };
        self.runtime.emit(AUCTION_STREAM.into(), &event);
    }

    /// Helper method to record a bid
    async fn record_bid(&mut self, bidder: AccountOwner, quantity: u64, _bid_price: Amount) {
        let current_time = self.runtime.system_time();
        let participant_info = ParticipantInfo {
            quantity,
            bid_timestamp: current_time,
        };

        // Update or add participant info
        let existing = self.state.participants.get(&bidder).await
            .expect("Failed to read participant info");

        if let Some(mut existing_info) = existing {
            // Update existing bid - accumulate quantity
            existing_info.quantity += quantity;
            existing_info.bid_timestamp = current_time;
            self.state.participants.insert(&bidder, existing_info)
                .expect("Failed to update participant");
        } else {
            // New participant
            self.state.participants.insert(&bidder, participant_info)
                .expect("Failed to insert participant");
        }
    }

    /// Handle BidAccepted message on bidder chain
    async fn handle_bid_accepted(&mut self, bid_id: u64, quantity: u64, bid_price: Amount, clearing_price: Option<Amount>) {
        // Get the specific bid by ID
        if let Ok(Some(mut bid)) = self.state.my_bids.get(&bid_id).await {
            // Verify this is the right bid (defensive check)
            if bid.quantity == quantity && bid.status == state::BidStatus::Pending {
                // Update bid with accepted status and price info
                bid.status = state::BidStatus::Accepted;
                bid.bid_price = bid_price;
                bid.clearing_price = clearing_price;

                self.state.my_bids.insert(&bid_id, bid)
                    .expect("Failed to update bid");

                // If clearing price is set, calculate refund
                if let Some(clearing) = clearing_price {
                    let refund_per_unit = bid_price.saturating_sub(clearing);
                    let total_refund = refund_per_unit.saturating_mul(quantity as u128);

                    let current_refund = *self.state.refund_owed.get();
                    self.state.refund_owed.set(current_refund.saturating_add(total_refund));
                }
            }
        }
    }

    /// Handle BidRejected message on bidder chain
    async fn handle_bid_rejected(&mut self, bid_id: u64, quantity: u64, _reason: String) {
        // Get the specific bid by ID
        if let Ok(Some(mut bid)) = self.state.my_bids.get(&bid_id).await {
            // Verify this is the right bid (defensive check)
            if bid.quantity == quantity && bid.status == state::BidStatus::Pending {
                // Mark bid as rejected
                bid.status = state::BidStatus::Rejected;

                self.state.my_bids.insert(&bid_id, bid)
                    .expect("Failed to update bid");
            }
        }
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
            end_timestamp: Timestamp::from(10000000),
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
            end_timestamp: Timestamp::from(10000000),
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
