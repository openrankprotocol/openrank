use openrank_common::{
	algos::et::convergence_check,
	merkle::{
		fixed::DenseMerkleTree, hash_leaf, hash_two, incremental::DenseIncrementalMerkleTree, Hash,
	},
	topics::{Domain, DomainHash},
	txs::{Address, CreateCommitment, CreateScores, ScoreEntry, TrustEntry, TxHash},
};
use sha3::Keccak256;
use std::collections::HashMap;

use crate::error::{
	ComputeTreesError, CreateScoresError, JobRunnerError, LocalTrustSubTreesError,
	VerifierNodeError,
};

pub struct VerificationJobRunner {
	count: HashMap<DomainHash, u32>,
	indices: HashMap<DomainHash, HashMap<Address, u32>>,
	local_trust: HashMap<DomainHash, HashMap<(u32, u32), f32>>,
	seed_trust: HashMap<DomainHash, HashMap<u32, f32>>,
	lt_sub_trees: HashMap<DomainHash, HashMap<u32, DenseIncrementalMerkleTree<Keccak256>>>,
	lt_master_tree: HashMap<DomainHash, DenseIncrementalMerkleTree<Keccak256>>,
	create_scores: HashMap<DomainHash, HashMap<TxHash, CreateScores>>,
	compute_tree: HashMap<DomainHash, HashMap<TxHash, DenseMerkleTree<Keccak256>>>,
	active_assignments: HashMap<DomainHash, Vec<TxHash>>,
	completed_assignments: HashMap<DomainHash, Vec<TxHash>>,
	commitments: HashMap<TxHash, CreateCommitment>,
}

impl VerificationJobRunner {
	pub fn new(domains: Vec<DomainHash>) -> Self {
		let mut count = HashMap::new();
		let mut indices = HashMap::new();
		let mut local_trust = HashMap::new();
		let mut seed_trust = HashMap::new();
		let mut lt_sub_trees = HashMap::new();
		let mut lt_master_tree = HashMap::new();
		let mut compute_results = HashMap::new();
		let mut create_scores = HashMap::new();
		let mut compute_tree = HashMap::new();
		let mut active_assignments = HashMap::new();
		let mut completed_assignments = HashMap::new();
		for domain in domains {
			count.insert(domain.clone(), 0);
			indices.insert(domain.clone(), HashMap::new());
			local_trust.insert(domain.clone(), HashMap::new());
			seed_trust.insert(domain.clone(), HashMap::new());
			lt_sub_trees.insert(domain.clone(), HashMap::new());
			lt_master_tree.insert(
				domain.clone(),
				DenseIncrementalMerkleTree::<Keccak256>::new(32),
			);
			compute_results.insert(domain.clone(), Vec::<f32>::new());
			create_scores.insert(domain.clone(), HashMap::new());
			compute_tree.insert(domain.clone(), HashMap::new());
			active_assignments.insert(domain.clone(), Vec::new());
			completed_assignments.insert(domain, Vec::new());
		}
		Self {
			count,
			indices,
			local_trust,
			seed_trust,
			lt_sub_trees,
			lt_master_tree,
			create_scores,
			compute_tree,
			active_assignments,
			completed_assignments,
			commitments: HashMap::new(),
		}
	}

