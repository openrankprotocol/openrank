use base64::prelude::*;
use getset::Getters;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::{
    runners::Error as BaseRunnerError,
    tx::trust::{ScoreEntry, TrustEntry},
};

/// Local trust object.
///
/// The local trust object stores the trust values that a node assigns to its
/// peers.
///
/// It also stores the sum of the trust values assigned to all peers.
#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct OutboundLocalTrust {
    /// The trust values that a node assigns to its peers.
    ///
    /// The `outbound_trust_scores` vector stores the trust values that a node
    /// assigns to its peers. The trust values are represented as a vector of
    /// floats, where each element in the vector corresponds to the trust value
    /// assigned to a particular peer.
    outbound_trust_scores: BTreeMap<u64, f32>,
    /// The sum of the trust values assigned to all peers.
    ///
    /// The `outbound_sum` value stores the sum of the trust values assigned to
    /// all peers. The sum is used to normalize the trust values such that they
    /// add up to 1.
    outbound_sum: f32,
}

impl Default for OutboundLocalTrust {
    fn default() -> Self {
        Self::new()
    }
}

impl OutboundLocalTrust {
    pub fn new() -> Self {
        Self { outbound_trust_scores: BTreeMap::new(), outbound_sum: 0.0 }
    }

    pub fn set_outbound_trust_scores(&mut self, outbound_trust_scores: BTreeMap<u64, f32>) {
        self.outbound_trust_scores = outbound_trust_scores;
        self.outbound_sum = self.outbound_trust_scores.values().sum();
    }

    pub fn from_score_map(score_map: &BTreeMap<u64, f32>) -> Self {
        let outbound_trust_scores = score_map.clone();
        let outbound_sum = outbound_trust_scores.values().sum();
        Self { outbound_trust_scores, outbound_sum }
    }

    pub fn norm(&self) -> Self {
        let mut outbound_trust_scores = self.outbound_trust_scores.clone();
        for (_, score) in outbound_trust_scores.iter_mut() {
            *score /= self.outbound_sum;
        }
        let outbound_sum = 1.0;
        OutboundLocalTrust { outbound_trust_scores, outbound_sum }
    }

    /*----------------- BTreeMap similar utils -----------------*/
    pub fn get(&self, peer_id: &u64) -> Option<f32> {
        self.outbound_trust_scores.get(peer_id).copied()
    }

    pub fn contains_key(&self, peer_id: &u64) -> bool {
        self.outbound_trust_scores.contains_key(peer_id)
    }

    pub fn remove(&mut self, peer_id: &u64) {
        let to_be_removed = self.outbound_trust_scores.get(peer_id).copied().unwrap_or(0.0);
        self.outbound_sum -= to_be_removed;
        self.outbound_trust_scores.remove(peer_id);
    }

