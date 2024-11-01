use openrank_common::{
    algos::{self, et::positive_run},
    merkle::{
        self, fixed::DenseMerkleTree, hash_leaf, hash_two, incremental::DenseIncrementalMerkleTree,
        Hash,
    },
    topics::{Domain, DomainHash},
    tx::{
        compute,
        trust::{ScoreEntry, TrustEntry},
    },
};
use sha3::Keccak256;
use std::{
    collections::HashMap,
    fmt::{Display, Formatter, Result as FmtResult},
};

/// Struct containing the state of the computer compute runner.
pub struct ComputeRunner {
    count: HashMap<DomainHash, u64>,
    indices: HashMap<DomainHash, HashMap<String, u64>>,
    local_trust: HashMap<DomainHash, HashMap<(u64, u64), f32>>,
    seed_trust: HashMap<DomainHash, HashMap<u64, f32>>,
    lt_sub_trees: HashMap<DomainHash, HashMap<u64, DenseIncrementalMerkleTree<Keccak256>>>,
    lt_master_tree: HashMap<DomainHash, DenseIncrementalMerkleTree<Keccak256>>,
    compute_results: HashMap<DomainHash, Vec<(u64, f32)>>,
    compute_tree: HashMap<DomainHash, DenseMerkleTree<Keccak256>>,
}

impl ComputeRunner {
    pub fn new(domains: Vec<DomainHash>) -> Self {
        let mut count = HashMap::new();
        let mut indices = HashMap::new();
        let mut local_trust = HashMap::new();
        let mut seed_trust = HashMap::new();
        let mut lt_sub_trees = HashMap::new();
        let mut lt_master_tree = HashMap::new();
        let mut compute_results = HashMap::new();
        for domain in domains {
            count.insert(domain, 0);
            indices.insert(domain, HashMap::new());
            local_trust.insert(domain, HashMap::new());
            seed_trust.insert(domain, HashMap::new());
            lt_sub_trees.insert(domain, HashMap::new());
            lt_master_tree.insert(domain, DenseIncrementalMerkleTree::<Keccak256>::new(32));
            compute_results.insert(domain, Vec::<(u64, f32)>::new());
        }
        Self {
            count,
            indices,
            local_trust,
            seed_trust,
            lt_sub_trees,
            lt_master_tree,
            compute_results,
            compute_tree: HashMap::new(),
        }
    }

    /// Update the state of trees for certain domain, with the given trust entries.
    pub fn update_trust(
        &mut self, domain: Domain, trust_entries: Vec<TrustEntry>,
    ) -> Result<(), Error> {
        let domain_indices = self
            .indices
            .get_mut(&domain.to_hash())
            .ok_or(Error::IndexNotFound(domain.to_hash()))?;
        let count =
            self.count.get_mut(&domain.to_hash()).ok_or(Error::CountNotFound(domain.to_hash()))?;
        let lt_sub_trees = self.lt_sub_trees.get_mut(&domain.to_hash()).ok_or(
            Error::LocalTrustSubTreesNotFoundWithDomain(domain.to_hash()),
        )?;
        let lt_master_tree = self
            .lt_master_tree
            .get_mut(&domain.to_hash())
            .ok_or(Error::LocalTrustMasterTreeNotFound(domain.to_hash()))?;
        let lt = self
            .local_trust
            .get_mut(&domain.to_hash())
            .ok_or(Error::LocalTrustNotFound(domain.to_hash()))?;
        let seed = self
            .seed_trust
            .get(&domain.to_hash())
            .ok_or(Error::SeedTrustNotFound(domain.to_hash()))?;
        let default_sub_tree = DenseIncrementalMerkleTree::<Keccak256>::new(32);
        for entry in trust_entries {
            let from_index = if let Some(i) = domain_indices.get(&entry.from) {
                *i
            } else {
                let curr_count = *count;
                domain_indices.insert(entry.from.clone(), curr_count);
                *count += 1;
                curr_count
            };
            let to_index = if let Some(i) = domain_indices.get(&entry.to) {
                *i
            } else {
                let curr_count = *count;
                domain_indices.insert(entry.to.clone(), *count);
                *count += 1;
                curr_count
            };
            lt.insert((from_index, to_index), entry.value);

            lt_sub_trees.entry(from_index).or_insert_with(|| default_sub_tree.clone());
            let sub_tree = lt_sub_trees
                .get_mut(&from_index)
                .ok_or(Error::LocalTrustSubTreesNotFoundWithIndex(from_index))?;
            let leaf = hash_leaf::<Keccak256>(entry.value.to_be_bytes().to_vec());
            sub_tree.insert_leaf(to_index, leaf);

            let sub_tree_root = sub_tree.root().map_err(Error::Merkle)?;
            let seed_value = seed.get(&to_index).unwrap_or(&0.0);
            let seed_hash = hash_leaf::<Keccak256>(seed_value.to_be_bytes().to_vec());
            let leaf = hash_two::<Keccak256>(sub_tree_root, seed_hash);
            lt_master_tree.insert_leaf(from_index, leaf);
        }

        Ok(())
    }

