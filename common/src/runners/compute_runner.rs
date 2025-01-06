use crate::{
    algos::et::positive_run,
    merkle::{
        self, fixed::DenseMerkleTree, hash_leaf, hash_two, incremental::DenseIncrementalMerkleTree,
        Hash,
    },
    misc::OutboundLocalTrust,
    topics::{Domain, DomainHash},
    tx::{
        compute,
        trust::{ScoreEntry, TrustEntry},
    },
};
use getset::Getters;
use sha3::Keccak256;
use std::{
    collections::HashMap,
    fmt::{Display, Formatter, Result as FmtResult},
};

use super::{BaseRunner, Error as BaseError};

#[derive(Getters)]
#[getset(get = "pub")]
/// Struct containing the state of the computer compute runner.
pub struct ComputeRunner {
    base: BaseRunner,
    compute_results: HashMap<DomainHash, Vec<(u64, f32)>>,
    compute_tree: HashMap<DomainHash, DenseMerkleTree<Keccak256>>,
}

impl ComputeRunner {
    pub fn new(domains: &[Domain]) -> Self {
        let base = BaseRunner::new(domains);
        let mut compute_results = HashMap::new();
        for domain in domains {
            let domain_hash = domain.to_hash();
            compute_results.insert(domain_hash, Vec::<(u64, f32)>::new());
        }
        Self { base, compute_results, compute_tree: HashMap::new() }
    }

    /// Update the state of trees for certain domain, with the given trust entries.
    pub fn update_trust(
        &mut self, domain: Domain, trust_entries: Vec<TrustEntry>,
    ) -> Result<(), Error> {
        self.base.update_trust(domain, trust_entries).map_err(Error::Base)
    }

    /// Update the state of trees for certain domain, with the given seed entries.
    pub fn update_seed(
        &mut self, domain: Domain, seed_entries: Vec<ScoreEntry>,
    ) -> Result<(), Error> {
        self.base.update_seed(domain, seed_entries).map_err(Error::Base)
    }

    /// Compute the EigenTrust scores for certain domain.
    pub fn compute(&mut self, domain: Domain) -> Result<(), Error> {
        let lt = self
            .base
            .local_trust
            .get(&domain.trust_namespace())
            .ok_or::<Error>(BaseError::LocalTrustNotFound(domain.trust_namespace()).into())?;
        let seed = self
            .base
            .seed_trust
            .get(&domain.seed_namespace())
            .ok_or::<Error>(BaseError::SeedTrustNotFound(domain.seed_namespace()).into())?;
        let count = self
            .base
            .count
            .get(&domain.to_hash())
            .ok_or::<Error>(BaseError::CountNotFound(domain.to_hash()).into())?;
        let res = positive_run(lt.clone(), seed.clone(), *count);
        self.compute_results.insert(domain.to_hash(), res);
        Ok(())
    }

    /// Create the compute tree for certain domain.
    pub fn create_compute_tree(&mut self, domain: Domain) -> Result<(), Error> {
        let scores = self
            .compute_results
            .get(&domain.to_hash())
            .ok_or(Error::ComputeResultsNotFound(domain.to_hash()))?;
        let score_hashes: Vec<Hash> =
            scores.iter().map(|(_, x)| hash_leaf::<Keccak256>(x.to_be_bytes().to_vec())).collect();
        let compute_tree =
            DenseMerkleTree::<Keccak256>::new(score_hashes).map_err(Error::Merkle)?;
        self.compute_tree.insert(domain.to_hash(), compute_tree);
        Ok(())
    }

    /// Get the compute scores for certain domain.
    pub fn get_compute_scores(&self, domain: Domain) -> Result<Vec<compute::Scores>, Error> {
        let domain_indices = self
            .base
            .indices
            .get(&domain.to_hash())
            .ok_or::<Error>(BaseError::IndicesNotFound(domain.to_hash()).into())?;
        let scores = self
            .compute_results
            .get(&domain.to_hash())
            .ok_or(Error::ComputeResultsNotFound(domain.to_hash()))?;
        let index_to_address: HashMap<&u64, &String> =
            domain_indices.iter().map(|(k, v)| (v, k)).collect();
        let mut compute_scores_txs = Vec::new();
        for chunk in scores.chunks(1000) {
            let mut entries = Vec::new();
            for (index, val) in chunk {
                let address =
                    index_to_address.get(&index).ok_or(Error::IndexToAddressNotFound(*index))?;
                let score_entry = ScoreEntry::new((*address).clone(), *val);
                entries.push(score_entry);
            }
            let compute_scores = compute::Scores::new(entries);
            compute_scores_txs.push(compute_scores);
        }
        Ok(compute_scores_txs)
    }

    /// Get the local trust root hash and compute tree root hash for certain domain.
    pub fn get_root_hashes(&self, domain: Domain) -> Result<(Hash, Hash), Error> {
        let tree_roots = self.base.get_base_root_hashes(&domain)?;

        let compute_tree = self
            .compute_tree
            .get(&domain.to_hash())
            .ok_or(Error::ComputeTreeNotFound(domain.to_hash()))?;
        let ct_tree_root = compute_tree.root().map_err(Error::Merkle)?;

        Ok((tree_roots, ct_tree_root))
    }
}

#[derive(Debug)]
/// Errors that can arise while using the compute runner.
pub enum Error {
    Base(BaseError),
    /// The compute results for the domain are not found.
    ComputeResultsNotFound(DomainHash),
    /// The index to address mapping for the domain are not found.
    IndexToAddressNotFound(u64),
    /// The compute tree for the domain are not found.
    ComputeTreeNotFound(DomainHash),

    /// The compute merkle tree error.
    Merkle(merkle::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Self::Base(err) => err.fmt(f),

            Self::ComputeResultsNotFound(domain) => {
                write!(f, "compute_results not found for domain: {:?}", domain)
            },
            Self::IndexToAddressNotFound(index) => {
                write!(f, "index_to_address not found for index: {}", index)
            },
            Self::ComputeTreeNotFound(domain) => {
                write!(f, "compute_tree not found for domain: {:?}", domain)
            },

            Self::Merkle(err) => err.fmt(f),
        }
    }
}

impl From<BaseError> for Error {
    fn from(err: BaseError) -> Self {
        Self::Base(err)
    }
}
