// Copyright (c) Fairdrop Contributors
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{scalar, SimpleObject};
use linera_sdk::{
    linera_base_types::{AccountOwner, Amount, Timestamp},
    views::{linera_views, MapView, RegisterView, RootView, ViewStorageContext},
};
use serde::{Deserialize, Serialize};

use fairdrop_basic::AuctionParameters;
   
/// Information about a participant's bid in the auction
#[derive(Clone, Debug, Deserialize, Serialize, SimpleObject)]
pub struct ParticipantInfo {
    /// Quantity of units the participant wants to purchase
    pub quantity: u64,

    /// Timestamp when the bid was placed
    pub bid_timestamp: Timestamp,
}

/// The status of the auction
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum AuctionStatus {
    /// Auction is scheduled but hasn't started yet
    Scheduled,

    /// Auction is active and accepting bids
    #[default]
    Active,

    /// Auction has ended (sold out or reached floor price)
    Ended,
}

scalar!(AuctionStatus);

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

/// The Fairdrop auction state
#[derive(RootView, SimpleObject)]
#[view(context = ViewStorageContext)]
pub struct AuctionState {
    /// Auction configuration parameters (stored at instantiation)
    pub parameters: RegisterView<Option<AuctionParameters>>,

    /// Current status of the auction
    pub status: RegisterView<AuctionStatus>,

    /// Mapping of participants (AccountOwner) to their bid information
    pub participants: MapView<AccountOwner, ParticipantInfo>,

    /// Total quantity sold so far
    pub quantity_sold: RegisterView<u64>,
}

/// Comprehensive auction information
#[derive(SimpleObject)]
pub struct AuctionInfo {
    pub owner: AccountOwner,
    pub start_timestamp: Timestamp,
    pub start_price: Amount,
    pub floor_price: Amount,
    pub decrement_rate: Amount,
    pub decrement_interval: u64,
    pub total_quantity: u64,
    pub quantity_sold: u64,
    pub quantity_remaining: u64,
    pub current_price: Amount,
    pub status: AuctionStatus,
    pub current_time: Timestamp,
    pub time_until_next_decrement: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auction_status_is_active() {
        assert!(AuctionStatus::Active.is_active());
        assert!(!AuctionStatus::Scheduled.is_active());
        assert!(!AuctionStatus::Ended.is_active());
    }

    #[test]
    fn test_auction_status_is_ended() {
        assert!(AuctionStatus::Ended.is_ended());
        assert!(!AuctionStatus::Active.is_ended());
        assert!(!AuctionStatus::Scheduled.is_ended());
    }

    #[test]
    fn test_auction_status_is_scheduled() {
        assert!(AuctionStatus::Scheduled.is_scheduled());
        assert!(!AuctionStatus::Active.is_scheduled());
        assert!(!AuctionStatus::Ended.is_scheduled());
    }

    #[test]
    fn test_auction_status_default() {
        let status = AuctionStatus::default();
        assert_eq!(status, AuctionStatus::Active);
        assert!(status.is_active());
    }

    #[test]
    fn test_auction_status_serialization() {
        let status = AuctionStatus::Scheduled;
        let json = serde_json::to_string(&status).expect("Serialization failed");
        let deserialized: AuctionStatus =
            serde_json::from_str(&json).expect("Deserialization failed");
        assert_eq!(deserialized, status);
    }

    #[test]
    fn test_participant_info_serialization() {
        let info = ParticipantInfo {
            quantity: 100,
            bid_timestamp: Timestamp::from(5000000),
        };

        let json = serde_json::to_string(&info).expect("Serialization failed");
        let deserialized: ParticipantInfo =
            serde_json::from_str(&json).expect("Deserialization failed");

        assert_eq!(deserialized.quantity, info.quantity);
        assert_eq!(deserialized.bid_timestamp, info.bid_timestamp);
    }

    #[test]
    fn test_participant_info_clone() {
        let info = ParticipantInfo {
            quantity: 200,
            bid_timestamp: Timestamp::from(6000000),
        };

        let cloned = info.clone();
        assert_eq!(cloned.quantity, info.quantity);
        assert_eq!(cloned.bid_timestamp, info.bid_timestamp);
    }
}
