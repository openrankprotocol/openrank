use alloy_rlp::{encode, Decodable};
use jsonrpsee::core::async_trait;
use jsonrpsee::proc_macros::rpc;
use jsonrpsee::types::error::{INVALID_REQUEST_CODE, PARSE_ERROR_CODE};
use jsonrpsee::types::{ErrorCode, ErrorObjectOwned};
use openrank_common::db::Db;
use openrank_common::result::{GetResultsQuery, JobResult};
use openrank_common::txs::{
    Address, CreateCommitment, CreateScores, JobRunRequest, JobVerification, ScoreEntry,
    SeedUpdate, TrustUpdate, Tx, TxKind,
};
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

    pub fn decode_tx<D: Decodable>(
        &self, tx_str: String, kind: TxKind,
    ) -> Result<(Vec<u8>, D), ErrorObjectOwned> {
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
        if tx.kind() != kind {
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
        let body = D::decode(&mut tx.body().as_slice()).map_err(|e| {
            ErrorObjectOwned::owned(
                PARSE_ERROR_CODE,
                "Failed to parse TX data".to_string(),
                Some(e.to_string()),
            )
        })?;

        Ok((tx_bytes, body))
    }
}

#[async_trait]
impl RpcServer for SequencerServer {
    /// Handles incoming `TrustUpdate` transactions from the network,
    /// and forward them to the network for processing.
    async fn trust_update(&self, tx_str: String) -> Result<TxEvent, ErrorObjectOwned> {
        let (tx_bytes, body) = self.decode_tx::<TrustUpdate>(tx_str, TxKind::TrustUpdate)?;

        // Build Tx Event
        // TODO: Replace with DA call
        let tx_event = TxEvent::default_with_data(tx_bytes);
        let channel_message = (
            encode(tx_event.clone()),
            Topic::NamespaceTrustUpdate(body.trust_id),
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
        let (tx_bytes, body) = self.decode_tx::<SeedUpdate>(tx_str, TxKind::SeedUpdate)?;

        // Build Tx Event
        // TODO: Replace with DA call
        let tx_event = TxEvent::default_with_data(tx_bytes);
        let channel_message = (
            encode(tx_event.clone()),
            Topic::NamespaceSeedUpdate(body.seed_id),
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
        let (tx_bytes, body) = self.decode_tx::<JobRunRequest>(tx_str, TxKind::JobRunRequest)?;

        // Build Tx Event
        // TODO: Replace with DA call
        let tx_event = TxEvent::default_with_data(tx_bytes);
        let channel_message = (
            encode(tx_event.clone()),
            Topic::DomainRequest(body.domain_id),
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

        let key = JobResult::construct_full_key(query.job_run_request_tx_hash);
        let result = self.db.get::<JobResult>(key).map_err(|e| {
            error!("{}", e);
            ErrorObjectOwned::from(ErrorCode::InternalError)
        })?;
        let key =
            Tx::construct_full_key(TxKind::CreateCommitment, result.create_commitment_tx_hash);
        let tx = self.db.get::<Tx>(key).map_err(|e| {
            error!("{}", e);
            ErrorObjectOwned::from(ErrorCode::InternalError)
        })?;
        let commitment = CreateCommitment::decode(&mut tx.body().as_slice()).map_err(|e| {
            error!("{}", e);
            ErrorObjectOwned::from(ErrorCode::InternalError)
        })?;
        let create_scores_tx: Vec<Tx> = {
            let mut create_scores_tx = Vec::new();
            for tx_hash in commitment.scores_tx_hashes.into_iter() {
                let key = Tx::construct_full_key(TxKind::CreateScores, tx_hash);
                let tx = self.db.get::<Tx>(key).map_err(|e| {
                    error!("{}", e);
                    ErrorObjectOwned::from(ErrorCode::InternalError)
                })?;
                create_scores_tx.push(tx);
            }
            create_scores_tx
        };
        let create_scores: Vec<CreateScores> = {
            let mut create_scores = Vec::new();
            for tx in create_scores_tx.into_iter() {
                create_scores.push(CreateScores::decode(&mut tx.body().as_slice()).map_err(
                    |e| {
                        error!("{}", e);
                        ErrorObjectOwned::from(ErrorCode::InternalError)
                    },
                )?);
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
            for tx_hash in result.job_verification_tx_hashes.iter() {
                let key = Tx::construct_full_key(TxKind::JobVerification, tx_hash.clone());
                let tx = self.db.get::<Tx>(key).map_err(|e| {
                    error!("{}", e);
                    ErrorObjectOwned::from(ErrorCode::InternalError)
                })?;
                verification_resutls_tx.push(tx);
            }
            verification_resutls_tx
        };
        let verification_results: Vec<JobVerification> = {
            let mut verification_results = Vec::new();
            for tx in verificarion_results_tx.into_iter() {
                let result = JobVerification::decode(&mut tx.body().as_slice()).map_err(|e| {
                    error!("{}", e);
                    ErrorObjectOwned::from(ErrorCode::InternalError)
                })?;
                verification_results.push(result);
            }
            verification_results
        };
        let verification_results_bools: Vec<bool> =
            verification_results.into_iter().map(|x| x.verification_result).collect();

        Ok((verification_results_bools, score_entries))
    }
}
