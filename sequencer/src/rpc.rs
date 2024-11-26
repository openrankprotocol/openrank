use alloy_rlp::{encode, Decodable};
use getset::Getters;
use jsonrpsee::core::async_trait;
use jsonrpsee::proc_macros::rpc;
use jsonrpsee::types::ErrorObjectOwned;
use openrank_common::db::{self, Db};
use openrank_common::tx::consts;
use openrank_common::tx::{self, compute, Address, Tx};
use openrank_common::{topics::Topic, tx_event::TxEvent};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tracing::{debug, error};

const GOSSIPSUB_FAILED_CODE: i32 = -32012;
const INVALID_TX_KIND_CODE: i32 = -32013;
const ROCKS_DB_FAILED_CODE: i32 = -32014;
const COMMON_DB_FAILED_CODE: i32 = -32015;
const NOT_FOUND_CODE: i32 = -32016;
const PARSE_FAILED_CODE: i32 = -32017;
const INVALID_SIGNATURE_CODE: i32 = -32018;
const NOT_WHITELISTED_CODE: i32 = -32019;

const GOSSIPSUB_FAILED_MESSAGE: &str = "Gossipsub failed";
const INVALID_TX_KIND_MESSAGE: &str = "Invalid tx kind";
const ROCKS_DB_FAILED_MESSAGE: &str = "RocksDB failed";
const COMMON_DB_FAILED_MESSAGE: &str = "Common DB failed";
const NOT_FOUND_MESSAGE: &str = "Object not found";
const PARSE_FAILED_MESSAGE: &str = "Failed to parse TX data";
const INVALID_SIGNATURE_MESSAGE: &str = "Failed to verify TX Signature";
const NOT_WHITELISTED_MESSAGE: &str = "TX signer is not whitelisted";

#[rpc(server, namespace = "sequencer")]
pub trait Rpc {
    #[method(name = "trust_update")]
    async fn trust_update(&self, tx_str: String) -> Result<TxEvent, ErrorObjectOwned>;

    #[method(name = "seed_update")]
    async fn seed_update(&self, tx_str: String) -> Result<TxEvent, ErrorObjectOwned>;

    #[method(name = "compute_request")]
    async fn compute_request(&self, tx_str: String) -> Result<TxEvent, ErrorObjectOwned>;

    #[method(name = "get_compute_result_seq_number")]
    async fn get_compute_result_seq_number(
        &self, request_tx_hash: tx::TxHash,
    ) -> Result<u64, ErrorObjectOwned>;

    #[method(name = "get_compute_result")]
    async fn get_compute_result(
        &self, seq_number: u64,
    ) -> Result<compute::Result, ErrorObjectOwned>;

    #[method(name = "get_tx")]
    async fn get_tx(&self, kind: String, tx_hash: tx::TxHash) -> Result<Tx, ErrorObjectOwned>;

    #[method(name = "get_txs")]
    async fn get_txs(&self, keys: Vec<(String, tx::TxHash)>) -> Result<Vec<Tx>, ErrorObjectOwned>;
}

#[derive(Getters)]
#[getset(get = "pub")]
/// The Sequencer JsonRPC server. It contains the sender, the whitelisted users, and the database connection.
pub struct SequencerServer {
    sender: Sender<(Vec<u8>, Topic)>,
    whitelisted_users: Vec<Address>,
    db: Arc<Db>,
}

impl SequencerServer {
    pub fn new(
        sender: Sender<(Vec<u8>, Topic)>, whitelisted_users: Vec<Address>, db: Arc<Db>,
    ) -> Self {
        Self { sender, whitelisted_users, db }
    }

    pub fn decode_tx(
        &self, tx_str: String, kind: &str,
    ) -> Result<(Vec<u8>, tx::Body), ErrorObjectOwned> {
        let tx_bytes = hex::decode(tx_str).map_err(|e| {
            debug!("{}", e);
            ErrorObjectOwned::owned(
                PARSE_FAILED_CODE,
                PARSE_FAILED_MESSAGE.to_string(),
                Some(e.to_string()),
            )
        })?;

        let tx = Tx::decode(&mut tx_bytes.as_slice()).map_err(|e| {
            ErrorObjectOwned::owned(
                PARSE_FAILED_CODE,
                PARSE_FAILED_MESSAGE.to_string(),
                Some(e.to_string()),
            )
        })?;
        if tx.body().prefix() != kind {
            return Err(ErrorObjectOwned::owned(
                INVALID_TX_KIND_CODE,
                INVALID_TX_KIND_MESSAGE.to_string(),
                None::<String>,
            ));
        }
        let address = tx.verify().map_err(|e| {
            ErrorObjectOwned::owned(
                INVALID_SIGNATURE_CODE,
                INVALID_SIGNATURE_MESSAGE.to_string(),
                Some(e.to_string()),
            )
        })?;
        if !self.whitelisted_users.contains(&address) {
            return Err(ErrorObjectOwned::owned(
                NOT_WHITELISTED_CODE,
                NOT_WHITELISTED_MESSAGE.to_string(),
                None::<String>,
            ));
        }

        Ok((tx_bytes, tx.body().clone()))
    }

