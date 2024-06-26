use std::hash::{DefaultHasher, Hasher};

use alloy_rlp_derive::{RlpDecodable, RlpEncodable};

use crate::txs::Address;

#[derive(Clone, Debug, Default, RlpEncodable, RlpDecodable)]
pub struct DomainHash(u64);

impl DomainHash {
	pub fn from_bytes(data: Vec<u8>) -> Self {
		let mut bytes = [0; 8];
		bytes.clone_from_slice(&data);
		let value = u64::from_be_bytes(bytes);
		Self(value)
	}

	pub fn to_bytes(&self) -> Vec<u8> {
		self.0.to_be_bytes().to_vec()
	}
}

#[derive(Clone, Debug)]
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

pub struct TopicHash(u64);

impl TopicHash {
	pub fn to_hex(&self) -> String {
		hex::encode(self.0.to_be_bytes())
	}
}

#[derive(Clone, Debug)]
pub enum Topic {
	DomainRequest(DomainHash),
	DomainAssignent(DomainHash),
	DomainCommitment(DomainHash),
	DomainScores(DomainHash),
	DomainVerification(DomainHash),
	ProposedBlock,
	FinalisedBlock,
}

impl Topic {
	pub fn to_hash(&self) -> TopicHash {
		let mut s = DefaultHasher::new();
		match self {
			Self::DomainRequest(domain_id) => {
				s.write(&domain_id.0.to_be_bytes());
				s.write("request".as_bytes());
			},
			Self::DomainAssignent(domain_id) => {
				s.write(&domain_id.0.to_be_bytes());
				s.write("assignment".as_bytes());
			},
			Self::DomainCommitment(domain_id) => {
				s.write(&domain_id.0.to_be_bytes());
				s.write("commitment".as_bytes());
			},
			Self::DomainScores(domain_id) => {
				s.write(&domain_id.0.to_be_bytes());
				s.write("scores".as_bytes());
			},
			Self::DomainVerification(domain_id) => {
				s.write(&domain_id.0.to_be_bytes());
				s.write("verification".as_bytes());
			},
			Self::ProposedBlock => {
				s.write("proposed_block".as_bytes());
			},
			Self::FinalisedBlock => {
				s.write("finalised_block".as_bytes());
			},
		}
		let res = s.finish();
		TopicHash(res)
	}
}
