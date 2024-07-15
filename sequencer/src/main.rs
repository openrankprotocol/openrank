use openrank_sequencer;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	openrank_sequencer::run().await
}

#[cfg(test)]
mod test {
	use alloy_rlp::encode;
	use karyon_jsonrpc::Client;
	use openrank_common::{
		tx_event::TxEvent,
		txs::{JobRunRequest, SeedUpdate, TrustUpdate, Tx, TxKind},
	};
	use openrank_sequencer::DomainRequest;
	use serde_json::Value;

	#[tokio::test]
	// #[ignore = "Used only for debugging purposes"]
	async fn call_trust_update() {
		let data = encode(TrustUpdate::default());
		let kind = TxKind::TrustUpdate;
		let tx = Tx::default_with(kind, data);
		let tx_event = hex::encode(encode(tx));
		let domain_req = DomainRequest::new(16832277605718933204, tx_event);
		// Creates a new client
		let client = Client::builder("tcp://127.0.0.1:60000")
			.expect("create new client builder")
			.build()
			.await
			.expect("build the client");

		let result: Value =
			client.call("Sequencer.trust_update", domain_req).await.expect("send a request");
		let tx_event: TxEvent = serde_json::from_value(result).unwrap();

		println!("Res: {:?}", tx_event);
	}

	#[tokio::test]
	// #[ignore = "Used only for debugging purposes"]
	async fn call_seed_update() {
		let data = encode(SeedUpdate::default());
		let kind = TxKind::SeedUpdate;
		let tx = Tx::default_with(kind, data);
		let tx_event = hex::encode(encode(tx));
		let domain_req = DomainRequest::new(16832277605718933204, tx_event);
		// Creates a new client
		let client = Client::builder("tcp://127.0.0.1:60000")
			.expect("create new client builder")
			.build()
			.await
			.expect("build the client");

		let result: Value =
			client.call("Sequencer.seed_update", domain_req).await.expect("send a request");
		let tx_event: TxEvent = serde_json::from_value(result).unwrap();

		println!("Res: {:?}", tx_event);
	}

	#[tokio::test]
	// #[ignore = "Used only for debugging purposes"]
	async fn call_job_run_request() {
		let data = encode(JobRunRequest::default());
		let kind = TxKind::JobRunRequest;
		let tx = Tx::default_with(kind, data);
		let tx_event = hex::encode(encode(tx));
		let domain_req = DomainRequest::new(16832277605718933204, tx_event);
		// Creates a new client
		let client = Client::builder("tcp://127.0.0.1:60000")
			.expect("create new client builder")
			.build()
			.await
			.expect("build the client");

		let result: Value =
			client.call("Sequencer.job_run_request", domain_req).await.expect("send a request");
		let tx_event: TxEvent = serde_json::from_value(result).unwrap();

		println!("Res: {:?}", tx_event);
	}
}
