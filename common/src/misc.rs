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

pub fn decode_localtrust_next_token(
    next_token: Option<String>,
) -> Result<(u64, u64), BaseRunnerError> {
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
    Ok((from_peer_start, to_peer_start))
}

pub fn encode_localtrust_next_token(from_peer_id: u64, to_peer_id: u64) -> Option<String> {
    if from_peer_id == 0 && to_peer_id == 0 {
        None
    } else {
        let to_peer_id = to_peer_id + 1;
        let id_bytes = [from_peer_id.to_be_bytes(), to_peer_id.to_be_bytes()].concat();
        let next_token = BASE64_STANDARD.encode(id_bytes);
        Some(next_token)
    }
}

pub fn decode_seedtrust_next_token(next_token: Option<String>) -> Result<u64, BaseRunnerError> {
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
    Ok(start_peer)
}

pub fn encode_seedtrust_next_token(next_peer_id: u64) -> Option<String> {
    if next_peer_id == 0 {
        None
    } else {
        let id_bytes = next_peer_id.to_be_bytes();
        let next_token = BASE64_STANDARD.encode(id_bytes);
        Some(next_token)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_decode_localtrust_next_token() {
        // next_token: None
        // (0, 0)
        let (from_peer_start, to_peer_start) = decode_localtrust_next_token(None).unwrap();
        assert_eq!(from_peer_start, 0);
        assert_eq!(to_peer_start, 0);

        // next_token: "AAAAAAAAAAAAAAAAAAAACg=="
        // (0, 10)
        let (from_peer_start, to_peer_start) =
            decode_localtrust_next_token(Some("AAAAAAAAAAAAAAAAAAAACg==".to_string())).unwrap();
        assert_eq!(from_peer_start, 0);
        assert_eq!(to_peer_start, 10);

        // next_token: "AAAAAAAAAGMAAAAAAAAAWg=="
        // (99, 90)
        let (from_peer_start, to_peer_start) =
            decode_localtrust_next_token(Some("AAAAAAAAAGMAAAAAAAAAWg==".to_string())).unwrap();
        assert_eq!(from_peer_start, 99);
        assert_eq!(to_peer_start, 90);
    }

    #[test]
    fn test_encode_localtrust_next_token() {
        // from_peer_id: 0, to_peer_id: 0
        let next_token = encode_localtrust_next_token(0, 0);
        assert_eq!(next_token, None);

        // from_peer_id: 0, to_peer_id: 10
        let next_token = encode_localtrust_next_token(0, 10);
        assert_eq!(next_token, Some("AAAAAAAAAAAAAAAAAAAACw==".to_string()));

        // from_peer_id: 99, to_peer_id: 90
        let next_token = encode_localtrust_next_token(99, 90);
        assert_eq!(next_token, Some("AAAAAAAAAGMAAAAAAAAAWw==".to_string()));
    }

    #[test]
    fn test_decode_seedtrust_next_token() {
        // next_token: None
        // 0
        let from_peer_start = decode_seedtrust_next_token(None).unwrap();
        assert_eq!(from_peer_start, 0);

        // next_token: "AAAAAAAAADc="
        // 55
        let from_peer_start =
            decode_seedtrust_next_token(Some("AAAAAAAAADc=".to_string())).unwrap();
        assert_eq!(from_peer_start, 55);
    }

    #[test]
    fn test_encode_seedtrust_next_token() {
        // next_peer_id: 0
        let next_token = encode_seedtrust_next_token(0);
        assert_eq!(next_token, None);

        // next_peer_id: 55
        let next_token = encode_seedtrust_next_token(55);
        assert_eq!(next_token, Some("AAAAAAAAADc=".to_string()));
    }
}
