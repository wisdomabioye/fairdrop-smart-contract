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

pub mod state;

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

/// Auction configuration parameters provided at initialization
/// Note: The owner is automatically set to the authenticated caller during instantiation
#[derive(Clone, Copy, Debug, Deserialize, Serialize, SimpleObject)]
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
    /// Owner of the auction (receives proceeds) - set from authenticated caller
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

impl std::fmt::Display for InstantiationArgument {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "Auction: starts={:?}, start_price={}, floor_price={}, decrement_rate={}, interval={}s, quantity={}",
            self.start_timestamp, self.start_price, self.floor_price,
            self.decrement_rate, self.decrement_interval, self.total_quantity
        )
    }
}

impl std::fmt::Display for AuctionParameters {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "Auction: owner={}, starts={:?}, start_price={}, floor_price={}, decrement_rate={}, interval={}s, quantity={}",
            self.owner, self.start_timestamp, self.start_price, self.floor_price,
            self.decrement_rate, self.decrement_interval, self.total_quantity
        )
    }
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


#[cfg(test)]
mod tests {
    use super::*;
    use linera_sdk::linera_base_types::{AccountOwner, Amount};
    use std::str::FromStr;

    #[test]
    fn test_instantiation_argument_display() {
        let arg = InstantiationArgument {
            start_timestamp: Timestamp::from(1000000),
            start_price: Amount::from_tokens(100),
            floor_price: Amount::from_tokens(10),
            decrement_rate: Amount::from_tokens(1),
            decrement_interval: 60,
            total_quantity: 1000,
        };

        let display = format!("{}", arg);
        assert!(display.contains("Auction"));
        assert!(display.contains("start_price="));
        assert!(display.contains("floor_price="));
    }

    #[test]
    fn test_auction_parameters_display() {
        let owner = AccountOwner::from_str("owner").unwrap();
        let params = AuctionParameters {
            owner,
            start_timestamp: Timestamp::from(1000000),
            start_price: Amount::from_tokens(100),
            floor_price: Amount::from_tokens(10),
            decrement_rate: Amount::from_tokens(1),
            decrement_interval: 60,
            total_quantity: 1000,
        };

        let display = format!("{}", params);
        assert!(display.contains("Auction"));
        assert!(display.contains("owner="));
        assert!(display.contains("start_price="));
        assert!(display.contains("floor_price="));
    }

    #[test]
    fn test_instantiation_argument_serialization() {
        let arg = InstantiationArgument {
            start_timestamp: Timestamp::from(2000000),
            start_price: Amount::from_tokens(200),
            floor_price: Amount::from_tokens(20),
            decrement_rate: Amount::from_tokens(2),
            decrement_interval: 120,
            total_quantity: 500,
        };

        // Test serialization and deserialization
        let json = serde_json::to_string(&arg).expect("Serialization failed");
        let deserialized: InstantiationArgument =
            serde_json::from_str(&json).expect("Deserialization failed");

        assert_eq!(deserialized.start_timestamp, arg.start_timestamp);
        assert_eq!(deserialized.start_price, arg.start_price);
        assert_eq!(deserialized.floor_price, arg.floor_price);
        assert_eq!(deserialized.decrement_rate, arg.decrement_rate);
        assert_eq!(deserialized.decrement_interval, arg.decrement_interval);
        assert_eq!(deserialized.total_quantity, arg.total_quantity);
    }

    #[test]
    fn test_operation_serialization() {
        let op = Operation::PlaceBid { quantity: 100 };

        // Test serialization and deserialization
        let json = serde_json::to_string(&op).expect("Serialization failed");
        let deserialized: Operation = serde_json::from_str(&json).expect("Deserialization failed");

        match deserialized {
            Operation::PlaceBid { quantity } => assert_eq!(quantity, 100),
        }
    }

    #[test]
    fn test_instantiation_argument_json_linera_format() {
        // Test with Linera's JSON format (Amount as decimal string)
        let json_str = r#"{
            "start_timestamp": 1762598125457000,
            "start_price": "100.",
            "floor_price": "1.",
            "decrement_rate": "1.",
            "decrement_interval": 600,
            "total_quantity": 250000
        }"#;

        let result = serde_json::from_str::<InstantiationArgument>(json_str);
        assert!(result.is_ok(), "Should deserialize Linera JSON format: {:?}", result.err());

        let arg = result.unwrap();
        assert_eq!(arg.start_timestamp, Timestamp::from(1762598125457000));
        assert_eq!(arg.start_price, Amount::from_tokens(100));
        assert_eq!(arg.floor_price, Amount::from_tokens(1));
        assert_eq!(arg.decrement_rate, Amount::from_tokens(1));
    }

    #[test]
    fn test_instantiation_argument_bcs_roundtrip() {
        use linera_sdk::bcs;

        let arg = InstantiationArgument {
            start_timestamp: Timestamp::from(1762598125457000),
            start_price: Amount::from_tokens(100),
            floor_price: Amount::from_tokens(1),
            decrement_rate: Amount::from_tokens(1),
            decrement_interval: 600,
            total_quantity: 250000,
        };

        // Test BCS serialization
        let bytes = bcs::to_bytes(&arg).expect("BCS serialization should work");

        // Test BCS deserialization
        let deserialized: InstantiationArgument = bcs::from_bytes(&bytes)
            .expect("BCS deserialization should work");

        assert_eq!(deserialized.start_timestamp, arg.start_timestamp);
        assert_eq!(deserialized.start_price, arg.start_price);
        assert_eq!(deserialized.floor_price, arg.floor_price);
        assert_eq!(deserialized.decrement_rate, arg.decrement_rate);
    }
}