    /// Update the state of trees for certain domain, with the given seed entries.
    pub fn update_seed(
        &mut self, domain: Domain, seed_entries: Vec<ScoreEntry>,
    ) -> Result<(), Error> {
        let domain_indices = self
            .indices
            .get_mut(&domain.to_hash())
            .ok_or(Error::IndexNotFound(domain.to_hash()))?;
        let count =
            self.count.get_mut(&domain.to_hash()).ok_or(Error::CountNotFound(domain.to_hash()))?;
        let lt_sub_trees = self.lt_sub_trees.get_mut(&domain.to_hash()).ok_or(
            Error::LocalTrustSubTreesNotFoundWithDomain(domain.to_hash()),
        )?;
        let lt_master_tree = self
            .lt_master_tree
            .get_mut(&domain.to_hash())
            .ok_or(Error::LocalTrustMasterTreeNotFound(domain.to_hash()))?;
        let seed = self
            .seed_trust
            .get_mut(&domain.to_hash())
            .ok_or(Error::SeedTrustNotFound(domain.to_hash()))?;
        let default_sub_tree = DenseIncrementalMerkleTree::<Keccak256>::new(32);
        for entry in seed_entries {
            let index = if let Some(i) = domain_indices.get(&entry.id) {
                *i
            } else {
                let curr_count = *count;
                domain_indices.insert(entry.id.clone(), curr_count);
                *count += 1;
                curr_count
            };

            lt_sub_trees.entry(index).or_insert_with(|| default_sub_tree.clone());
            let sub_tree = lt_sub_trees
                .get_mut(&index)
                .ok_or(Error::LocalTrustSubTreesNotFoundWithIndex(index))?;
            let sub_tree_root = sub_tree.root().map_err(Error::Merkle)?;
            let seed_hash = hash_leaf::<Keccak256>(entry.value.to_be_bytes().to_vec());
            let leaf = hash_two::<Keccak256>(sub_tree_root, seed_hash);
            lt_master_tree.insert_leaf(index, leaf);

            seed.insert(index, entry.value);
        }

        Ok(())
    }

    /// Compute the EigenTrust scores for certain domain.
    pub fn compute(&mut self, domain: Domain) -> Result<(), Error> {
        let lt = self
            .local_trust
            .get(&domain.to_hash())
            .ok_or(Error::LocalTrustNotFound(domain.to_hash()))?;
        let seed = self
            .seed_trust
            .get(&domain.to_hash())
            .ok_or(Error::SeedTrustNotFound(domain.to_hash()))?;
        let res = positive_run::<20>(lt.clone(), seed.clone()).map_err(Error::Algo)?;
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
        let domain_indices =
            self.indices.get(&domain.to_hash()).ok_or(Error::IndexNotFound(domain.to_hash()))?;
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
        let lt_tree = self
            .lt_master_tree
            .get(&domain.to_hash())
            .ok_or(Error::LocalTrustMasterTreeNotFound(domain.to_hash()))?;
        let compute_tree = self
            .compute_tree
            .get(&domain.to_hash())
            .ok_or(Error::ComputeTreeNotFound(domain.to_hash()))?;
        let lt_tree_root = lt_tree.root().map_err(Error::Merkle)?;
        let ct_tree_root = compute_tree.root().map_err(Error::Merkle)?;
        Ok((lt_tree_root, ct_tree_root))
    }
}

#[derive(Debug)]
/// Errors that can arise while using the compute runner.
pub enum Error {
    /// The index for the domain are not found.
    IndexNotFound(DomainHash),
    /// The count for the domain is not found.
    CountNotFound(DomainHash),
    /// The local trust sub trees for the domain are not found.
    LocalTrustSubTreesNotFoundWithDomain(DomainHash),
    /// The local trust sub tree for the index is not found.
    LocalTrustSubTreesNotFoundWithIndex(u64),
    /// The local trust master tree for the domain are not found.
    LocalTrustMasterTreeNotFound(DomainHash),
    /// The local trust for the domain are not found.
    LocalTrustNotFound(DomainHash),
    /// The seed trust for the domain are not found.
    SeedTrustNotFound(DomainHash),
    /// The compute results for the domain are not found.
    ComputeResultsNotFound(DomainHash),
    /// The index to address mapping for the domain are not found.
    IndexToAddressNotFound(u64),
    /// The compute tree for the domain are not found.
    ComputeTreeNotFound(DomainHash),

    /// The compute merkle tree error.
    Merkle(merkle::Error),
    /// The compute algorithm error.
    Algo(algos::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Self::IndexNotFound(domain) => {
                write!(f, "index not found for domain: {:?}", domain)
            },
            Self::CountNotFound(domain) => write!(f, "count not found for domain: {:?}", domain),
            Self::LocalTrustSubTreesNotFoundWithDomain(domain) => write!(
                f,
                "local_trust_sub_trees not found for domain: {:?}",
                domain
            ),
            Self::LocalTrustSubTreesNotFoundWithIndex(index) => {
                write!(f, "local_trust_sub_trees not found for index: {}", index)
            },

            Self::LocalTrustMasterTreeNotFound(domain) => {
                write!(
                    f,
                    "local_trust_master_tree not found for domain: {:?}",
                    domain
                )
            },
            Self::LocalTrustNotFound(domain) => {
                write!(f, "local_trust not found for domain: {:?}", domain)
            },
            Self::SeedTrustNotFound(domain) => {
                write!(f, "seed_trust not found for domain: {:?}", domain)
            },
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
            Self::Algo(err) => err.fmt(f),
        }
    }
}
