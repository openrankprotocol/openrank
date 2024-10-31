use alloy_rlp::{encode, Decodable};
use jsonrpsee::core::async_trait;
use jsonrpsee::proc_macros::rpc;
use jsonrpsee::types::error::{INVALID_REQUEST_CODE, PARSE_ERROR_CODE};
use jsonrpsee::types::{ErrorCode, ErrorObjectOwned};
use openrank_common::db::Db;
use openrank_common::result::GetResultsQuery;
use openrank_common::tx::consts;
use openrank_common::tx::{self, compute, trust::ScoreEntry, Address, Tx};
use openrank_common::{topics::Topic, tx_event::TxEvent};
use std::cmp::Ordering;
use tokio::sync::mpsc::Sender;
use tracing::error;

#[rpc(server, namespace = "sequencer")]
pub trait Rpc {
    #[method(name = "trust_update")]
    async fn trust_update(&self, tx_str: String) -> Result<TxEvent, ErrorObjectOwned>;

    #[method(name = "seed_update")]
    async fn seed_update(&self, tx_str: String) -> Result<TxEvent, ErrorObjectOwned>;

    #[method(name = "compute_request")]
    async fn compute_request(&self, tx_str: String) -> Result<TxEvent, ErrorObjectOwned>;

    #[method(name = "get_results")]
    async fn get_results(
        &self, query: GetResultsQuery,
    ) -> Result<(Vec<bool>, Vec<ScoreEntry>), ErrorObjectOwned>;

    #[method(name = "get_compute_result")]
    async fn get_compute_result(
        &self, seq_number: u64,
    ) -> Result<compute::Result, ErrorObjectOwned>;

    #[method(name = "get_tx")]
    async fn get_tx(&self, kind: String, tx_hash: tx::TxHash) -> Result<Tx, ErrorObjectOwned>;

    #[method(name = "get_txs")]
    async fn get_txs(&self, keys: Vec<(String, tx::TxHash)>) -> Result<Vec<Tx>, ErrorObjectOwned>;
}

/// The Sequencer JsonRPC server. It contains the sender, the whitelisted users, and the database connection.
pub struct SequencerServer {
    sender: Sender<(Vec<u8>, Topic)>,
    whitelisted_users: Vec<Address>,
    db: Db,
}

impl SequencerServer {
    pub fn new(sender: Sender<(Vec<u8>, Topic)>, whitelisted_users: Vec<Address>, db: Db) -> Self {
        Self { sender, whitelisted_users, db }
    }

    pub fn decode_tx(
        &self, tx_str: String, kind: &str,
    ) -> Result<(Vec<u8>, tx::Body), ErrorObjectOwned> {
        let tx_bytes = hex::decode(tx_str).map_err(|e| {
            error!("{}", e);
            ErrorObjectOwned::owned(
                PARSE_ERROR_CODE,
                "Failed to parse TX data".to_string(),
                Some(e.to_string()),
            )
        })?;

        let tx = Tx::decode(&mut tx_bytes.as_slice()).map_err(|e| {
            ErrorObjectOwned::owned(
                PARSE_ERROR_CODE,
                "Failed to parse TX data".to_string(),
                Some(e.to_string()),
            )
        })?;
        if tx.body().prefix() != kind {
            return Err(ErrorObjectOwned::owned(
                INVALID_REQUEST_CODE,
                "Invalid tx kind".to_string(),
                None::<String>,
            ));
        }
        let address = tx.verify().map_err(|e| {
            ErrorObjectOwned::owned(
                INVALID_REQUEST_CODE,
                "Failed to verify TX Signature".to_string(),
                Some(e.to_string()),
            )
        })?;
        if !self.whitelisted_users.contains(&address) {
            return Err(ErrorObjectOwned::owned(
                INVALID_REQUEST_CODE,
                "Invalid TX signer".to_string(),
                None::<String>,
            ));
        }

        Ok((tx_bytes, tx.body()))
    }
}

