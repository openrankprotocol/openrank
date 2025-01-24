use crate::{
    algos::et::convergence_check,
    merkle::{self, fixed::DenseMerkleTree, hash_leaf, Hash},
    topics::{Domain, DomainHash},
    tx::{
        compute,
        trust::{ScoreEntry, TrustEntry},
        TxHash,
    },
};
use getset::Getters;
use sha3::Keccak256;
use std::collections::HashMap;
use tracing::info;

use super::{BaseRunner, Error as BaseError};

#[derive(Getters)]
#[getset(get = "pub")]
/// Struct containing the state of the verification runner
pub struct VerificationRunner {
    base: BaseRunner,
    compute_scores: HashMap<DomainHash, HashMap<TxHash, compute::Scores>>,
    compute_tree: HashMap<DomainHash, HashMap<TxHash, DenseMerkleTree<Keccak256>>>,
    active_assignments: HashMap<DomainHash, Vec<TxHash>>,
    commitments: HashMap<TxHash, compute::Commitment>,
}

impl VerificationRunner {
    pub fn new(domains: &[Domain]) -> Self {
        let base = BaseRunner::new(domains);
        let mut compute_scores = HashMap::new();
        let mut compute_tree = HashMap::new();
        let mut active_assignments = HashMap::new();
        for domain in domains {
            let domain_hash = domain.to_hash();
            compute_scores.insert(domain_hash, HashMap::new());
            compute_tree.insert(domain_hash, HashMap::new());
            active_assignments.insert(domain_hash, Vec::new());
        }
        Self {
            base,
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
        self.base.update_trust(domain, trust_entries).map_err(Error::Base)
    }

    /// Update the state of trees for certain domain, with the given seed entries
    pub fn update_seed(
        &mut self, domain: Domain, seed_entries: Vec<ScoreEntry>,
    ) -> Result<(), Error> {
        self.base.update_seed(domain, seed_entries).map_err(Error::Base)
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
        info!("COMPLETED_ASSIGNMENT_SEARCH: {}", domain.to_hash());
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
                    let cp_root = commitment.compute_root_hash().clone();

                    self.create_compute_tree(domain.clone(), assignment_id.clone())?;
                    let (res_lt_root, res_compute_root) =
                        self.get_root_hashes(domain.clone(), assignment_id.clone())?;
                    info!(
                        "LT_ROOT: {}, COMPUTE_ROOT: {}",
                        res_lt_root, res_compute_root
                    );
                    let is_root_equal = cp_root == res_compute_root;
                    let is_converged =
                        self.compute_verification(domain.clone(), assignment_id.clone())?;
                    results.push((assgn_tx.clone(), is_root_equal && is_converged));
                    completed.push(assignment_id.clone());
                    info!(
                        "COMPLETED_ASSIGNMENT, DOMAIN: {}, is_root_equal: {}, is_converged: {}",
                        domain.to_hash(),
                        is_root_equal,
                        is_converged,
                    );
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
        info!("CREATE_COMPUTE_TREE: {}", domain.to_hash());
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
        info!(
            "COMPUTE_TREE_ROOT_HASH: {}",
            compute_tree.root().map_err(Error::Merkle)?
        );
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
        let domain_indices = self
            .base
            .indices
            .get(&domain.to_hash())
            .ok_or::<Error>(BaseError::IndicesNotFound(domain.to_hash()).into())?;
        let lt = self
            .base
            .local_trust
            .get(&domain.trust_namespace())
            .ok_or::<Error>(BaseError::LocalTrustNotFound(domain.trust_namespace()).into())?;
        let count = self
            .base
            .count
            .get(&domain.to_hash())
            .ok_or::<Error>(BaseError::CountNotFound(domain.to_hash()).into())?;
        let seed = self
            .base
            .seed_trust
            .get(&domain.seed_namespace())
            .ok_or::<Error>(BaseError::SeedTrustNotFound(domain.seed_namespace()).into())?;
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
        Ok(convergence_check(
            lt.clone(),
            seed.clone(),
            &score_entries,
            *count,
        ))
    }

    /// Get the local trust tree root and compute tree root of certain assignment, for certain domain
    pub fn get_root_hashes(
        &self, domain: Domain, assignment_id: TxHash,
    ) -> Result<(Hash, Hash), Error> {
        let tree_roots = self.base.get_base_root_hashes(&domain)?;

        let compute_tree_map = self
            .compute_tree
            .get(&domain.to_hash())
            .ok_or(Error::ComputeTreeNotFoundWithDomain(domain.to_hash()))?;
        let compute_tree = compute_tree_map
            .get(&assignment_id)
            .ok_or(Error::ComputeTreeNotFoundWithTxHash(assignment_id.clone()))?;
        let ct_tree_root = compute_tree.root().map_err(Error::Merkle)?;

        Ok((tree_roots, ct_tree_root))
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Base(BaseError),
    #[error("compute_tree not found for domain: {0}")]
    ComputeTreeNotFoundWithDomain(DomainHash),
    #[error("compute_scores not found for domain: {0}")]
    ComputeTreeNotFoundWithTxHash(TxHash),
    #[error("compute_scores not found for domain: {0}")]
    ComputeScoresNotFoundWithDomain(DomainHash),
    #[error("compute_scores not found for tx_hash: {0}")]
    ComputeScoresNotFoundWithTxHash(TxHash),
    #[error("active_assignments not found for domain: {0}")]
    ActiveAssignmentsNotFound(DomainHash),
    #[error("commitment not found for assignment_id: {0}")]
    CommitmentNotFound(TxHash),
    #[error("domain_indice not found for address: {0}")]
    DomainIndexNotFound(String),
    #[error("{0}")]
    Merkle(merkle::Error),
}

impl From<BaseError> for Error {
    fn from(err: BaseError) -> Self {
        Self::Base(err)
    }
}
