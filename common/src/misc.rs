use base64::prelude::*;
use getset::Getters;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::runners::Error as BaseRunnerError;

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
    result: Vec<(u64, u64, f32)>,
    next_token: Option<String>,
}

impl LocalTrustStateResponse {
    pub fn new(result: Vec<(u64, u64, f32)>, next_token: Option<String>) -> Self {
        Self { result, next_token }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct SeedTrustStateResponse {
    result: Vec<(u64, f32)>,
    next_token: Option<String>,
}

impl SeedTrustStateResponse {
    pub fn new(result: Vec<(u64, f32)>, next_token: Option<String>) -> Self {
        Self { result, next_token }
    }
}

pub fn compute_seedtrust_peer_range(
    st_peers_cnt: usize,
    page_size: Option<usize>, 
    next_token: Option<String>,
) -> Result<(u64, u64), BaseRunnerError> {
    let page_size = page_size.unwrap_or(1000);
    let start_peer = match next_token {
        Some(token) => {
            let decoded_bytes = BASE64_STANDARD.decode(token).map_err(BaseRunnerError::Base64Decode)?;
            let id_bytes = TryInto::<[u8; 8]>::try_into(decoded_bytes).map_err(|e| BaseRunnerError::Misc(format!("Failed to convert to 8 bytes: {:?}", e)))?;
            u64::from_be_bytes(id_bytes)
        },
        None => 0
    };
    let end_peer = start_peer + page_size as u64;
    let end_peer = std::cmp::min(end_peer, st_peers_cnt as u64);
    Ok((start_peer, end_peer))
}

pub fn create_seedtrust_next_token(st_peers_cnt: usize, next_peer_id: u64) -> Option<String> {
    if 0 < next_peer_id && next_peer_id < st_peers_cnt as u64 {
        let id_bytes = next_peer_id.to_be_bytes();
        let next_token = BASE64_STANDARD.encode(id_bytes);
        Some(next_token)
    } else {
        None
    }
}
