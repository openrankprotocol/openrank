use alloy_rlp::encode;
use clap::{Parser, ValueEnum};
use csv::StringRecord;
use dotenv::dotenv;
use k256::{ecdsa::SigningKey, schnorr::CryptoRngCore};
use karyon_jsonrpc::Client;
use openrank_common::{
    address_from_sk,
    merkle::hash_leaf,
    topics::Domain,
    tx_event::TxEvent,
    txs::{Address, JobRunRequest, ScoreEntry, SeedUpdate, TrustEntry, TrustUpdate, Tx, TxKind},
};
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha3::Keccak256;
use std::error::Error;
use std::fs::File;

const TRUST_CHUNK_SIZE: usize = 500;
const SEED_CHUNK_SIZE: usize = 1000;

#[derive(Parser, Debug, Clone, ValueEnum)]
enum Method {
    TrustUpdate,
    SeedUpdate,
    JobRunRequest,
    GetResults,
    GenerateKeypair,
    ShowAddress,
}

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(value_enum)]
    method: Method,
    arg: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub domain: Domain,
}

async fn update_trust(sk: SigningKey) -> Result<(), Box<dyn Error>> {
    let config: Config = toml::from_str(include_str!("../config.toml"))?;

    let f = File::open("./trust-db.csv")?;
    let mut rdr = csv::Reader::from_reader(f);
    let mut entries = Vec::new();
    for result in rdr.records() {
        let record: StringRecord = result?;
        let (from, to, value): (u32, u32, f32) = record.deserialize(None)?;
        let from_addr = Address::from(from);
        let to_addr = Address::from(to);
        let trust_entry = TrustEntry::new(from_addr, to_addr, value);
        entries.push(trust_entry);
    }

    // Creates a new client
    let client = Client::builder("tcp://127.0.0.1:60000")?.build().await?;

    for chunk in entries.chunks(TRUST_CHUNK_SIZE) {
        let data = encode(TrustUpdate::new(
            config.domain.trust_namespace(),
            chunk.to_vec(),
        ));
        let mut tx = Tx::default_with(TxKind::TrustUpdate, data);
        tx.sign(&sk)?;

        let result: Value = client.call("Sequencer.trust_update", hex::encode(encode(tx))).await?;
        let tx_event: TxEvent = serde_json::from_value(result)?;

        println!("Res: {:?}", tx_event);
    }
    Ok(())
}

async fn update_seed(sk: SigningKey) -> Result<(), Box<dyn Error>> {
    let config: Config = toml::from_str(include_str!("../config.toml"))?;

    let f = File::open("./seed-db.csv")?;
    let mut rdr = csv::Reader::from_reader(f);
    let mut entries = Vec::new();
    for result in rdr.records() {
        let record: StringRecord = result?;
        let (i, value): (u32, f32) = record.deserialize(None)?;
        let i_addr = Address::from(i);
        let score_entry = ScoreEntry::new(i_addr, value);
        entries.push(score_entry);
    }

    // Creates a new client
    let client = Client::builder("tcp://127.0.0.1:60000")?.build().await?;

    for chunk in entries.chunks(SEED_CHUNK_SIZE) {
        let data = encode(SeedUpdate::new(
            config.domain.seed_namespace(),
            chunk.to_vec(),
        ));
        let mut tx = Tx::default_with(TxKind::SeedUpdate, data);
        tx.sign(&sk)?;

        let result: Value = client.call("Sequencer.seed_update", hex::encode(encode(tx))).await?;
        let tx_event: TxEvent = serde_json::from_value(result)?;

        println!("Res: {:?}", tx_event);
    }
    Ok(())
}

async fn job_run_request(sk: SigningKey) -> Result<(), Box<dyn Error>> {
    let config: Config = toml::from_str(include_str!("../config.toml"))?;

    // Creates a new client
    let client = Client::builder("tcp://127.0.0.1:60000")?.build().await?;

    let rng = &mut thread_rng();
    let domain_id = config.domain.to_hash();
    let hash = hash_leaf::<Keccak256>(rng.gen::<[u8; 32]>().to_vec());
    let data = encode(JobRunRequest::new(domain_id, 0, hash));
    let mut tx = Tx::default_with(TxKind::JobRunRequest, data);
    tx.sign(&sk)?;
    let tx_hash = tx.hash();
    let hex_encoded_tx_hash = hex::encode(tx_hash.0);
    println!("JobRunRequest TX_HASH: {}", hex_encoded_tx_hash);

    let result: Value = client.call("Sequencer.job_run_request", hex::encode(encode(tx))).await?;
    let tx_event: TxEvent = serde_json::from_value(result)?;

    println!("Res: {:?}", tx_event);
    Ok(())
}

async fn get_results(arg: String) -> Result<(Vec<bool>, Vec<ScoreEntry>), Box<dyn Error>> {
    // Creates a new client
    let client = Client::builder("tcp://127.0.0.1:60000")?.build().await?;
    let result: Value = client.call("Sequencer.get_results", arg).await?;
    let scores: (Vec<bool>, Vec<ScoreEntry>) = serde_json::from_value(result)?;
    Ok(scores)
}

fn generate_keypair<R: CryptoRngCore>(rng: &mut R) -> (SigningKey, Address) {
    let sk = SigningKey::random(rng);
    let addr = address_from_sk(&sk);
    (sk, addr)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    let cli = Args::parse();
    let secret_key_hex = std::env::var("SECRET_KEY").expect("SECRET_KEY must be set.");
    let secret_key_bytes = hex::decode(secret_key_hex)?;
    let secret_key = SigningKey::from_slice(secret_key_bytes.as_slice())?;

    match cli.method {
        Method::TrustUpdate => {
            update_trust(secret_key).await?;
        },
        Method::SeedUpdate => {
            update_seed(secret_key).await?;
        },
        Method::JobRunRequest => {
            job_run_request(secret_key).await?;
        },
        Method::GetResults => {
            let arg = match cli.arg {
                Some(arg) => arg,
                None => {
                    eprintln!("Missing argument");
                    std::process::exit(1);
                },
            };
            let (votes, mut results) = get_results(arg).await?;
            println!("votes: {:?}", votes);
            results.reverse();
            let chunk = results.chunks(100).next();
            if let Some(scores) = chunk {
                for res in scores {
                    println!("{}: {}", res.id, res.value);
                }
            }
        },
        Method::GenerateKeypair => {
            let rng = &mut thread_rng();
            let (sk, address) = generate_keypair(rng);
            let sk_bytes = sk.to_bytes();
            println!("SIGNING_KEY: {}", hex::encode(sk_bytes));
            println!("ADDRESS:     {}", address.to_hex());
        },
        Method::ShowAddress => {
            let addr = address_from_sk(&secret_key);
            println!("ADDRESS: {}", addr.to_hex());
        },
    }
    Ok(())
}
