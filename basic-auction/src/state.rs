// Copyright (c) Fairdrop Contributors
// SPDX-License-Identifier: Apache-2.0

use async_graphql::SimpleObject;
use linera_sdk::{
    linera_base_types::{AccountOwner, Amount, ChainId, Timestamp},
    views::{linera_views, MapView, RegisterView, RootView, ViewStorageContext},
};
use serde::{Deserialize, Serialize};

use fairdrop_basic::{AuctionParameters, AuctionStatus};
   
/// Information about a participant's bid in the auction
#[derive(Clone, Debug, Deserialize, Serialize, SimpleObject)]
pub struct ParticipantInfo {
    /// Quantity of units the participant wants to purchase
    pub quantity: u64,

    /// Timestamp when the bid was placed
    pub bid_timestamp: Timestamp,
}

// Note: AuctionStatus is now defined in lib.rs and imported above

/// The Fairdrop auction state
#[derive(RootView, SimpleObject)]
#[view(context = ViewStorageContext)]
pub struct AuctionState {
    /// Auction configuration parameters (stored at instantiation)
    /// Only set on the creator chain
    pub parameters: RegisterView<Option<AuctionParameters>>,

    /// Current status of the auction
    pub status: RegisterView<AuctionStatus>,

    /// Mapping of participants (AccountOwner) to their bid information
    pub participants: MapView<AccountOwner, ParticipantInfo>,

    /// Total quantity sold so far
    pub quantity_sold: RegisterView<u64>,

    /// Cached state for chains subscribed to updates
    /// This is only used on non-creator chains that have subscribed to events
    pub cached_state: RegisterView<Option<CachedAuctionState>>,

    /// Counter for stream events (auto-incrementing)
    pub stream_event_counter: RegisterView<u64>,

    /// Stream events storage for service layer queries
    /// Key: event_counter (u64), Value: JSON with chain_id, timestamp, and event data
    /// This allows the service to query historical events received via streams
    pub stream_events: MapView<u64, String>,
}

/// Wrapper for stored stream events with metadata
#[derive(Clone, Debug, Deserialize, Serialize, SimpleObject)]
pub struct StoredStreamEvent {
    /// Chain ID where the event originated
    pub chain_id: ChainId,
    /// Timestamp of the event in microseconds
    pub timestamp: u64,
    /// Event type: "AuctionInitialized", "BidPlaced", or "StatusChanged"
    pub event_type: String,
    /// JSON serialized event data
    pub event_data: String,
}

/// Cached auction state for chains subscribed to updates
/// This allows non-creator chains to serve queries without hitting the creator chain
#[derive(Clone, Debug, Deserialize, Serialize, SimpleObject)]
pub struct CachedAuctionState {
    /// Auction parameters (copied from creator chain)
    pub owner: AccountOwner,
    pub start_timestamp: Timestamp,
    pub start_price: Amount,
    pub floor_price: Amount,
    pub decrement_rate: Amount,
    pub decrement_interval: u64,
    pub total_quantity: u64,

    /// Total quantity sold (from latest event)
    pub quantity_sold: u64,

    /// Current auction status (from latest event)
    pub status: AuctionStatus,

    /// Last known current price
    pub current_price: Amount,

    /// Timestamp of last update
    pub last_updated: Timestamp,
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