	pub fn update_trust(
		&mut self, domain: Domain, trust_entries: Vec<TrustEntry>,
	) -> Result<(), VerifierNodeError> {
		let domain_indices = self.indices.get_mut(&domain.to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::IndicesNotFound(
				domain.to_hash(),
			)),
		)?;
		let count = self.count.get_mut(&domain.to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::CountNotFound(
				domain.to_hash(),
			)),
		)?;
		let lt_sub_trees = self.lt_sub_trees.get_mut(&domain.to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::LocalTrustSubTreesNotFound(
				LocalTrustSubTreesError::NotFoundWithDomain(domain.to_hash()),
			)),
		)?;
		let lt_master_tree = self.lt_master_tree.get_mut(&domain.to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::LocalTrustMasterTreeNotFound(
				domain.to_hash(),
			)),
		)?;
		let lt = self.local_trust.get_mut(&domain.to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::LocalTrustNotFound(
				domain.to_hash(),
			)),
		)?;
		let seed = self.seed_trust.get(&domain.to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::SeedTrustNotFound(
				domain.to_hash(),
			)),
		)?;
		let default_sub_tree = DenseIncrementalMerkleTree::<Keccak256>::new(32);
		for entry in trust_entries {
			let from_index = if let Some(i) = domain_indices.get(&entry.from) {
				*i
			} else {
				let curr_count = count.clone();
				domain_indices.insert(entry.from.clone(), curr_count);
				*count += 1;
				curr_count
			};
			let to_index = if let Some(i) = domain_indices.get(&entry.to) {
				*i
			} else {
				let curr_count = count.clone();
				domain_indices.insert(entry.to.clone(), curr_count);
				*count += 1;
				curr_count
			};
			let old_value = lt.get(&(from_index, to_index)).unwrap_or(&0.0);
			lt.insert((from_index, to_index), entry.value + old_value);

			if !lt_sub_trees.contains_key(&from_index) {
				lt_sub_trees.insert(from_index, default_sub_tree.clone());
			}
			let sub_tree = lt_sub_trees.get_mut(&from_index).ok_or(
				VerifierNodeError::ComputeInternalError(
					JobRunnerError::LocalTrustSubTreesNotFound(
						LocalTrustSubTreesError::NotFoundWithIndex(from_index),
					),
				),
			)?;
			let leaf = hash_leaf::<Keccak256>(entry.value.to_be_bytes().to_vec());
			sub_tree.insert_leaf(to_index, leaf);

			let sub_tree_root =
				sub_tree.root().map_err(|e| VerifierNodeError::ComputeMerkleError(e))?;
			let seed_value = seed.get(&to_index).unwrap_or(&0.0);
			let seed_hash = hash_leaf::<Keccak256>(seed_value.to_be_bytes().to_vec());
			let leaf = hash_two::<Keccak256>(sub_tree_root, seed_hash);
			lt_master_tree.insert_leaf(from_index, leaf);
		}

		Ok(())
	}

	pub fn update_seed(
		&mut self, domain: Domain, seed_entries: Vec<ScoreEntry>,
	) -> Result<(), VerifierNodeError> {
		let domain_indices = self.indices.get_mut(&domain.to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::IndicesNotFound(
				domain.to_hash(),
			)),
		)?;
		let count = self.count.get_mut(&domain.to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::CountNotFound(
				domain.to_hash(),
			)),
		)?;
		let lt_sub_trees = self.lt_sub_trees.get_mut(&domain.to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::LocalTrustSubTreesNotFound(
				LocalTrustSubTreesError::NotFoundWithDomain(domain.to_hash()),
			)),
		)?;
		let lt_master_tree = self.lt_master_tree.get_mut(&domain.to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::LocalTrustMasterTreeNotFound(
				domain.to_hash(),
			)),
		)?;
		let seed = self.seed_trust.get_mut(&domain.to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::SeedTrustNotFound(
				domain.to_hash(),
			)),
		)?;
		let default_sub_tree = DenseIncrementalMerkleTree::<Keccak256>::new(32);
		for entry in seed_entries {
			let index = if let Some(i) = domain_indices.get(&entry.id) {
				*i
			} else {
				let curr_count = count.clone();
				domain_indices.insert(entry.id.clone(), curr_count);
				*count += 1;
				curr_count
			};

			if !lt_sub_trees.contains_key(&index) {
				lt_sub_trees.insert(index, default_sub_tree.clone());
			}
			let sub_tree =
				lt_sub_trees.get_mut(&index).ok_or(VerifierNodeError::ComputeInternalError(
					JobRunnerError::LocalTrustSubTreesNotFound(
						LocalTrustSubTreesError::NotFoundWithIndex(index),
					),
				))?;
			let sub_tree_root =
				sub_tree.root().map_err(|e| VerifierNodeError::ComputeMerkleError(e))?;
			let seed_hash = hash_leaf::<Keccak256>(entry.value.to_be_bytes().to_vec());
			let leaf = hash_two::<Keccak256>(sub_tree_root, seed_hash);
			lt_master_tree.insert_leaf(index, leaf);

			seed.insert(index, entry.value);
		}

		Ok(())
	}

	pub fn check_scores_tx_hashes(
		&self, domain: Domain, commitment: CreateCommitment,
	) -> Result<bool, VerifierNodeError> {
		let create_scores_txs = self.create_scores.get(&domain.clone().to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::CreateScoresNotFound(
				CreateScoresError::NotFoundWithDomain(domain.to_hash()),
			)),
		)?;
		for score_tx in commitment.scores_tx_hashes {
			let res = create_scores_txs.contains_key(&score_tx);
			if !res {
				return Ok(false);
			}
		}
		Ok(true)
	}

	pub fn check_finished_jobs(
		&mut self, domain: Domain,
	) -> Result<Vec<(TxHash, bool)>, VerifierNodeError> {
		let assignments = self.active_assignments.get(&domain.clone().to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::ActiveAssignmentsNotFound(
				domain.to_hash(),
			)),
		)?;
		let mut results = Vec::new();
		let mut completed = Vec::new();
		for assignment_id in assignments.clone().into_iter() {
			if let Some(commitment) = self.commitments.get(&assignment_id.clone()) {
				let is_check_score_tx_hashes =
					self.check_scores_tx_hashes(domain.clone(), commitment.clone())?;
				if is_check_score_tx_hashes {
					let assgn_tx = assignment_id.clone();
					let lt_root = commitment.lt_root_hash.clone();
					let cp_root = commitment.compute_root_hash.clone();

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
		let active_assignments = self.active_assignments.get_mut(&domain.clone().to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::ActiveAssignmentsNotFound(
				domain.to_hash(),
			)),
		)?;
		let completed_assignments = self
			.completed_assignments
			.get_mut(&domain.clone().to_hash())
			.ok_or(VerifierNodeError::ComputeInternalError(
			JobRunnerError::CompletedAssignmentsNotFound(domain.to_hash()),
		))?;
		active_assignments.retain(|x| !completed.contains(x));
		completed_assignments.extend(completed);
		Ok(results)
	}

	pub fn update_scores(
		&mut self, domain: Domain, tx_hash: TxHash, create_scores: CreateScores,
	) -> Result<(), VerifierNodeError> {
		let score_values = self.create_scores.get_mut(&domain.clone().to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::CreateScoresNotFound(
				CreateScoresError::NotFoundWithDomain(domain.to_hash()),
			)),
		)?;
		score_values.insert(tx_hash, create_scores);
		Ok(())
	}

	pub fn update_assigment(
		&mut self, domain: Domain, job_run_assignment_tx_hash: TxHash,
	) -> Result<(), VerifierNodeError> {
		let completed_assignments = self
			.completed_assignments
			.get(&domain.clone().to_hash())
			.ok_or(VerifierNodeError::ComputeInternalError(
				JobRunnerError::CompletedAssignmentsNotFound(domain.to_hash()),
			))?;
		if !completed_assignments.contains(&job_run_assignment_tx_hash) {
			let active_assignments = self.active_assignments.get_mut(&domain.to_hash()).ok_or(
				VerifierNodeError::ComputeInternalError(JobRunnerError::ActiveAssignmentsNotFound(
					domain.to_hash(),
				)),
			)?;
			active_assignments.push(job_run_assignment_tx_hash);
		}
		Ok(())
	}

	pub fn update_commitment(&mut self, commitment: CreateCommitment) {
		self.commitments.insert(
			commitment.job_run_assignment_tx_hash.clone(),
			commitment.clone(),
		);
	}

	pub fn create_compute_tree(
		&mut self, domain: Domain, assignment_id: TxHash,
	) -> Result<(), VerifierNodeError> {
		let compute_tree_map = self.compute_tree.get_mut(&domain.to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::ComputeTreeNotFound(
				ComputeTreesError::NotFoundWithDomain(domain.to_hash()),
			)),
		)?;
		let commitment =
			self.commitments.get(&assignment_id).ok_or(VerifierNodeError::ComputeInternalError(
				JobRunnerError::CommitmentNotFound(assignment_id.clone()),
			))?;
		let create_scores = self.create_scores.get(&domain.to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::CreateScoresNotFound(
				CreateScoresError::NotFoundWithDomain(domain.to_hash()),
			)),
		)?;
		let scores: Vec<&CreateScores> = {
			let mut scores = Vec::new();
			for tx_hash in commitment.scores_tx_hashes.iter() {
				scores.push(create_scores.get(&tx_hash).ok_or(
					VerifierNodeError::ComputeInternalError(JobRunnerError::CreateScoresNotFound(
						CreateScoresError::NotFoundWithTxHash(tx_hash.clone()),
					)),
				)?)
			}
			scores
		};
		let score_entries: Vec<f32> =
			scores.iter().map(|cs| cs.entries.clone()).flatten().map(|x| x.value).collect();
		let score_hashes: Vec<Hash> = score_entries
			.iter()
			.map(|&x| hash_leaf::<Keccak256>(x.to_be_bytes().to_vec()))
			.collect();
		let compute_tree = DenseMerkleTree::<Keccak256>::new(score_hashes)
			.map_err(|e| VerifierNodeError::ComputeMerkleError(e))?;
		compute_tree_map.insert(assignment_id.clone(), compute_tree);

		Ok(())
	}

	pub fn compute_verification(
		&mut self, domain: Domain, assignment_id: TxHash,
	) -> Result<bool, VerifierNodeError> {
		let commitment =
			self.commitments.get(&assignment_id).ok_or(VerifierNodeError::ComputeInternalError(
				JobRunnerError::CommitmentNotFound(assignment_id.clone()),
			))?;
		let create_scores = self.create_scores.get(&domain.to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::CreateScoresNotFound(
				CreateScoresError::NotFoundWithDomain(domain.to_hash()),
			)),
		)?;
		let domain_indices =
			self.indices.get(&domain.to_hash()).ok_or(VerifierNodeError::ComputeInternalError(
				JobRunnerError::IndicesNotFound(domain.to_hash()),
			))?;
		let lt = self.local_trust.get(&domain.to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::LocalTrustNotFound(
				domain.to_hash(),
			)),
		)?;
		let seed = self.seed_trust.get(&domain.to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::SeedTrustNotFound(
				domain.to_hash(),
			)),
		)?;
		let scores: Vec<&CreateScores> = {
			let mut scores = Vec::new();
			for tx_hash in commitment.scores_tx_hashes.iter() {
				scores.push(create_scores.get(&tx_hash).ok_or(
					VerifierNodeError::ComputeInternalError(JobRunnerError::CreateScoresNotFound(
						CreateScoresError::NotFoundWithTxHash(tx_hash.clone()),
					)),
				)?)
			}
			scores
		};
		let score_entries: HashMap<u32, f32> = {
			let score_entries_vec: Vec<ScoreEntry> =
				scores.iter().map(|cs| cs.entries.clone()).flatten().collect();

			let mut score_entries_map: HashMap<u32, f32> = HashMap::new();
			for entry in score_entries_vec {
				let i = domain_indices.get(&entry.id).ok_or(
					VerifierNodeError::ComputeInternalError(JobRunnerError::DomainIndiceNotFound(
						entry.id.clone(),
					)),
				)?;
				score_entries_map.insert(*i, entry.value);
			}
			score_entries_map
		};
		convergence_check(lt.clone(), seed, &score_entries)
			.map_err(|e| VerifierNodeError::ComputeAlgoError(e))
	}

	pub fn get_root_hashes(
		&self, domain: Domain, assignment_id: TxHash,
	) -> Result<(Hash, Hash), VerifierNodeError> {
		let lt_tree = self.lt_master_tree.get(&domain.to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::LocalTrustMasterTreeNotFound(
				domain.to_hash(),
			)),
		)?;
		let compute_tree_map = self.compute_tree.get(&domain.to_hash()).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::ComputeTreeNotFound(
				ComputeTreesError::NotFoundWithDomain(domain.to_hash()),
			)),
		)?;
		let compute_tree = compute_tree_map.get(&assignment_id).ok_or(
			VerifierNodeError::ComputeInternalError(JobRunnerError::ComputeTreeNotFound(
				ComputeTreesError::NotFoundWithTxHash(assignment_id.clone()),
			)),
		)?;
		let lt_tree_root = lt_tree.root().map_err(|e| VerifierNodeError::ComputeMerkleError(e))?;
		let ct_tree_root =
			compute_tree.root().map_err(|e| VerifierNodeError::ComputeMerkleError(e))?;
		Ok((lt_tree_root, ct_tree_root))
	}
}
