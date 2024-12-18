use getset::Getters;
use crate::{
    algos::{self, et::convergence_check},
    merkle::{
        self, fixed::DenseMerkleTree, hash_leaf, hash_two, incremental::DenseIncrementalMerkleTree,
        Hash,
    },
    topics::{Domain, DomainHash},
    tx::{
        compute,
        trust::{OwnedNamespace, ScoreEntry, TrustEntry},
        TxHash,
    },
};
use sha3::Keccak256;
use std::{
    collections::HashMap,
    fmt::{Display, Formatter, Result as FmtResult},
};

#[derive(Getters)]
#[getset(get = "pub")]
/// Struct containing the state of the verification runner
pub struct VerificationRunner {
    count: HashMap<DomainHash, u64>,
    indices: HashMap<DomainHash, HashMap<String, u64>>,
    local_trust: HashMap<OwnedNamespace, HashMap<(u64, u64), f32>>,
    seed_trust: HashMap<OwnedNamespace, HashMap<u64, f32>>,
    lt_sub_trees: HashMap<DomainHash, HashMap<u64, DenseIncrementalMerkleTree<Keccak256>>>,
    lt_master_tree: HashMap<DomainHash, DenseIncrementalMerkleTree<Keccak256>>,
    st_master_tree: HashMap<DomainHash, DenseIncrementalMerkleTree<Keccak256>>,
    compute_scores: HashMap<DomainHash, HashMap<TxHash, compute::Scores>>,
    compute_tree: HashMap<DomainHash, HashMap<TxHash, DenseMerkleTree<Keccak256>>>,
    active_assignments: HashMap<DomainHash, Vec<TxHash>>,
    commitments: HashMap<TxHash, compute::Commitment>,
}

