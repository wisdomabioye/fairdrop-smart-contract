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

    /// Claim tokens and refund after auction ends
    /// Sends a message to creator chain to process the claim
    /// Refund = total_paid - (clearing_price × total_quantity)
    Claim,

    /// Subscribe to auction updates from the creator chain
    /// This allows a chain to receive real-time updates via event streaming
    Subscribe,

    /// Unsubscribe from auction updates
    Unsubscribe,

    // Future: Create a new instance of this application with the given parameters
    // (Dynamic application creation not implemented yet)
    // CreateApplication { ... }
}

/// Messages for cross-chain communication
#[derive(Debug, Deserialize, Serialize)]
pub enum Message {
    /// User → Auction: Submit a bid at current price (one-way message)
    BidSubmission {
        /// The bidder placing the bid
        bidder: AccountOwner,
        /// Quantity of units to purchase
        quantity: u64,
    },

    /// User → Auction: Claim tokens and refund after auction ends
    ClaimRequest {
        /// The bidder claiming their allocation
        bidder: AccountOwner,
    },

    /// Request initialization event (for Subscribe operation)
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

    /// A bid was accepted by the auction
    BidAccepted {
        bidder: AccountOwner,
        quantity: u64,
        bid_price: Amount,
        clearing_price: Option<Amount>,
        new_total_sold: u64,
        timestamp: Timestamp,
    },

    /// A bid was rejected by the auction
    BidRejected {
        bidder: AccountOwner,
        quantity: u64,
        reason: String,
        timestamp: Timestamp,
    },

    /// Claim processed - tokens and refund calculated
    ClaimProcessed {
        bidder: AccountOwner,
        total_quantity: u64,
        total_paid: Amount,
        clearing_price: Amount,
        refund: Amount,
        timestamp: Timestamp,
    },

    /// Auction status changed
    StatusChanged {
        new_status: AuctionStatus,
        timestamp: Timestamp,
    },
}