#[async_trait]
impl RpcServer for SequencerServer {
    /// Handles incoming `TrustUpdate` transactions from the network,
    /// and forward them to the network for processing.
    async fn trust_update(&self, tx_str: String) -> Result<TxEvent, ErrorObjectOwned> {
        let (tx_bytes, body) = self.decode_tx(tx_str, consts::TRUST_UPDATE)?;
        let trust_update = match body {
            tx::Body::TrustUpdate(trust_update) => Ok(trust_update),
            _ => Err(ErrorObjectOwned::from(ErrorCode::InternalError)),
        }?;

        // Build Tx Event
        // TODO: Replace with DA call
        let tx_event = TxEvent::default_with_data(tx_bytes);
        let channel_message = (
            encode(tx_event.clone()),
            Topic::NamespaceTrustUpdate(trust_update.trust_id),
        );
        self.sender.send(channel_message).await.map_err(|e| {
            error!("{}", e);
            ErrorObjectOwned::from(ErrorCode::InternalError)
        })?;
        Ok(tx_event)
    }

    /// Handles incoming `SeedUpdate` transactions from the network,
    /// and forward them to the network node for processing.
    async fn seed_update(&self, tx_str: String) -> Result<TxEvent, ErrorObjectOwned> {
        let (tx_bytes, body) = self.decode_tx(tx_str, consts::SEED_UPDATE)?;
        let seed_update = match body {
            tx::Body::SeedUpdate(seed_update) => Ok(seed_update),
            _ => Err(ErrorObjectOwned::from(ErrorCode::InternalError)),
        }?;

        // Build Tx Event
        // TODO: Replace with DA call
        let tx_event = TxEvent::default_with_data(tx_bytes);
        let channel_message = (
            encode(tx_event.clone()),
            Topic::NamespaceSeedUpdate(seed_update.seed_id),
        );
        self.sender.send(channel_message).await.map_err(|e| {
            error!("{}", e);
            ErrorObjectOwned::from(ErrorCode::InternalError)
        })?;
        Ok(tx_event)
    }

    /// Handles incoming `ComputeRequest` transactions from the network,
    /// and forward them to the network node for processing
    async fn compute_request(&self, tx_str: String) -> Result<TxEvent, ErrorObjectOwned> {
        let (tx_bytes, body) = self.decode_tx(tx_str, consts::COMPUTE_REQUEST)?;
        let compute_request = match body {
            tx::Body::ComputeRequest(compute_request) => Ok(compute_request),
            _ => Err(ErrorObjectOwned::from(ErrorCode::InternalError)),
        }?;

        // Build Tx Event
        // TODO: Replace with DA call
        let tx_event = TxEvent::default_with_data(tx_bytes);
        let channel_message = (
            encode(tx_event.clone()),
            Topic::DomainRequest(compute_request.domain_id),
        );
        self.sender.send(channel_message).await.map_err(|e| {
            error!("{}", e);
            ErrorObjectOwned::from(ErrorCode::InternalError)
        })?;
        Ok(tx_event)
    }

