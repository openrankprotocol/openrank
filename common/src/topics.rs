use crate::txs::{Address, OwnedNamespace};
use alloy_rlp_derive::{RlpDecodable, RlpEncodable};
use hex::FromHex;
use serde::{Deserialize, Serialize};
use std::{
	fmt::Display,
	hash::{DefaultHasher, Hasher},
};

/// Hash(u64) of the [Domain]
#[derive(
	Clone, Debug, Default, Hash, PartialEq, Eq, RlpEncodable, RlpDecodable, Serialize, Deserialize,
)]
pub struct DomainHash(u64);

impl DomainHash {
	/// Convert the hash value to a hex string
	pub fn to_hex(self) -> String {
		hex::encode(self.0.to_be_bytes())
	}
}

impl FromHex for DomainHash {
	type Error = hex::FromHexError;

	/// Convert a hex string to a [DomainHash]
	fn from_hex<T: AsRef<[u8]>>(hex: T) -> Result<Self, Self::Error> {
		Ok(DomainHash(u64::from_be_bytes(<[u8; 8]>::from_hex(hex)?)))
	}
}

impl From<u64> for DomainHash {
	fn from(value: u64) -> Self {
		Self(value)
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Domain {
	trust_owner: Address,
	trust_id: u32,
	seed_owner: Address,
	seed_id: u32,
	algo_id: u64,
}

impl Domain {
	pub fn new(
		trust_owner: Address, trust_id: u32, seed_owner: Address, seed_id: u32, algo_id: u64,
	) -> Self {
		Self { trust_owner, trust_id, seed_owner, seed_id, algo_id }
	}

	pub fn trust_namespace(&self) -> OwnedNamespace {
		OwnedNamespace::new(self.trust_owner.clone(), self.trust_id)
	}

	pub fn seed_namespace(&self) -> OwnedNamespace {
		OwnedNamespace::new(self.seed_owner.clone(), self.seed_id)
	}

	pub fn to_hash(&self) -> DomainHash {
		let mut s = DefaultHasher::new();
		s.write(&self.trust_owner.0);
		s.write(&self.trust_id.to_be_bytes());
		s.write(&self.seed_owner.0);
		s.write(&self.seed_id.to_be_bytes());
		s.write(&self.algo_id.to_be_bytes());
		let res = s.finish();
		DomainHash(res)
	}

	pub fn topics(&self) -> Vec<Topic> {
		vec![
			Topic::NamespaceTrustUpdate(self.trust_namespace()),
			Topic::NamespaceSeedUpdate(self.seed_namespace()),
			Topic::DomainAssignent(self.to_hash()),
		]
	}
}

/// Topics for openrank p2p node gossipsub events
#[derive(Clone, Debug)]
pub enum Topic {
	NamespaceTrustUpdate(OwnedNamespace),
	NamespaceSeedUpdate(OwnedNamespace),
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
			Topic::NamespaceTrustUpdate(namespace) => {
				s.push_str(&namespace.to_hex());
				s.push_str(":trust_update");
			},
			Topic::NamespaceSeedUpdate(domain_id) => {
				s.push_str(&domain_id.to_hex());
				s.push_str(":seed_update");
			},
			Topic::DomainRequest(domain_id) => {
				s.push_str(&domain_id.to_hex());
				s.push_str(":request");
			},
			Topic::DomainAssignent(domain_id) => {
				s.push_str(&domain_id.to_hex());
				s.push_str(":assignment");
			},
			Topic::DomainCommitment(domain_id) => {
				s.push_str(&domain_id.to_hex());
				s.push_str(":commitment");
			},
			Topic::DomainScores(domain_id) => {
				s.push_str(&domain_id.to_hex());
				s.push_str(":scores");
			},
			Topic::DomainVerification(domain_id) => {
				s.push_str(&domain_id.to_hex());
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

impl Display for Topic {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(String::from(self.clone()).as_str())
	}
}

#[cfg(test)]
mod test {
	use crate::txs::Address;

	use super::{Domain, Topic};

	#[test]
	fn test_domain_to_hash() {
		let domain = Domain::new(Address::default(), 1, Address::default(), 1, 1);

		let hash = domain.to_hash();
		assert_eq!(hash.to_hex(), "00902259a9dc1a51");
	}

	#[test]
	fn test_topic_to_string() {
		let domain = Domain::new(Address::default(), 1, Address::default(), 1, 1);
		let topic1 = Topic::DomainRequest(domain.to_hash());
		let topic2 = Topic::DomainAssignent(domain.to_hash());
		let topic3 = Topic::DomainVerification(domain.to_hash());

		let topic1_string = String::from(topic1);
		let topic2_string = String::from(topic2);
		let topic3_string = String::from(topic3);

		assert_eq!(topic1_string, "00902259a9dc1a51:request".to_string());
		assert_eq!(topic2_string, "00902259a9dc1a51:assignment".to_string());
		assert_eq!(topic3_string, "00902259a9dc1a51:verification".to_string());
	}
}
