use alloy_rlp::{encode, Decodable};
use getset::Getters;
use jsonrpsee::core::async_trait;
use jsonrpsee::proc_macros::rpc;
use jsonrpsee::types::ErrorObjectOwned;
use openrank_common::db::{self, Db};
use openrank_common::tx::consts;
use openrank_common::tx::{self, compute, Address, Tx};
use openrank_common::{topics::Topic, tx_event::TxEvent};
use std::fmt;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tracing::{debug, error};

#[derive(Debug, Clone, Copy)]
pub enum ErrorCode {
    GossipsubFailed = -32012,
    InvalidTxKind = -32013,
    RocksDbFailed = -32014,
    CommonDbFailed = -32015,
    NotFound = -32016,
    ParseFailed = -32017,
    InvalidSignature = -32018,
    NotWhitelisted = -32019,
}
impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            ErrorCode::GossipsubFailed => "Gossipsub failed",
            ErrorCode::InvalidTxKind => "Invalid tx kind",
            ErrorCode::RocksDbFailed => "RocksDB failed",
            ErrorCode::CommonDbFailed => "Common DB failed",
            ErrorCode::NotFound => "Object not found",
            ErrorCode::ParseFailed => "Failed to parse TX data",
            ErrorCode::InvalidSignature => "Failed to verify TX Signature",
            ErrorCode::NotWhitelisted => "TX signer is not whitelisted",
        };
        write!(f, "{}", message)
    }
}
impl ErrorCode {
    pub fn code(&self) -> i32 {
        *self as i32
    }
}

fn to_error_object<T: ToString>(code: ErrorCode, data: Option<T>) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(code.code(), code.to_string(), data.map(|d| d.to_string()))
}

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
            to_error_object(ErrorCode::ParseFailed, Some(e))
        })?;

        let tx = Tx::decode(&mut tx_bytes.as_slice())
            .map_err(|e| to_error_object(ErrorCode::ParseFailed, Some(e)))?;
        if tx.body().prefix() != kind {
            return Err(to_error_object(ErrorCode::InvalidTxKind, None::<String>));
        }
        let address =
            tx.verify().map_err(|e| to_error_object(ErrorCode::InvalidSignature, Some(e)))?;
        if !self.whitelisted_users.contains(&address) {
            return Err(to_error_object(ErrorCode::NotWhitelisted, None::<String>));
        }

        Ok((tx_bytes, tx.body().clone()))
    }

    pub fn map_db_error(e: db::Error) -> ErrorObjectOwned {
        match e {
            db::Error::NotFound => to_error_object(ErrorCode::NotFound, None::<String>),
            db::Error::RocksDB(err) => {
                error!("{}", err);
                to_error_object(ErrorCode::RocksDbFailed, Some(err))
            },
            err => {
                error!("{}", err);
                to_error_object(ErrorCode::CommonDbFailed, Some(err))
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
            _ => Err(to_error_object(ErrorCode::InvalidTxKind, None::<String>)),
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
            to_error_object(ErrorCode::GossipsubFailed, None::<String>)
        })?;
        Ok(tx_event)
    }

    /// Handles incoming `SeedUpdate` transactions from the network,
    /// and forward them to the network node for processing.
    async fn seed_update(&self, tx_str: String) -> Result<TxEvent, ErrorObjectOwned> {
        let (tx_bytes, body) = self.decode_tx(tx_str, consts::SEED_UPDATE)?;
        let seed_update = match body {
            tx::Body::SeedUpdate(seed_update) => Ok(seed_update),
            _ => Err(to_error_object(ErrorCode::InvalidTxKind, None::<String>)),
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
            to_error_object(ErrorCode::GossipsubFailed, None::<String>)
        })?;
        Ok(tx_event)
    }

    /// Handles incoming `ComputeRequest` transactions from the network,
    /// and forward them to the network node for processing
    async fn compute_request(&self, tx_str: String) -> Result<TxEvent, ErrorObjectOwned> {
        let (tx_bytes, body) = self.decode_tx(tx_str, consts::COMPUTE_REQUEST)?;
        let compute_request = match body {
            tx::Body::ComputeRequest(compute_request) => Ok(compute_request),
            _ => Err(to_error_object(ErrorCode::GossipsubFailed, None::<String>)),
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
            to_error_object(ErrorCode::GossipsubFailed, None::<String>)
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