    pub fn insert(&mut self, peer_id: u64, value: f32) {
        let prev_value = self.outbound_trust_scores.get(&peer_id).copied().unwrap_or(0.0);
        self.outbound_sum -= prev_value;
        self.outbound_sum += value;
        self.outbound_trust_scores.insert(peer_id, value);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct LocalTrustStateResponse {
    result: Vec<TrustEntry>,
    next_token: Option<String>,
}

impl LocalTrustStateResponse {
    pub fn new(result: Vec<TrustEntry>, next_token: Option<String>) -> Self {
        Self { result, next_token }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct SeedTrustStateResponse {
    result: Vec<ScoreEntry>,
    next_token: Option<String>,
}

impl SeedTrustStateResponse {
    pub fn new(result: Vec<ScoreEntry>, next_token: Option<String>) -> Self {
        Self { result, next_token }
    }
}

/// Computes the range of local/seed trust peers for pagination.
///
/// This function calculates the start and end indices for a range of peers
/// based on the provided `next_token` and `page_size`. The `next_token` is
/// expected to be a base64 encoded string representing an 8-byte integer,
/// which determines the starting peer index (`start_peer`). If `next_token`
/// is `None`, the starting index defaults to 0.
///
/// The `end_peer` is calculated by adding the `page_size` (defaulting to 1000
/// if not provided) to the `start_peer`. The `end_peer` is capped at
/// `peers_cnt` to ensure it does not exceed the total number of peers.
///
/// # Arguments
///
/// * `peers_cnt` - The total number of local/seed trust peers.
/// * `page_size` - Optional size of the page, determining the number of peers
///   in the range.
/// * `next_token` - Optional base64 encoded string used to determine the
///   starting peer index.
///
/// # Returns
///
/// A `Result` containing a tuple with the start and end indices of the peer
/// range, or a `BaseRunnerError` if decoding the `next_token` fails.
pub fn compute_peer_range(
    peers_cnt: u64, page_size: Option<usize>, next_token: Option<String>,
) -> Result<(u64, u64), BaseRunnerError> {
    let page_size = if let Some(page_size) = page_size { page_size as u64 } else { peers_cnt };

    let start_peer = decode_next_token(next_token)?;
    let start_peer = std::cmp::min(start_peer, peers_cnt);

    let end_peer = start_peer + page_size;
    let end_peer = std::cmp::min(end_peer, peers_cnt);

    Ok((start_peer, end_peer))
}

/// Creates a next token for local/seed trust pagination.
///
/// This function generates a base64 encoded string that represents the `next_peer_id` as an 8-byte integer.
/// The `next_peer_id` must be greater than 0 and less than `peers_cnt` to generate a non-empty token.
///
/// # Arguments
///
/// * `peers_cnt` - Total number of local/seed trust peers. It defines the upper limit for `next_peer_id`.
/// * `next_peer_id` - The peer ID for which the next token is to be created.
///
/// # Returns
///
/// An `Option<String>` containing the base64 encoded next token if `next_peer_id` is in range; otherwise, `None`.
pub fn create_next_token(peers_cnt: u64, next_peer_id: u64) -> Option<String> {
    if next_peer_id == 0 || next_peer_id == peers_cnt {
        None
    } else {
        let id_bytes = next_peer_id.to_be_bytes();
        let next_token = BASE64_STANDARD.encode(id_bytes);
        Some(next_token)
    }
}

pub fn decode_next_token(next_token: Option<String>) -> Result<u64, BaseRunnerError> {
    let next_peer_id = match next_token {
        Some(token) => {
            let decoded_bytes =
                BASE64_STANDARD.decode(token).map_err(BaseRunnerError::Base64Decode)?;
            let id_bytes = TryInto::<[u8; 8]>::try_into(decoded_bytes).map_err(|e| {
                BaseRunnerError::Misc(format!("Failed to convert to 8 bytes: {:?}", e))
            })?;
            u64::from_be_bytes(id_bytes)
        },
        None => 0,
    };
    Ok(next_peer_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_peer_range() {
        // st: 100, page_size: 100, next_token: None
        // 0 => 100
        let (from_peer_start, from_peer_end) = compute_peer_range(100, None, None).unwrap();
        assert_eq!(from_peer_start, 0);
        assert_eq!(from_peer_end, 100);

        // st: 100, page_size: 55, next_token: None
        // 0 => 55
        let (from_peer_start, from_peer_end) = compute_peer_range(100, Some(55), None).unwrap();
        assert_eq!(from_peer_start, 0);
        assert_eq!(from_peer_end, 55);

        // st: 100, page_size: 100, next_token: "AAAAAAAAADc="
        // 55 => 100
        let (from_peer_start, from_peer_end) =
            compute_peer_range(100, Some(100), Some("AAAAAAAAADc=".to_string())).unwrap();
        assert_eq!(from_peer_start, 55);
        assert_eq!(from_peer_end, 100);

        // st: 100, page_size: 10, next_token: "AAAAAAAAADc="
        // 55 => 65
        let (from_peer_start, from_peer_end) =
            compute_peer_range(100, Some(10), Some("AAAAAAAAADc=".to_string())).unwrap();
        assert_eq!(from_peer_start, 55);
        assert_eq!(from_peer_end, 65);
    }

    #[test]
    fn test_create_next_token() {
        // st: 100, next_peer_id: 0
        let next_token = create_next_token(100, 0);
        assert_eq!(next_token, None);

        // st:100 next_peer_id: 100
        let next_token = create_next_token(100, 100);
        assert_eq!(next_token, None);

        // st: 100, next_peer_id: 55
        let next_token = create_next_token(100, 55);
        assert_eq!(next_token, Some("AAAAAAAAADc=".to_string()));
    }
}
