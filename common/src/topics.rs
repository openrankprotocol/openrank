use crate::txs::Address;
use alloy_rlp_derive::{RlpDecodable, RlpEncodable};
use serde::{Deserialize, Serialize};
use std::hash::{DefaultHasher, Hasher};

#[derive(Clone, Debug, Default, RlpEncodable, RlpDecodable, Serialize, Deserialize)]
pub struct DomainHash(u64);

impl From<u64> for DomainHash {
	fn from(value: u64) -> Self {
		Self(value)
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Domain {
	trust_owner: Address,
	trust_suffix: String,
	seed_owner: Address,
	seed_suffix: String,
	algo_id: u64,
}

impl Domain {
	pub fn new(
		trust_owner: Address, trust_suffix: String, seed_owner: Address, seed_suffix: String,
		algo_id: u64,
	) -> Self {
		Self { trust_owner, trust_suffix, seed_owner, seed_suffix, algo_id }
	}

	pub fn to_hash(&self) -> DomainHash {
		let mut s = DefaultHasher::new();
		s.write(&self.trust_owner.0);
		s.write(self.trust_suffix.as_bytes());
		s.write(&self.seed_owner.0);
		s.write(self.seed_suffix.as_bytes());
		s.write(&self.algo_id.to_be_bytes());
		let res = s.finish();
		DomainHash(res)
	}
}

#[derive(Clone, Debug)]
pub enum Topic {
	DomainTrustUpdate(DomainHash),
	DomainSeedUpdate(DomainHash),
	DomainRequest(DomainHash),
	DomainAssignent(DomainHash),
	DomainCommitment(DomainHash),
	DomainScores(DomainHash),
	DomainVerification(DomainHash),
	ProposedBlock,
	FinalisedBlock,
}

impl From<Topic> for String {
	fn from(value: Topic) -> Self {
		let mut s = String::new();
		match value {
			Topic::DomainTrustUpdate(domain_id) => {
				s.push_str(&hex::encode(domain_id.0.to_be_bytes()));
				s.push_str(":trust_update");
			},
			Topic::DomainSeedUpdate(domain_id) => {
				s.push_str(&hex::encode(domain_id.0.to_be_bytes()));
				s.push_str(":seed_update");
			},
			Topic::DomainRequest(domain_id) => {
				s.push_str(&hex::encode(domain_id.0.to_be_bytes()));
				s.push_str(":request");
			},
			Topic::DomainAssignent(domain_id) => {
				s.push_str(&hex::encode(domain_id.0.to_be_bytes()));
				s.push_str(":assignment");
			},
			Topic::DomainCommitment(domain_id) => {
				s.push_str(&hex::encode(domain_id.0.to_be_bytes()));
				s.push_str(":commitment");
			},
			Topic::DomainScores(domain_id) => {
				s.push_str(&hex::encode(domain_id.0.to_be_bytes()));
				s.push_str(":scores");
			},
			Topic::DomainVerification(domain_id) => {
				s.push_str(&hex::encode(domain_id.0.to_be_bytes()));
				s.push_str(":verification");
			},
			Topic::ProposedBlock => {
				s.push_str("proposed_block");
			},
			Topic::FinalisedBlock => {
				s.push_str("finalised_block");
			},
		}
		s
	}
}
