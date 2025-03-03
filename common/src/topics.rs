use crate::{
    format_hex,
    tx::{consts, trust::OwnedNamespace, Address},
};
use alloy_rlp_derive::{RlpDecodable, RlpEncodable};
use getset::Getters;
use hex::FromHex;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{Display, Formatter, Result as FmtResult},
    hash::{DefaultHasher, Hasher},
};

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Hash,
    PartialEq,
    Eq,
    RlpEncodable,
    RlpDecodable,
    Serialize,
    Deserialize,
)]
/// Hash of the [Domain].
pub struct DomainHash(#[serde(with = "hex")] [u8; 8]);

impl DomainHash {
    /// Convert the hash value to a hex string.
    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }

    /// Get the inner value of the hash.
    pub fn inner(self) -> [u8; 8] {
        self.0
    }
}

impl FromHex for DomainHash {
    type Error = hex::FromHexError;

    /// Convert a hex string to a [DomainHash].
    fn from_hex<T: AsRef<[u8]>>(hex: T) -> Result<Self, Self::Error> {
        Ok(DomainHash(<[u8; 8]>::from_hex(hex)?))
    }
}

impl From<[u8; 8]> for DomainHash {
    fn from(value: [u8; 8]) -> Self {
        Self(value)
    }
}

impl Display for DomainHash {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", format_hex(self.to_hex()))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
/// Domain of the openrank network. Consists of a trust namespace and a seed namespace + algorithm id.
pub struct Domain {
    /// Address of the trust namespace owner.
    trust_owner: Address,
    /// ID of the trust namespace.
    trust_id: u32,
    /// Address of the seed namespace owner.
    seed_owner: Address,
    /// ID of the seed namespace.
    seed_id: u32,
    /// ID of the algorithm used for the domain.
    algo_id: u64,
}

impl Domain {
    pub fn new(
        trust_owner: Address, trust_id: u32, seed_owner: Address, seed_id: u32, algo_id: u64,
    ) -> Self {
        Self { trust_owner, trust_id, seed_owner, seed_id, algo_id }
    }

    /// Returns the trust namespace of the domain.
    pub fn trust_namespace(&self) -> OwnedNamespace {
        OwnedNamespace::new(self.trust_owner, self.trust_id)
    }

    /// Returns the seed namespace of the domain.
    pub fn seed_namespace(&self) -> OwnedNamespace {
        OwnedNamespace::new(self.seed_owner, self.seed_id)
    }

    /// Returns the domain hash, created from the trust and seed namespace + algo id.
    pub fn to_hash(&self) -> DomainHash {
        let mut s = DefaultHasher::new();
        s.write(self.trust_owner.as_slice());
        s.write(&self.trust_id.to_be_bytes());
        s.write(self.seed_owner.as_slice());
        s.write(&self.seed_id.to_be_bytes());
        s.write(&self.algo_id.to_be_bytes());
        let res = s.finish();
        DomainHash(res.to_be_bytes())
    }

    /// Returns the topics for the domain.
    pub fn topics(&self) -> Vec<Topic> {
        vec![
            Topic::NamespaceTrustUpdate(self.trust_namespace()),
            Topic::NamespaceSeedUpdate(self.seed_namespace()),
            Topic::DomainAssignent(self.to_hash()),
        ]
    }
}

/// Topics for openrank p2p node gossipsub events.
#[derive(Clone, Debug)]
pub enum Topic {
    /// Topic for the trust namespace update.
    NamespaceTrustUpdate(OwnedNamespace),
    /// Topic for the seed namespace update.
    NamespaceSeedUpdate(OwnedNamespace),
    /// Topic for the domain request.
    DomainRequest(DomainHash),
    /// Topic for the domain assignent.
    DomainAssignent(DomainHash),
    /// Topic for the domain commitment.
    DomainCommitment(DomainHash),
    /// Topic for the domain scores.
    DomainScores(DomainHash),
    /// Topic for the domain verification.
    DomainVerification(DomainHash),
}

impl From<Topic> for String {
    fn from(value: Topic) -> Self {
        match value {
            Topic::NamespaceTrustUpdate(namespace) => {
                format!("{}:{}", namespace.to_hex(), consts::TRUST_UPDATE)
            },
            Topic::NamespaceSeedUpdate(namespace) => {
                format!("{}:{}", namespace.to_hex(), consts::SEED_UPDATE)
            },
            Topic::DomainRequest(domain_id) => {
                format!("{}:{}", domain_id.to_hex(), consts::COMPUTE_REQUEST)
            },
            Topic::DomainAssignent(domain_id) => {
                format!("{}:{}", domain_id.to_hex(), consts::COMPUTE_ASSIGNMENT)
            },
            Topic::DomainCommitment(domain_id) => {
                format!("{}:{}", domain_id.to_hex(), consts::COMPUTE_COMMITMENT)
            },
            Topic::DomainScores(domain_id) => {
                format!("{}:{}", domain_id.to_hex(), consts::COMPUTE_SCORES)
            },
            Topic::DomainVerification(domain_id) => {
                format!("{}:{}", domain_id.to_hex(), consts::COMPUTE_VERIFICATION)
            },
        }
    }
}

impl Display for Topic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Topic::NamespaceTrustUpdate(namespace) => {
                write!(f, "{}:{}", namespace, consts::TRUST_UPDATE)
            },
            Topic::NamespaceSeedUpdate(namespace) => {
                write!(f, "{}:{}", namespace, consts::SEED_UPDATE)
            },
            Topic::DomainRequest(domain_id) => {
                write!(f, "{}:{}", domain_id, consts::COMPUTE_REQUEST)
            },
            Topic::DomainAssignent(domain_id) => {
                write!(f, "{}:{}", domain_id, consts::COMPUTE_ASSIGNMENT)
            },
            Topic::DomainCommitment(domain_id) => {
                write!(f, "{}:{}", domain_id, consts::COMPUTE_COMMITMENT)
            },
            Topic::DomainScores(domain_id) => {
                write!(f, "{}:{}", domain_id, consts::COMPUTE_SCORES)
            },
            Topic::DomainVerification(domain_id) => {
                write!(f, "{}:{}", domain_id, consts::COMPUTE_VERIFICATION)
            },
        }
    }
}

#[cfg(test)]
mod test {
    use crate::topics::{Domain, Topic};
    use crate::tx::Address;

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

        assert_eq!(
            topic1_string,
            "00902259a9dc1a51:compute_request".to_string()
        );
        assert_eq!(
            topic2_string,
            "00902259a9dc1a51:compute_assignment".to_string()
        );
        assert_eq!(
            topic3_string,
            "00902259a9dc1a51:compute_verification".to_string()
        );
    }
}
