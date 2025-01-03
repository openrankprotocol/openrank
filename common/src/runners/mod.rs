use crate::{
    merkle::{self, incremental::DenseIncrementalMerkleTree},
    misc::OutboundLocalTrust,
    topics::{Domain, DomainHash},
    tx::trust::OwnedNamespace,
};
use getset::Getters;
use sha3::Keccak256;
use std::{
    collections::HashMap,
    fmt::{Display, Formatter, Result as FmtResult},
};

pub mod compute_runner;
pub mod verification_runner;

#[derive(Getters)]
#[getset(get = "pub")]
pub struct BaseRunner {
    pub(crate) count: HashMap<DomainHash, u64>,
    pub(crate) indices: HashMap<DomainHash, HashMap<String, u64>>,
    pub(crate) local_trust: HashMap<OwnedNamespace, HashMap<u64, OutboundLocalTrust>>,
    pub(crate) seed_trust: HashMap<OwnedNamespace, HashMap<u64, f32>>,
    pub(crate) lt_sub_trees:
        HashMap<DomainHash, HashMap<u64, DenseIncrementalMerkleTree<Keccak256>>>,
    pub(crate) lt_master_tree: HashMap<DomainHash, DenseIncrementalMerkleTree<Keccak256>>,
    pub(crate) st_master_tree: HashMap<DomainHash, DenseIncrementalMerkleTree<Keccak256>>,
}

impl BaseRunner {
    pub fn new(domains: &[Domain]) -> Self {
        let mut count = HashMap::new();
        let mut indices = HashMap::new();
        let mut local_trust = HashMap::new();
        let mut seed_trust = HashMap::new();
        let mut lt_sub_trees = HashMap::new();
        let mut lt_master_tree = HashMap::new();
        let mut st_master_tree = HashMap::new();
        let mut compute_results = HashMap::new();
        for domain in domains {
            let domain_hash = domain.to_hash();
            count.insert(domain_hash, 0);
            indices.insert(domain_hash, HashMap::new());
            local_trust.insert(domain.trust_namespace(), HashMap::new());
            seed_trust.insert(domain.trust_namespace(), HashMap::new());
            lt_sub_trees.insert(domain_hash, HashMap::new());
            lt_master_tree.insert(
                domain_hash,
                DenseIncrementalMerkleTree::<Keccak256>::new(32),
            );
            st_master_tree.insert(
                domain_hash,
                DenseIncrementalMerkleTree::<Keccak256>::new(32),
            );
            compute_results.insert(domain_hash, Vec::<f32>::new());
        }
        Self {
            count,
            indices,
            local_trust,
            seed_trust,
            lt_sub_trees,
            lt_master_tree,
            st_master_tree,
        }
    }
}

#[derive(Debug)]
pub enum Error {
    IndicesNotFound(DomainHash),
    CountNotFound(DomainHash),

    LocalTrustSubTreesNotFoundWithDomain(DomainHash),

    LocalTrustMasterTreeNotFound(DomainHash),
    SeedTrustMasterTreeNotFound(DomainHash),

    LocalTrustNotFound(OwnedNamespace),

    SeedTrustNotFound(OwnedNamespace),

    DomainIndexNotFound(String),

    Merkle(merkle::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Self::IndicesNotFound(domain) => {
                write!(f, "indices not found for domain: {:?}", domain)
            },
            Self::CountNotFound(domain) => write!(f, "count not found for domain: {:?}", domain),
            Self::LocalTrustSubTreesNotFoundWithDomain(domain) => {
                write!(
                    f,
                    "local_trust_sub_trees not found for domain: {:?}",
                    domain
                )
            },

            Self::LocalTrustMasterTreeNotFound(domain) => {
                write!(
                    f,
                    "local_trust_master_tree not found for domain: {:?}",
                    domain
                )
            },
            Self::SeedTrustMasterTreeNotFound(domain) => {
                write!(
                    f,
                    "seed_trust_master_tree not found for domain: {:?}",
                    domain
                )
            },
            Self::LocalTrustNotFound(domain) => {
                write!(f, "local_trust not found for domain: {:?}", domain)
            },
            Self::SeedTrustNotFound(domain) => {
                write!(f, "seed_trust not found for domain: {:?}", domain)
            },
            Self::DomainIndexNotFound(address) => {
                write!(f, "domain_indice not found for address: {:?}", address)
            },
            Self::Merkle(err) => err.fmt(f),
        }
    }
}
