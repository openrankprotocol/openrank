use alloy_rlp::{encode, Decodable};
use karyon_jsonrpc::{rpc_impl, RPCError};
use openrank_common::{
	db::{Db, DbItem},
	result::JobResult,
	topics::Topic,
	tx_event::TxEvent,
	txs::{
		Address, CreateCommitment, CreateScores, JobRunRequest, JobVerification, ScoreEntry,
		SeedUpdate, TrustUpdate, Tx, TxHash, TxKind,
	},
};
use serde_json::Value;
use tokio::sync::mpsc::Sender;
use tracing::error;

pub struct Sequencer {
	sender: Sender<(Vec<u8>, Topic)>,
	whitelisted_users: Vec<Address>,
}

impl Sequencer {
	pub fn new(sender: Sender<(Vec<u8>, Topic)>, whitelisted_users: Vec<Address>) -> Self {
		Self { sender, whitelisted_users }
	}
}

#[rpc_impl]
impl Sequencer {
	async fn trust_update(&self, tx: Value) -> Result<Value, RPCError> {
		let tx_bytes = hex::decode(tx.as_str().unwrap()).map_err(|e| {
			error!("{}", e);
			RPCError::ParseError("Failed to parse TX data".to_string())
		})?;

		let tx = Tx::decode(&mut tx_bytes.as_slice())
			.map_err(|_| RPCError::ParseError("Failed to parse TX data".to_string()))?;
		if tx.kind() != TxKind::TrustUpdate {
			return Err(RPCError::InvalidRequest("Invalid tx kind"));
		}
		let (res, address) = tx.verify();
		if !res || !self.whitelisted_users.contains(&address) {
			return Err(RPCError::InvalidRequest("Invalid tx signature"));
		}
		let body: TrustUpdate = TrustUpdate::decode(&mut tx.body().as_slice())
			.map_err(|_| RPCError::ParseError("Failed to parse TX data".to_string()))?;

		// Build Tx Event
		// TODO: Replace with DA call
		let tx_event = TxEvent::default_with_data(tx_bytes);
		let channel_message = (
			encode(tx_event.clone()),
			Topic::NamespaceTrustUpdate(body.trust_id),
		);
		self.sender.send(channel_message).await.map_err(|_| RPCError::InternalError)?;

		let tx_event_value = serde_json::to_value(tx_event)?;
		Ok(tx_event_value)
	}

	async fn seed_update(&self, tx: Value) -> Result<Value, RPCError> {
		let tx_bytes = hex::decode(tx.as_str().unwrap())
			.map_err(|_| RPCError::ParseError("Failed to parse TX data".to_string()))?;

		let tx = Tx::decode(&mut tx_bytes.as_slice())
			.map_err(|_| RPCError::ParseError("Failed to parse TX data".to_string()))?;
		if tx.kind() != TxKind::SeedUpdate {
			return Err(RPCError::InvalidRequest("Invalid tx kind"));
		}
		let (res, address) = tx.verify();
		if !res || !self.whitelisted_users.contains(&address) {
			return Err(RPCError::InvalidRequest("Invalid tx signature"));
		}
		let body = SeedUpdate::decode(&mut tx.body().as_slice())
			.map_err(|_| RPCError::ParseError("Failed to parse TX data".to_string()))?;

		// Build Tx Event
		// TODO: Replace with DA call
		let tx_event = TxEvent::default_with_data(tx_bytes);
		let channel_message = (
			encode(tx_event.clone()),
			Topic::NamespaceSeedUpdate(body.seed_id),
		);
		self.sender.send(channel_message).await.map_err(|_| RPCError::InternalError)?;

		let tx_event_value = serde_json::to_value(tx_event)?;
		Ok(tx_event_value)
	}

	async fn job_run_request(&self, tx: Value) -> Result<Value, RPCError> {
		let tx_bytes = hex::decode(tx.as_str().unwrap()).map_err(|e| {
			error!("{}", e);
			RPCError::ParseError("Failed to parse TX data".to_string())
		})?;

		let tx = Tx::decode(&mut tx_bytes.as_slice()).map_err(|e| {
			error!("{}", e);
			RPCError::ParseError("Failed to parse TX data".to_string())
		})?;
		if tx.kind() != TxKind::JobRunRequest {
			return Err(RPCError::InvalidRequest("Invalid tx kind"));
		}
		let (res, address) = tx.verify();
		if !res || !self.whitelisted_users.contains(&address) {
			return Err(RPCError::InvalidRequest("Invalid tx signature"));
		}
		let body = JobRunRequest::decode(&mut tx.body().as_slice())
			.map_err(|_| RPCError::ParseError("Failed to parse TX data".to_string()))?;

		// Build Tx Event
		// TODO: Replace with DA call
		let tx_event = TxEvent::default_with_data(tx_bytes);
		let channel_message = (
			encode(tx_event.clone()),
			Topic::DomainRequest(body.domain_id),
		);
		self.sender.send(channel_message).await.map_err(|_| RPCError::InternalError)?;

		let tx_event_value = serde_json::to_value(tx_event)?;
		Ok(tx_event_value)
	}

	async fn get_results(&self, job_run_tx_hash: Value) -> Result<Value, RPCError> {
		let tx_hash_bytes = hex::decode(job_run_tx_hash.as_str().unwrap()).map_err(|e| {
			error!("{}", e);
			RPCError::ParseError("Failed to parse TX data".to_string())
		})?;
		let tx_hash = TxHash::from_bytes(tx_hash_bytes);
		let db = Db::new_read_only("./local-storage", &[&Tx::get_cf(), &JobResult::get_cf()])
			.map_err(|e| {
				error!("{}", e);
				RPCError::InternalError
			})?;

		let key = JobResult::construct_full_key(tx_hash);
		let result = db.get::<JobResult>(key).unwrap();
		let key =
			Tx::construct_full_key(TxKind::CreateCommitment, result.create_commitment_tx_hash);
		let tx = db.get::<Tx>(key).unwrap();
		let commitment = CreateCommitment::decode(&mut tx.body().as_slice()).unwrap();
		let create_scores_tx: Vec<Tx> = commitment
			.scores_tx_hashes
			.into_iter()
			.map(|x| {
				let key = Tx::construct_full_key(TxKind::CreateScores, x);
				db.get::<Tx>(key).unwrap()
			})
			.collect();
		let create_scores: Vec<CreateScores> = create_scores_tx
			.into_iter()
			.map(|tx| CreateScores::decode(&mut tx.body().as_slice()).unwrap())
			.collect();
		let mut score_entries: Vec<ScoreEntry> =
			create_scores.into_iter().map(|x| x.entries).flatten().collect();
		score_entries.sort_by(|a, b| a.value.partial_cmp(&b.value).unwrap());

		let verificarion_results_tx: Vec<Tx> = result
			.job_verification_tx_hashes
			.iter()
			.map(|x| {
				let key = Tx::construct_full_key(TxKind::JobVerification, x.clone());
				db.get::<Tx>(key).unwrap()
			})
			.collect();
		let verification_results: Vec<JobVerification> = verificarion_results_tx
			.into_iter()
			.map(|tx| JobVerification::decode(&mut tx.body().as_slice()).unwrap())
			.collect();
		let verification_results_bools: Vec<bool> =
			verification_results.into_iter().map(|x| x.verification_result).collect();

		Ok(serde_json::to_value((verification_results_bools, score_entries)).unwrap())
	}
}