impl VerificationRunner {
    pub fn new(domains: &[Domain]) -> Self {
        let mut count = HashMap::new();
        let mut indices = HashMap::new();
        let mut local_trust = HashMap::new();
        let mut seed_trust = HashMap::new();
        let mut lt_sub_trees = HashMap::new();
        let mut lt_master_tree = HashMap::new();
        let mut st_master_tree = HashMap::new();
        let mut compute_results = HashMap::new();
        let mut compute_scores = HashMap::new();
        let mut compute_tree = HashMap::new();
        let mut active_assignments = HashMap::new();
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
            compute_scores.insert(domain_hash, HashMap::new());
            compute_tree.insert(domain_hash, HashMap::new());
            active_assignments.insert(domain_hash, Vec::new());
        }
        Self {
            count,
            indices,
            local_trust,
            seed_trust,
            lt_sub_trees,
            lt_master_tree,
            st_master_tree,
            compute_scores,
            compute_tree,
            active_assignments,
            commitments: HashMap::new(),
        }
    }

    /// Update the state of trees for certain domain, with the given trust entries
    pub fn update_trust(
        &mut self, domain: Domain, trust_entries: Vec<TrustEntry>,
    ) -> Result<(), Error> {
        let domain_indices = self
            .indices
            .get_mut(&domain.to_hash())
            .ok_or(Error::IndicesNotFound(domain.to_hash()))?;
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
            .get_mut(&domain.trust_namespace())
            .ok_or(Error::LocalTrustNotFound(domain.trust_namespace()))?;
        let default_sub_tree = DenseIncrementalMerkleTree::<Keccak256>::new(32);
        for entry in trust_entries {
            let from_index = if let Some(i) = domain_indices.get(entry.from()) {
                *i
            } else {
                let curr_count = *count;
                domain_indices.insert(entry.from().clone(), curr_count);
                *count += 1;
                curr_count
            };
            let to_index = if let Some(i) = domain_indices.get(entry.to()) {
                *i
            } else {
                let curr_count = *count;
                domain_indices.insert(entry.to().clone(), curr_count);
                *count += 1;
                curr_count
            };
            let is_zero = entry.value() == &0.0;
            let exists = lt.contains_key(&(from_index, to_index));
            if is_zero && exists {
                lt.remove(&(from_index, to_index));
            } else if !is_zero {
                lt.insert((from_index, to_index), *entry.value());
            }

            lt_sub_trees.entry(from_index).or_insert_with(|| default_sub_tree.clone());
            let sub_tree = lt_sub_trees
                .get_mut(&from_index)
                .ok_or(Error::LocalTrustSubTreesNotFoundWithIndex(from_index))?;

            let leaf = hash_leaf::<Keccak256>(entry.value().to_be_bytes().to_vec());
            sub_tree.insert_leaf(to_index, leaf);

            let sub_tree_root = sub_tree.root().map_err(Error::Merkle)?;

            let leaf = hash_leaf::<Keccak256>(sub_tree_root.inner().to_vec());
            lt_master_tree.insert_leaf(from_index, leaf);
        }

        Ok(())
    }

    /// Update the state of trees for certain domain, with the given seed entries
    pub fn update_seed(
        &mut self, domain: Domain, seed_entries: Vec<ScoreEntry>,
    ) -> Result<(), Error> {
        let domain_indices = self
            .indices
            .get_mut(&domain.to_hash())
            .ok_or(Error::IndicesNotFound(domain.to_hash()))?;
        let count =
            self.count.get_mut(&domain.to_hash()).ok_or(Error::CountNotFound(domain.to_hash()))?;
        let st_master_tree = self
            .st_master_tree
            .get_mut(&domain.to_hash())
            .ok_or(Error::SeedTrustMasterTreeNotFound(domain.to_hash()))?;
        let seed = self
            .seed_trust
            .get_mut(&domain.seed_namespace())
            .ok_or(Error::SeedTrustNotFound(domain.seed_namespace()))?;
        for entry in seed_entries {
            let index = if let Some(i) = domain_indices.get(entry.id()) {
                *i
            } else {
                let curr_count = *count;
                domain_indices.insert(entry.id().clone(), curr_count);
                *count += 1;
                curr_count
            };
            let is_zero = entry.value() == &0.0;
            let exists = seed.contains_key(&index);
            if is_zero && exists {
                seed.remove(&index);
            } else if !is_zero {
                seed.insert(index, *entry.value());
            }

            let leaf = hash_leaf::<Keccak256>(entry.value().to_be_bytes().to_vec());
            st_master_tree.insert_leaf(index, leaf);
        }

        Ok(())
    }

    /// Check if the score tx hashes of the given commitment exists in the `compute_scores` of certain domain
    pub fn check_scores_tx_hashes(
        &self, domain: Domain, commitment: compute::Commitment,
    ) -> Result<bool, Error> {
        let compute_scores_txs = self
            .compute_scores
            .get(&domain.clone().to_hash())
            .ok_or(Error::ComputeScoresNotFoundWithDomain(domain.to_hash()))?;
        for score_tx in commitment.scores_tx_hashes() {
            let res = compute_scores_txs.contains_key(score_tx);
            if !res {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Get the list of completed assignments for certain domain
    pub fn check_finished_assignments(
        &mut self, domain: Domain,
    ) -> Result<Vec<(TxHash, bool)>, Error> {
        let assignments = self
            .active_assignments
            .get(&domain.clone().to_hash())
            .ok_or(Error::ActiveAssignmentsNotFound(domain.to_hash()))?;
        let mut results = Vec::new();
        let mut completed = Vec::new();
        for assignment_id in assignments.clone().into_iter() {
            if let Some(commitment) = self.commitments.get(&assignment_id.clone()) {
                let is_check_score_tx_hashes =
                    self.check_scores_tx_hashes(domain.clone(), commitment.clone())?;
                if is_check_score_tx_hashes {
                    let assgn_tx = assignment_id.clone();
                    let lt_root = commitment.lt_root_hash().clone();
                    let cp_root = commitment.compute_root_hash().clone();

                    self.create_compute_tree(domain.clone(), assignment_id.clone())?;
                    let (res_lt_root, res_compute_root) =
                        self.get_root_hashes(domain.clone(), assignment_id.clone())?;
                    let is_root_equal = lt_root == res_lt_root && cp_root == res_compute_root;
                    let is_converged =
                        self.compute_verification(domain.clone(), assignment_id.clone())?;
                    results.push((assgn_tx, is_root_equal && is_converged));
                    completed.push(assignment_id.clone());
                }
            }
        }
        let active_assignments = self
            .active_assignments
            .get_mut(&domain.clone().to_hash())
            .ok_or(Error::ActiveAssignmentsNotFound(domain.to_hash()))?;
        active_assignments.retain(|x| !completed.contains(x));
        Ok(results)
    }

    /// Add a new scores of certain transaction, for certain domain
    pub fn update_scores(
        &mut self, domain: Domain, tx_hash: TxHash, compute_scores: compute::Scores,
    ) -> Result<(), Error> {
        let score_values = self
            .compute_scores
            .get_mut(&domain.clone().to_hash())
            .ok_or(Error::ComputeScoresNotFoundWithDomain(domain.to_hash()))?;
        score_values.insert(tx_hash, compute_scores);
        Ok(())
    }

    /// Add a new verification assignment for certain domain.
    pub fn update_assigment(
        &mut self, domain: Domain, compute_assignment_tx_hash: TxHash,
    ) -> Result<(), Error> {
        let active_assignments = self
            .active_assignments
            .get_mut(&domain.to_hash())
            .ok_or(Error::ActiveAssignmentsNotFound(domain.to_hash()))?;
        if !active_assignments.contains(&compute_assignment_tx_hash) {
            active_assignments.push(compute_assignment_tx_hash);
        }
        Ok(())
    }

    /// Add a new commitment of certain assignment
    pub fn update_commitment(&mut self, commitment: compute::Commitment) {
        self.commitments.insert(commitment.assignment_tx_hash().clone(), commitment.clone());
    }

    /// Build the compute tree of certain assignment, for certain domain.
    pub fn create_compute_tree(
        &mut self, domain: Domain, assignment_id: TxHash,
    ) -> Result<(), Error> {
        let compute_tree_map = self
            .compute_tree
            .get_mut(&domain.to_hash())
            .ok_or(Error::ComputeTreeNotFoundWithDomain(domain.to_hash()))?;
        let commitment = self
            .commitments
            .get(&assignment_id)
            .ok_or(Error::CommitmentNotFound(assignment_id.clone()))?;
        let compute_scores = self
            .compute_scores
            .get(&domain.to_hash())
            .ok_or(Error::ComputeScoresNotFoundWithDomain(domain.to_hash()))?;
        let scores: Vec<&compute::Scores> = {
            let mut scores = Vec::new();
            for tx_hash in commitment.scores_tx_hashes().iter() {
                scores.push(
                    compute_scores
                        .get(tx_hash)
                        .ok_or(Error::ComputeScoresNotFoundWithTxHash(tx_hash.clone()))?,
                )
            }
            scores
        };
        let score_entries: Vec<f32> =
            scores.iter().flat_map(|cs| cs.entries().clone()).map(|x| *x.value()).collect();
        let score_hashes: Vec<Hash> = score_entries
            .iter()
            .map(|&x| hash_leaf::<Keccak256>(x.to_be_bytes().to_vec()))
            .collect();
        let compute_tree =
            DenseMerkleTree::<Keccak256>::new(score_hashes).map_err(Error::Merkle)?;
        compute_tree_map.insert(assignment_id.clone(), compute_tree);

        Ok(())
    }

    /// Get the verification result(True or False) of certain assignment, for certain domain
    pub fn compute_verification(
        &mut self, domain: Domain, assignment_id: TxHash,
    ) -> Result<bool, Error> {
        let commitment = self
            .commitments
            .get(&assignment_id)
            .ok_or(Error::CommitmentNotFound(assignment_id.clone()))?;
        let compute_scores = self
            .compute_scores
            .get(&domain.to_hash())
            .ok_or(Error::ComputeScoresNotFoundWithDomain(domain.to_hash()))?;
        let domain_indices =
            self.indices.get(&domain.to_hash()).ok_or(Error::IndicesNotFound(domain.to_hash()))?;
        let lt = self
            .local_trust
            .get(&domain.trust_namespace())
            .ok_or(Error::LocalTrustNotFound(domain.trust_namespace()))?;
        let seed = self
            .seed_trust
            .get(&domain.seed_namespace())
            .ok_or(Error::SeedTrustNotFound(domain.seed_namespace()))?;
        let scores: Vec<&compute::Scores> = {
            let mut scores = Vec::new();
            for tx_hash in commitment.scores_tx_hashes().iter() {
                scores.push(
                    compute_scores
                        .get(tx_hash)
                        .ok_or(Error::ComputeScoresNotFoundWithTxHash(tx_hash.clone()))?,
                )
            }
            scores
        };
        let score_entries: HashMap<u64, f32> = {
            let score_entries_vec: Vec<ScoreEntry> =
                scores.iter().flat_map(|cs| cs.entries().clone()).collect();

            let mut score_entries_map: HashMap<u64, f32> = HashMap::new();
            for entry in score_entries_vec {
                let i = domain_indices
                    .get(entry.id())
                    .ok_or(Error::DomainIndexNotFound(entry.id().clone()))?;
                score_entries_map.insert(*i, *entry.value());
            }
            score_entries_map
        };
        convergence_check(lt.clone(), seed.clone(), &score_entries).map_err(Error::Algo)
    }

    /// Get the local trust tree root and compute tree root of certain assignment, for certain domain
    pub fn get_root_hashes(
        &self, domain: Domain, assignment_id: TxHash,
    ) -> Result<(Hash, Hash), Error> {
        let lt_tree = self
            .lt_master_tree
            .get(&domain.to_hash())
            .ok_or(Error::LocalTrustMasterTreeNotFound(domain.to_hash()))?;
        let st_tree = self
            .st_master_tree
            .get(&domain.to_hash())
            .ok_or(Error::SeedTrustMasterTreeNotFound(domain.to_hash()))?;
        let compute_tree_map = self
            .compute_tree
            .get(&domain.to_hash())
            .ok_or(Error::ComputeTreeNotFoundWithDomain(domain.to_hash()))?;
        let compute_tree = compute_tree_map
            .get(&assignment_id)
            .ok_or(Error::ComputeTreeNotFoundWithTxHash(assignment_id.clone()))?;
        let lt_tree_root = lt_tree.root().map_err(Error::Merkle)?;
        let st_tree_root = st_tree.root().map_err(Error::Merkle)?;
        let tree_roots = hash_two::<Keccak256>(lt_tree_root, st_tree_root);
        let ct_tree_root = compute_tree.root().map_err(Error::Merkle)?;
        Ok((tree_roots, ct_tree_root))
    }
}

#[derive(Debug)]
pub enum Error {
    IndicesNotFound(DomainHash),
    CountNotFound(DomainHash),

    LocalTrustSubTreesNotFoundWithDomain(DomainHash),
    LocalTrustSubTreesNotFoundWithIndex(u64),

    LocalTrustMasterTreeNotFound(DomainHash),
    SeedTrustMasterTreeNotFound(DomainHash),

    LocalTrustNotFound(OwnedNamespace),

    SeedTrustNotFound(OwnedNamespace),

    ComputeTreeNotFoundWithDomain(DomainHash),
    ComputeTreeNotFoundWithTxHash(TxHash),

    ComputeScoresNotFoundWithDomain(DomainHash),
    ComputeScoresNotFoundWithTxHash(TxHash),

    ActiveAssignmentsNotFound(DomainHash),

    CommitmentNotFound(TxHash),
    DomainIndexNotFound(String),

    Merkle(merkle::Error),
    Algo(algos::Error),
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
            Self::ComputeTreeNotFoundWithDomain(domain) => {
                write!(f, "compute_tree not found for domain: {:?}", domain)
            },
            Self::ComputeTreeNotFoundWithTxHash(tx_hash) => {
                write!(f, "compute_tree not found for tx_hash: {:?}", tx_hash)
            },

            Self::ComputeScoresNotFoundWithDomain(domain) => {
                write!(f, "compute_scores not found for domain: {:?}", domain)
            },
            Self::ComputeScoresNotFoundWithTxHash(tx_hash) => {
                write!(f, "compute_scores not found for tx_hash: {:?}", tx_hash)
            },

            Self::ActiveAssignmentsNotFound(domain) => {
                write!(f, "active_assignments not found for domain: {:?}", domain)
            },
            Self::CommitmentNotFound(assigment_id) => {
                write!(
                    f,
                    "commitment not found for assignment_id: {:?}",
                    assigment_id
                )
            },
            Self::DomainIndexNotFound(address) => {
                write!(f, "domain_indice not found for address: {:?}", address)
            },
            Self::Merkle(err) => err.fmt(f),
            Self::Algo(err) => err.fmt(f),
        }
    }
}
