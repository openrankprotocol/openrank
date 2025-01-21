use base64::prelude::*;
use getset::Getters;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    outbound_trust_scores: HashMap<u64, f32>,
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
        Self { outbound_trust_scores: HashMap::new(), outbound_sum: 0.0 }
    }

    pub fn set_outbound_trust_scores(&mut self, outbound_trust_scores: HashMap<u64, f32>) {
        self.outbound_trust_scores = outbound_trust_scores;
        self.outbound_sum = self.outbound_trust_scores.values().sum();
    }

    pub fn from_score_map(score_map: &HashMap<u64, f32>) -> Self {
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

    /*----------------- HashMap similar utils -----------------*/
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

/// Computes the range of local trust peers for pagination.
///
/// This function calculates the start and end indices of peers for the current
/// page based on the provided `page_size` and `next_token`. If a `next_token`
/// is provided, it decodes the token to get the starting positions. Otherwise,
/// it starts from the beginning. The function ensures that the end indices do
/// not exceed the total number of peers (`lt_peers_cnt`).
///
/// # Arguments
///
/// * `lt_peers_cnt` - Total number of local trust peers.
/// * `page_size` - Optional size of the page; defaults to 1000 if not provided.
/// * `next_token` - Optional base64-encoded string representing the starting
///   indices for the current page.
///
/// # Returns
///
/// A `Result` containing a tuple with the start and end indices for the
/// `from_peer` and `to_peer` ranges, or an error if decoding the token fails.
///
/// # Errors
///
/// Returns `BaseRunnerError` if the token decoding or conversion to bytes
/// fails.
pub fn compute_localtrust_peer_range(
    lt_peers_cnt: u64, page_size: Option<usize>, next_token: Option<String>,
) -> Result<(u64, u64, u64, u64), BaseRunnerError> {
    let page_size = if let Some(page_size) = page_size {
        page_size as u64
    } else {
        lt_peers_cnt * lt_peers_cnt
    };

    let (from_peer_start, to_peer_start) = match next_token {
        Some(token) => {
            let decoded_bytes =
                BASE64_STANDARD.decode(token).map_err(BaseRunnerError::Base64Decode)?;
            let id_bytes = TryInto::<[u8; 16]>::try_into(decoded_bytes).map_err(|e| {
                BaseRunnerError::Misc(format!("Failed to convert to 16 bytes: {:?}", e))
            })?;
            let first_bytes = id_bytes[0..8].try_into().map_err(|e| {
                BaseRunnerError::Misc(format!("Failed to convert to 8 bytes: {:?}", e))
            })?;
            let second_bytes = id_bytes[8..].try_into().map_err(|e| {
                BaseRunnerError::Misc(format!("Failed to convert to 8 bytes: {:?}", e))
            })?;
            (
                u64::from_be_bytes(first_bytes),
                u64::from_be_bytes(second_bytes),
            )
        },
        None => (0, 0),
    };
    let from_peer_start = std::cmp::min(from_peer_start, lt_peers_cnt);
    let to_peer_start = std::cmp::min(to_peer_start, lt_peers_cnt);

    let to_peer_end = to_peer_start + page_size;
    let from_peer_end = from_peer_start + to_peer_end / lt_peers_cnt;
    let from_peer_end = std::cmp::min(from_peer_end, lt_peers_cnt);
    let to_peer_end = to_peer_end % lt_peers_cnt;

    Ok((from_peer_start, from_peer_end, to_peer_start, to_peer_end))
}

/// Creates a next token for local trust pagination.
///
/// This function generates a base64 encoded string that represents the
/// `from_peer_id` and `to_peer_id` as 8-byte integers. The `from_peer_id`
/// must be less than `lt_peers_cnt` to generate a non-empty token.
///
/// # Arguments
///
/// * `lt_peers_cnt` - Total number of local trust peers. It defines the upper
///   limit for `from_peer_id`.
/// * `from_peer_id` - The peer ID for which the next token is to be created.
/// * `to_peer_id` - The peer ID up to which the peers are to be returned in
///   the next page.
///
/// # Returns
///
/// An `Option<String>` containing the base64 encoded next token if
/// `from_peer_id` is in range; otherwise, `None`.
pub fn create_localtrust_next_token(
    lt_peers_cnt: u64, from_peer_id: u64, to_peer_id: u64,
) -> Option<String> {
    if (from_peer_id == lt_peers_cnt) || (from_peer_id == 0 && to_peer_id == 0) {
        None
    } else {
        let id_bytes = [from_peer_id.to_be_bytes(), to_peer_id.to_be_bytes()].concat();
        let next_token = BASE64_STANDARD.encode(id_bytes);
        Some(next_token)
    }
}

/// Computes the range of seed trust peers for pagination.
///
/// This function calculates the start and end indices for a range of peers
/// based on the provided `next_token` and `page_size`. The `next_token` is
/// expected to be a base64 encoded string representing an 8-byte integer,
/// which determines the starting peer index (`start_peer`). If `next_token`
/// is `None`, the starting index defaults to 0.
///
/// The `end_peer` is calculated by adding the `page_size` (defaulting to 1000
/// if not provided) to the `start_peer`. The `end_peer` is capped at
/// `st_peers_cnt` to ensure it does not exceed the total number of peers.
///
/// # Arguments
///
/// * `st_peers_cnt` - The total number of seed trust peers.
/// * `page_size` - Optional size of the page, determining the number of peers
///   in the range.
/// * `next_token` - Optional base64 encoded string used to determine the
///   starting peer index.
///
/// # Returns
///
/// A `Result` containing a tuple with the start and end indices of the peer
/// range, or a `BaseRunnerError` if decoding the `next_token` fails.
pub fn compute_seedtrust_peer_range(
    st_peers_cnt: u64, page_size: Option<usize>, next_token: Option<String>,
) -> Result<(u64, u64), BaseRunnerError> {
    let page_size = if let Some(page_size) = page_size { page_size as u64 } else { st_peers_cnt };

    let start_peer = match next_token {
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
    let start_peer = std::cmp::min(start_peer, st_peers_cnt);

    let end_peer = start_peer + page_size;
    let end_peer = std::cmp::min(end_peer, st_peers_cnt);

    Ok((start_peer, end_peer))
}

/// Creates a next token for seed trust pagination.
///
/// This function generates a base64 encoded string that represents the `next_peer_id` as an 8-byte integer.
/// The `next_peer_id` must be greater than 0 and less than `st_peers_cnt` to generate a non-empty token.
///
/// # Arguments
///
/// * `st_peers_cnt` - Total number of seed trust peers. It defines the upper limit for `next_peer_id`.
/// * `next_peer_id` - The peer ID for which the next token is to be created.
///
/// # Returns
///
/// An `Option<String>` containing the base64 encoded next token if `next_peer_id` is in range; otherwise, `None`.
pub fn create_seedtrust_next_token(st_peers_cnt: u64, next_peer_id: u64) -> Option<String> {
    if next_peer_id == 0 || next_peer_id == st_peers_cnt {
        None
    } else {
        let id_bytes = next_peer_id.to_be_bytes();
        let next_token = BASE64_STANDARD.encode(id_bytes);
        Some(next_token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_localtrust_peer_range() {
        // lt: 100 * 100, page_size: 100 * 100, next_token: None
        // (0, 0) => (100, 0)
        let (from_peer_start, from_peer_end, to_peer_start, to_peer_end) =
            compute_localtrust_peer_range(100, None, None).unwrap();
        assert_eq!(from_peer_start, 0);
        assert_eq!(to_peer_start, 0);
        assert_eq!(from_peer_end, 100);
        assert_eq!(to_peer_end, 0);

        // lt: 100 * 100, page_size: 10, next_token: None
        // (0, 0) => (0, 10)
        let (from_peer_start, from_peer_end, to_peer_start, to_peer_end) =
            compute_localtrust_peer_range(100, Some(10), None).unwrap();
        assert_eq!(from_peer_start, 0);
        assert_eq!(to_peer_start, 0);
        assert_eq!(from_peer_end, 0);
        assert_eq!(to_peer_end, 10);

        // lt: 100 * 100, page_size: 250, next_token: None
        // (0, 0) => (2, 50)
        let (from_peer_start, from_peer_end, to_peer_start, to_peer_end) =
            compute_localtrust_peer_range(100, Some(250), None).unwrap();
        assert_eq!(from_peer_start, 0);
        assert_eq!(to_peer_start, 0);
        assert_eq!(from_peer_end, 2);
        assert_eq!(to_peer_end, 50);

        // lt: 100 * 100, page_size: 100 * 100, next_token: "AAAAAAAAAAAAAAAAAAAACg=="
        // (0, 10) => (100, 10)
        let (from_peer_start, from_peer_end, to_peer_start, to_peer_end) =
            compute_localtrust_peer_range(100, None, Some("AAAAAAAAAAAAAAAAAAAACg==".to_string()))
                .unwrap();
        assert_eq!(from_peer_start, 0);
        assert_eq!(to_peer_start, 10);
        assert_eq!(from_peer_end, 100);
        assert_eq!(to_peer_end, 10);

        // lt: 100 * 100, page_size: 100 * 100, next_token: "AAAAAAAAAGMAAAAAAAAAWg=="
        // (99, 90) => (100, 90)
        let (from_peer_start, from_peer_end, to_peer_start, to_peer_end) =
            compute_localtrust_peer_range(100, None, Some("AAAAAAAAAGMAAAAAAAAAWg==".to_string()))
                .unwrap();
        assert_eq!(from_peer_start, 99);
        assert_eq!(to_peer_start, 90);
        assert_eq!(from_peer_end, 100);
        assert_eq!(to_peer_end, 90);
    }

    #[test]
    fn test_create_localtrust_next_token() {
        // lt: 100 * 100, from_peer_id: 0, to_peer_id: 10
        let next_token = create_localtrust_next_token(100, 0, 10);
        assert_eq!(next_token, Some("AAAAAAAAAAAAAAAAAAAACg==".to_string()));

        // lt: 100 * 100, from_peer_id: 99, to_peer_id: 90
        let next_token = create_localtrust_next_token(100, 99, 90);
        assert_eq!(next_token, Some("AAAAAAAAAGMAAAAAAAAAWg==".to_string()));

        // lt: 100 * 100, from_peer_id: 100, to_peer_id: 0
        let next_token = create_localtrust_next_token(100, 100, 0);
        assert_eq!(next_token, None);

        // lt: 100 * 100, from_peer_id: 0, to_peer_id: 0
        let next_token = create_localtrust_next_token(100, 0, 0);
        assert_eq!(next_token, None);
    }

    #[test]
    fn test_compute_seedtrust_peer_range() {
        // st: 100, page_size: 100, next_token: None
        // 0 => 100
        let (from_peer_start, from_peer_end) =
            compute_seedtrust_peer_range(100, None, None).unwrap();
        assert_eq!(from_peer_start, 0);
        assert_eq!(from_peer_end, 100);

        // st: 100, page_size: 55, next_token: None
        // 0 => 55
        let (from_peer_start, from_peer_end) =
            compute_seedtrust_peer_range(100, Some(55), None).unwrap();
        assert_eq!(from_peer_start, 0);
        assert_eq!(from_peer_end, 55);

        // st: 100, page_size: 100, next_token: "AAAAAAAAADc="
        // 55 => 100
        let (from_peer_start, from_peer_end) =
            compute_seedtrust_peer_range(100, Some(100), Some("AAAAAAAAADc=".to_string())).unwrap();
        assert_eq!(from_peer_start, 55);
        assert_eq!(from_peer_end, 100);

        // st: 100, page_size: 10, next_token: "AAAAAAAAADc="
        // 55 => 65
        let (from_peer_start, from_peer_end) =
            compute_seedtrust_peer_range(100, Some(10), Some("AAAAAAAAADc=".to_string())).unwrap();
        assert_eq!(from_peer_start, 55);
        assert_eq!(from_peer_end, 65);
    }

    #[test]
    fn test_create_seedtrust_next_token() {
        // st: 100, next_peer_id: 0
        let next_token = create_seedtrust_next_token(100, 0);
        assert_eq!(next_token, None);

        // st:100 next_peer_id: 100
        let next_token = create_seedtrust_next_token(100, 100);
        assert_eq!(next_token, None);

        // st: 100, next_peer_id: 55
        let next_token = create_seedtrust_next_token(100, 55);
        assert_eq!(next_token, Some("AAAAAAAAADc=".to_string()));
    }
}
