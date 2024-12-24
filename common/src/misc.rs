use getset::Getters;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Local trust object.
///
/// The local trust object stores the trust values that a node assigns to its
/// peers.
///
/// It also stores the sum of the trust values assigned to all peers.
#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct SingleLT {
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

impl Default for SingleLT {
    fn default() -> Self {
        Self::new()
    }
}

impl SingleLT {
    pub fn new() -> Self {
        Self { outbound_trust_scores: HashMap::new(), outbound_sum: 0.0 }
    }

    pub fn set_outbound_trust_scores(&mut self, outbound_trust_scores: HashMap<u64, f32>) {
        self.outbound_trust_scores = outbound_trust_scores;
    }

    pub fn set_outbound_sum(&mut self, outbound_sum: f32) {
        self.outbound_sum = outbound_sum;
    }

    pub fn from_score_map(score_map: &HashMap<u64, f32>) -> Self {
        let outbound_trust_scores = score_map.clone();
        let outbound_sum = outbound_trust_scores.values().sum();
        Self { outbound_trust_scores, outbound_sum }
    }

    pub fn normalize(&mut self) {
        for (_, value) in self.outbound_trust_scores.iter_mut() {
            *value /= self.outbound_sum;
        }
        self.outbound_sum = 1.0;
    }

    /*----------------- HashMap similar utils -----------------*/
    pub fn get(&self, peer_id: &u64) -> Option<f32> {
        self.outbound_trust_scores.get(&peer_id).copied()
    }

    pub fn contains_key(&self, peer_id: &u64) -> bool {
        self.outbound_trust_scores.contains_key(peer_id)
    }

    pub fn remove(&mut self, peer_id: &u64) {
        self.outbound_trust_scores.remove(peer_id);
        self.outbound_sum = self.outbound_trust_scores.values().sum();
    }

    pub fn insert(&mut self, peer_id: u64, value: f32) {
        self.outbound_trust_scores.insert(peer_id, value);
        self.outbound_sum = self.outbound_trust_scores.values().sum();
    }
}
