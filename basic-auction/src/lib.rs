// Copyright (c) Fairdrop Contributors
// SPDX-License-Identifier: Apache-2.0

/*!
# Fairdrop Stage 1: Basic Auction MVP

This is the first stage in building the Fairdrop auction system on Linera.
It implements core auction logic without external dependencies:
- Auction initialization with descending price parameters
- Bid placement (tracking quantity, no actual payment yet)
- Automatic price calculation based on elapsed time
- Query interface for current price and auction state
*/

use async_graphql::{Request, Response, SimpleObject};
use linera_sdk::{
    graphql::GraphQLMutationRoot,
    linera_base_types::{AccountOwner, Amount, ContractAbi, ServiceAbi, Timestamp},
};
use serde::{Deserialize, Serialize};

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
    /// Note: In Stage 1, no actual payment is made - we just track the bid
    PlaceBid {
        /// Quantity of units to purchase
        quantity: u64
    }
}

/// Messages for cross-chain communication (not used in Stage 1)
#[derive(Debug, Deserialize, Serialize)]
pub enum Message {
    /// Placeholder variant for Stage 1 (will be replaced in Stage 4)
    #[doc(hidden)]
    _Placeholder,
}