    pub fn map_db_error(e: db::Error) -> ErrorObjectOwned {
        match e {
            db::Error::NotFound => ErrorObjectOwned::owned(
                NOT_FOUND_CODE,
                NOT_FOUND_MESSAGE.to_string(),
                None::<String>,
            ),
            db::Error::RocksDB(err) => {
                error!("{}", err);
                ErrorObjectOwned::owned(
                    ROCKS_DB_FAILED_CODE,
                    ROCKS_DB_FAILED_MESSAGE.to_string(),
                    Some(err.to_string()),
                )
            },
            err => {
                error!("{}", err);
                ErrorObjectOwned::owned(
                    COMMON_DB_FAILED_CODE,
                    COMMON_DB_FAILED_MESSAGE.to_string(),
                    Some(err.to_string()),
                )
            },
        }
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
            _ => Err(ErrorObjectOwned::owned(
                INVALID_TX_KIND_CODE,
                INVALID_TX_KIND_MESSAGE.to_string(),
                None::<String>,
            )),
        }?;

        // Build Tx Event
        // TODO: Replace with DA call
        let tx_event = TxEvent::default_with_data(tx_bytes);
        let channel_message = (
            encode(tx_event.clone()),
            Topic::NamespaceTrustUpdate(trust_update.trust_id().clone()),
        );
        self.sender.send(channel_message).await.map_err(|e| {
            error!("{}", e);
            ErrorObjectOwned::owned(
                GOSSIPSUB_FAILED_CODE,
                GOSSIPSUB_FAILED_MESSAGE.to_string(),
                None::<String>,
            )
        })?;
        Ok(tx_event)
    }

    /// Handles incoming `SeedUpdate` transactions from the network,
    /// and forward them to the network node for processing.
    async fn seed_update(&self, tx_str: String) -> Result<TxEvent, ErrorObjectOwned> {
        let (tx_bytes, body) = self.decode_tx(tx_str, consts::SEED_UPDATE)?;
        let seed_update = match body {
            tx::Body::SeedUpdate(seed_update) => Ok(seed_update),
            _ => Err(ErrorObjectOwned::owned(
                INVALID_TX_KIND_CODE,
                INVALID_TX_KIND_MESSAGE.to_string(),
                None::<String>,
            )),
        }?;

        // Build Tx Event
        // TODO: Replace with DA call
        let tx_event = TxEvent::default_with_data(tx_bytes);
        let channel_message = (
            encode(tx_event.clone()),
            Topic::NamespaceSeedUpdate(seed_update.seed_id().clone()),
        );
        self.sender.send(channel_message).await.map_err(|e| {
            error!("{}", e);
            ErrorObjectOwned::owned(
                GOSSIPSUB_FAILED_CODE,
                GOSSIPSUB_FAILED_MESSAGE.to_string(),
                None::<String>,
            )
        })?;
        Ok(tx_event)
    }

    /// Handles incoming `ComputeRequest` transactions from the network,
    /// and forward them to the network node for processing
    async fn compute_request(&self, tx_str: String) -> Result<TxEvent, ErrorObjectOwned> {
        let (tx_bytes, body) = self.decode_tx(tx_str, consts::COMPUTE_REQUEST)?;
        let compute_request = match body {
            tx::Body::ComputeRequest(compute_request) => Ok(compute_request),
            _ => Err(ErrorObjectOwned::owned(
                INVALID_TX_KIND_CODE,
                INVALID_TX_KIND_MESSAGE.to_string(),
                None::<String>,
            )),
        }?;

        // Build Tx Event
        // TODO: Replace with DA call
        let tx_event = TxEvent::default_with_data(tx_bytes);
        let channel_message = (
            encode(tx_event.clone()),
            Topic::DomainRequest(*compute_request.domain_id()),
        );
        self.sender.send(channel_message).await.map_err(|e| {
            error!("{}", e);
            ErrorObjectOwned::owned(
                GOSSIPSUB_FAILED_CODE,
                GOSSIPSUB_FAILED_MESSAGE.to_string(),
                None::<String>,
            )
        })?;
        Ok(tx_event)
    }

    /// Fetch the ComputeResult sequence number, given the request TX hash
    async fn get_compute_result_seq_number(
        &self, request_tx_hash: tx::TxHash,
    ) -> Result<u64, ErrorObjectOwned> {
        let db_handler = self.db.clone();

        let result = db_handler
            .get::<compute::ResultReference>(request_tx_hash.to_bytes())
            .map_err(SequencerServer::map_db_error)?;

        Ok(*result.seq_number())
    }

    /// Fetch the ComputeResult TX by its sequence number
    async fn get_compute_result(
        &self, seq_number: u64,
    ) -> Result<compute::Result, ErrorObjectOwned> {
        let db_handler = self.db.clone();

        let key = compute::Result::construct_full_key(seq_number);
        let result =
            db_handler.get::<compute::Result>(key).map_err(SequencerServer::map_db_error)?;

        Ok(result)
    }

    /// Fetch the TX given its `kind` and `tx_hash`
    async fn get_tx(&self, kind: String, tx_hash: tx::TxHash) -> Result<Tx, ErrorObjectOwned> {
        let db_handler = self.db.clone();

        let key = Tx::construct_full_key(&kind, tx_hash);
        let tx = db_handler.get::<Tx>(key).map_err(SequencerServer::map_db_error)?;

        Ok(tx)
    }

    /// Fetch multiple TXs given an array of `keys`.
    async fn get_txs(&self, keys: Vec<(String, tx::TxHash)>) -> Result<Vec<Tx>, ErrorObjectOwned> {
        let db_handler = self.db.clone();

        let mut key_bytes = Vec::new();
        for (kind, tx_hash) in keys {
            let full_key = Tx::construct_full_key(&kind, tx_hash);
            key_bytes.push(full_key);
        }
        let txs = db_handler.get_multi::<Tx>(key_bytes).map_err(SequencerServer::map_db_error)?;

        Ok(txs)
    }
}
