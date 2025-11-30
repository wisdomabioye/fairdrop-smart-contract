// Copyright (c) Fairdrop Contributors
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Enum, Request, Response, SimpleObject};
use linera_sdk::{
    graphql::GraphQLMutationRoot,
    linera_base_types::{AccountOwner, Amount, ContractAbi, ServiceAbi, Timestamp},
};
use serde::{Deserialize, Serialize};

// AuctionStatus type - defined here to be shared between lib and state modules
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Enum)]
pub enum AuctionStatus {
    Scheduled,
    #[default]
    Active,
    Ended,
}

impl AuctionStatus {
    /// Returns `true` if the auction is active and accepting bids
    pub fn is_active(&self) -> bool {
        matches!(self, AuctionStatus::Active)
    }

    /// Returns `true` if the auction has ended
    pub fn is_ended(&self) -> bool {
        matches!(self, AuctionStatus::Ended)
    }

    /// Returns `true` if the auction is scheduled for the future
    pub fn is_scheduled(&self) -> bool {
        matches!(self, AuctionStatus::Scheduled)
    }
}

/// ABI for the Fairdrop Stage 1 application
pub struct FairdropAbi;

impl ContractAbi for FairdropAbi {
    type Operation = Operation;
    type Response = ();
}

impl ServiceAbi for FairdropAbi {
    type Query = Request;
    type QueryResponse = Response;
}

/// Instantiation argument (excludes owner, which is set from authenticated caller)
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct InstantiationArgument {
    /// When the auction starts (allows for scheduled/upcoming auctions)
    pub start_timestamp: Timestamp,

    /// When the auction ends (time-based finalization)
    pub end_timestamp: Timestamp,

    /// Starting price per unit
    pub start_price: Amount,

    /// Minimum floor price
    pub floor_price: Amount,

    /// Amount to decrease price per interval
    pub decrement_rate: Amount,

    /// Time interval between price decrements (in seconds)
    pub decrement_interval: u64,

    /// Total quantity available for auction
    pub total_quantity: u64,
}

/// Stored auction parameters (includes owner determined at instantiation)
#[derive(Clone, Copy, Debug, Deserialize, Serialize, SimpleObject)]
pub struct AuctionParameters {
    /// Owner of the auction (receives proceeds) - set from authenticated caller during instantiation
    pub owner: AccountOwner,

    /// When the auction starts (allows for scheduled/upcoming auctions)
    pub start_timestamp: Timestamp,

    /// When the auction ends (time-based finalization)
    pub end_timestamp: Timestamp,

    /// Starting price per unit
    pub start_price: Amount,

    /// Minimum floor price
    pub floor_price: Amount,

    /// Amount to decrease price per interval
    pub decrement_rate: Amount,

    /// Time interval between price decrements (in seconds)
    pub decrement_interval: u64,

    /// Total quantity available for auction
    pub total_quantity: u64,
}

/// Operations that users can perform on the auction
#[derive(Debug, Deserialize, Serialize, GraphQLMutationRoot)]
pub enum Operation {
    /// Place a bid for a specified quantity at the current price
    PlaceBid {
        /// Quantity of units to purchase
        quantity: u64
    },

    /// Subscribe to auction updates from the creator chain
    /// This allows a chain to receive real-time updates via event streaming
    Subscribe,

    /// Unsubscribe from auction updates
    Unsubscribe,

    // Future: Create a new instance of this application with the given parameters
    // (Dynamic application creation not implemented yet)
    // CreateApplication { ... }
}

/// Messages for cross-chain communication (Two-Contract Pattern)
#[derive(Debug, Deserialize, Serialize)]
pub enum Message {
    /// User → Auction: Submit a bid at current price
    BidSubmission {
        /// The bidder placing the bid
        bidder: AccountOwner,
        /// Quantity of units to purchase
        quantity: u64,
        /// Chain ID where the bidder is located (for response routing)
        bidder_chain: linera_sdk::linera_base_types::ChainId,
        /// Bid ID from the bidder chain (for matching responses)
        bid_id: u64,
    },

    /// Auction → User: Bid accepted, record bid_price and clearing_price
    BidAccepted {
        /// Bid ID from the original submission (for matching)
        bid_id: u64,
        /// Quantity that was accepted
        quantity: u64,
        /// Price when bid was placed
        bid_price: Amount,
        /// Clearing price if auction ended, None if still active
        clearing_price: Option<Amount>,
    },

    /// Auction → User: Bid rejected (quantity exceeded or auction ended)
    BidRejected {
        /// Bid ID from the original submission (for matching)
        bid_id: u64,
        /// Quantity that was rejected
        quantity: u64,
        /// Reason for rejection
        reason: String,
    },

    RequestInitialization,
}

/// Events for streaming auction updates to subscribed chains
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum AuctionEvent {
    /// Auction parameters initialized - sent when a chain subscribes
    /// This contains all the static configuration needed to interpret other events
    AuctionInitialized {
        owner: AccountOwner,
        start_timestamp: Timestamp,
        end_timestamp: Timestamp,
        start_price: Amount,
        floor_price: Amount,
        decrement_rate: Amount,
        decrement_interval: u64,
        total_quantity: u64,
        current_quantity_sold: u64,
        current_status: AuctionStatus,
        current_price: Amount,
        timestamp: Timestamp,
    },

    /// A bid was placed
    BidPlaced {
        bidder: AccountOwner,
        quantity: u64,
        new_total_sold: u64,
        current_price: Amount,
        timestamp: Timestamp,
    },

    /// Auction status changed
    StatusChanged {
        new_status: AuctionStatus,
        timestamp: Timestamp,
    },
}