    /// Gets the results(EigenTrust scores) of the `ComputeRequest` with the ComputeRequest TX hash,
    /// along with start and size parameters.
    async fn get_results(
        &self, query: GetResultsQuery,
    ) -> Result<(Vec<bool>, Vec<ScoreEntry>), ErrorObjectOwned> {
        self.db.refresh().map_err(|e| {
            error!("{}", e);
            ErrorObjectOwned::from(ErrorCode::InternalError)
        })?;

        let key = compute::Result::construct_full_key(query.seq_number);
        let result = self.db.get::<compute::Result>(key).map_err(|e| {
            error!("{}", e);
            ErrorObjectOwned::from(ErrorCode::InternalError)
        })?;
        let key = Tx::construct_full_key(
            consts::COMPUTE_COMMITMENT,
            result.compute_commitment_tx_hash,
        );
        let tx = self.db.get::<Tx>(key).map_err(|e| {
            error!("{}", e);
            ErrorObjectOwned::from(ErrorCode::InternalError)
        })?;
        let commitment = match tx.body() {
            tx::Body::ComputeCommitment(commitment) => Ok(commitment),
            _ => Err(ErrorObjectOwned::from(ErrorCode::InternalError)),
        }?;
        let create_scores_tx: Vec<Tx> = {
            let mut create_scores_tx = Vec::new();
            for tx_hash in commitment.scores_tx_hashes.into_iter() {
                let key = Tx::construct_full_key(consts::COMPUTE_SCORES, tx_hash);
                let tx = self.db.get::<Tx>(key).map_err(|e| {
                    error!("{}", e);
                    ErrorObjectOwned::from(ErrorCode::InternalError)
                })?;
                create_scores_tx.push(tx);
            }
            create_scores_tx
        };
        let create_scores: Vec<compute::Scores> = {
            let mut create_scores = Vec::new();
            for tx in create_scores_tx.into_iter() {
                let scores = match tx.body() {
                    tx::Body::ComputeScores(scores) => Ok(scores),
                    _ => Err(ErrorObjectOwned::from(ErrorCode::InternalError)),
                }?;
                create_scores.push(scores);
            }
            create_scores
        };
        let mut score_entries: Vec<ScoreEntry> =
            create_scores.into_iter().flat_map(|x| x.entries).collect();
        score_entries.sort_by(|a, b| match a.value.partial_cmp(&b.value) {
            Some(ordering) => ordering,
            None => {
                if a.value.is_nan() && b.value.is_nan() {
                    Ordering::Equal
                } else if a.value.is_nan() {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            },
        });
        score_entries.reverse();
        let score_entries: Vec<ScoreEntry> = score_entries
            .split_at(query.start as usize)
            .1
            .iter()
            .take(query.size as usize)
            .cloned()
            .collect();

        let verificarion_results_tx: Vec<Tx> = {
            let mut verification_resutls_tx = Vec::new();
            for tx_hash in result.compute_verification_tx_hashes.iter() {
                let key = Tx::construct_full_key(consts::COMPUTE_VERIFICATION, tx_hash.clone());
                let tx = self.db.get::<Tx>(key).map_err(|e| {
                    error!("{}", e);
                    ErrorObjectOwned::from(ErrorCode::InternalError)
                })?;
                verification_resutls_tx.push(tx);
            }
            verification_resutls_tx
        };
        let verification_results: Vec<compute::Verification> = {
            let mut verification_results = Vec::new();
            for tx in verificarion_results_tx.into_iter() {
                let result = match tx.body() {
                    tx::Body::ComputeVerification(result) => Ok(result),
                    _ => Err(ErrorObjectOwned::from(ErrorCode::InternalError)),
                }?;
                verification_results.push(result);
            }
            verification_results
        };
        let verification_results_bools: Vec<bool> =
            verification_results.into_iter().map(|x| x.verification_result).collect();

        Ok((verification_results_bools, score_entries))
    }

    /// Fetch the ComputeResult TX by its sequence number
    async fn get_compute_result(
        &self, seq_number: u64,
    ) -> Result<compute::Result, ErrorObjectOwned> {
        self.db.refresh().map_err(|e| {
            error!("{}", e);
            ErrorObjectOwned::from(ErrorCode::InternalError)
        })?;

        let key = compute::Result::construct_full_key(seq_number);
        let result = self.db.get::<compute::Result>(key).map_err(|e| {
            error!("{}", e);
            ErrorObjectOwned::from(ErrorCode::InternalError)
        })?;

        Ok(result)
    }

    /// Fetch the TX given its `kind` and `tx_hash`
    async fn get_tx(&self, kind: String, tx_hash: tx::TxHash) -> Result<Tx, ErrorObjectOwned> {
        self.db.refresh().map_err(|e| {
            error!("{}", e);
            ErrorObjectOwned::from(ErrorCode::InternalError)
        })?;

        println!("{:?} {:?}", kind, tx_hash);
        let key = Tx::construct_full_key(&kind, tx_hash);
        let tx = self.db.get::<Tx>(key).map_err(|e| {
            error!("{}", e);
            ErrorObjectOwned::from(ErrorCode::InternalError)
        })?;

        Ok(tx)
    }

    /// Fetch multiple TXs given an array of `keys`.
    async fn get_txs(&self, keys: Vec<(String, tx::TxHash)>) -> Result<Vec<Tx>, ErrorObjectOwned> {
        self.db.refresh().map_err(|e| {
            error!("{}", e);
            ErrorObjectOwned::from(ErrorCode::InternalError)
        })?;

        let mut key_bytes = Vec::new();
        for (kind, tx_hash) in keys {
            let full_key = Tx::construct_full_key(&kind, tx_hash);
            key_bytes.push(full_key);
        }
        let txs = self.db.get_multi::<Tx>(key_bytes).map_err(|e| {
            error!("{}", e);
            ErrorObjectOwned::from(ErrorCode::InternalError)
        })?;

        Ok(txs)
    }
}
