// Copyright (c) Fairdrop Contributors
// SPDX-License-Identifier: Apache-2.0

use async_graphql::SimpleObject;
use linera_sdk::{
    linera_base_types::{AccountOwner, Amount, Timestamp},
    views::{linera_views, LogView, MapView, RegisterView, RootView, ViewStorageContext},
};
use serde::{Deserialize, Serialize};

use fairdrop_basic::{AuctionParameters, AuctionStatus};
   
/// Information about a participant's bids in the auction (Creator chain only)
/// Tracks accumulated totals for claim/refund calculation
#[derive(Clone, Debug, Deserialize, Serialize, SimpleObject)]
pub struct ParticipantInfo {
    /// Total quantity won across all bids
    pub total_quantity: u64,

    /// Total amount paid (sum of bid_price * quantity at time of each bid)
    /// Used to calculate refund: refund = total_paid - (clearing_price * total_quantity)
    pub total_paid: Amount,

    /// Whether participant has claimed their allocation and refund
    pub has_claimed: bool,

    /// Timestamp of last bid
    pub bid_timestamp: Timestamp,
}

/// The Fairdrop auction state
#[derive(RootView, SimpleObject)]
#[view(context = ViewStorageContext)]
pub struct AuctionState {
    // ========================================
    // CREATOR CHAIN ONLY
    // ========================================

    /// Auction configuration parameters (stored at instantiation)
    /// Only set on the creator chain
    pub parameters: RegisterView<Option<AuctionParameters>>,

    /// Current status of the auction
    pub status: RegisterView<AuctionStatus>,

    /// Mapping of participants (AccountOwner) to their bid information
    pub participants: MapView<AccountOwner, ParticipantInfo>,

    /// Total quantity sold so far
    pub quantity_sold: RegisterView<u64>,

    /// Clearing price (set when auction is finalized)
    pub clearing_price: RegisterView<Option<Amount>>,

    // ========================================
    // SUBSCRIBER CHAINS ONLY
    // ========================================

    /// Cached state for chains subscribed to updates
    /// This is only used on non-creator chains that have subscribed to events
    pub cached_state: RegisterView<Option<CachedAuctionState>>,

    /// Indexed bid storage by owner for O(1) lookup
    /// Maps AccountOwner to all their bids (accepts + rejects)
    pub bids_by_owner: MapView<AccountOwner, Vec<BidInfo>>,

    /// Chronological storage of all bids for history queries
    /// Bids are stored in insertion order (chronological)
    /// Enables efficient pagination and filtering
    pub all_bids: LogView<BidInfoWithOwner>,

    // Future: Counter for created applications (auto-incrementing)
    // pub created_applications_counter: RegisterView<u64>,

    // Future: Collection of created application IDs
    // pub created_applications: MapView<u64, ApplicationId>,
}

/// Information about a single bid (stored in bids_by_owner)
#[derive(Clone, Debug, Deserialize, Serialize, SimpleObject)]
pub struct BidInfo {
    /// Quantity of units bid for
    pub quantity: u64,

    /// Price when bid was placed
    pub bid_price: Amount,

    /// Timestamp when bid was placed
    pub timestamp: Timestamp,

    /// Status of this bid
    pub status: BidStatus,

    /// Clearing price (set when auction ends)
    pub clearing_price: Option<Amount>,
}

/// Information about a single bid with owner (stored in all_bids log)
#[derive(Clone, Debug, Deserialize, Serialize, SimpleObject)]
pub struct BidInfoWithOwner {
    /// The bidder's account
    pub bidder: AccountOwner,

    /// Quantity of units bid for
    pub quantity: u64,

    /// Price when bid was placed
    pub bid_price: Amount,

    /// Timestamp when bid was placed
    pub timestamp: Timestamp,

    /// Status of this bid
    pub status: BidStatus,

    /// Clearing price (set when auction ends)
    pub clearing_price: Option<Amount>,
}

/// Status of a bid
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, async_graphql::Enum)]
pub enum BidStatus {
    /// Bid was accepted by the auction
    Accepted,
    /// Bid was rejected by the auction
    Rejected,
}

/// Cached auction state for chains subscribed to updates
/// This allows non-creator chains to serve queries without hitting the creator chain
#[derive(Clone, Debug, Deserialize, Serialize, SimpleObject)]
pub struct CachedAuctionState {
    /// Auction parameters (copied from creator chain)
    pub owner: AccountOwner,
    pub start_timestamp: Timestamp,
    pub end_timestamp: Timestamp,
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
    pub end_timestamp: Timestamp,
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

/// Aggregated statistics for a bidder
#[derive(Clone, Debug, Deserialize, Serialize, SimpleObject)]
pub struct BidderSummary {
    /// Total quantity across all accepted bids
    pub total_quantity: u64,

    /// Total cost (sum of bid_price * quantity for accepted bids)
    pub total_cost: Amount,

    /// Total refund owed (if auction ended with clearing price)
    pub total_refund: Amount,

    /// Net cost after refunds
    pub net_cost: Amount,

    /// Number of accepted bids
    pub accepted_bids: u64,

    /// Number of rejected bids
    pub rejected_bids: u64,
}

/// Response for paginated bid queries
#[derive(Clone, Debug, Deserialize, Serialize, SimpleObject)]
pub struct BidHistoryResponse {
    /// The bids in the requested range
    pub bids: Vec<BidInfoWithOwner>,

    /// Total number of bids matching the filters
    pub total_count: usize,

    /// Whether there are more bids after this page
    pub has_more: bool,
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
            total_quantity: 100,
            total_paid: Amount::from_tokens(1000),
            has_claimed: false,
            bid_timestamp: Timestamp::from(5000000),
        };

        let json = serde_json::to_string(&info).expect("Serialization failed");
        let deserialized: ParticipantInfo =
            serde_json::from_str(&json).expect("Deserialization failed");

        assert_eq!(deserialized.total_quantity, info.total_quantity);
        assert_eq!(deserialized.total_paid, info.total_paid);
        assert_eq!(deserialized.has_claimed, info.has_claimed);
        assert_eq!(deserialized.bid_timestamp, info.bid_timestamp);
    }

    #[test]
    fn test_participant_info_clone() {
        let info = ParticipantInfo {
            total_quantity: 200,
            total_paid: Amount::from_tokens(2000),
            has_claimed: false,
            bid_timestamp: Timestamp::from(6000000),
        };

        let cloned = info.clone();
        assert_eq!(cloned.total_quantity, info.total_quantity);
        assert_eq!(cloned.total_paid, info.total_paid);
        assert_eq!(cloned.has_claimed, info.has_claimed);
        assert_eq!(cloned.bid_timestamp, info.bid_timestamp);
    }
}
