use openrank_common::{
	merkle::{
		fixed::DenseMerkleTree, hash_leaf, hash_two, incremental::DenseIncrementalMerkleTree, Hash,
	},
	topics::{Domain, DomainHash},
	txs::{Address, CreateCommitment, CreateScores, ScoreEntry, TrustEntry, TxHash},
};
use sha3::Keccak256;
use std::collections::HashMap;

pub struct VerificationJobRunner {
	count: HashMap<DomainHash, u32>,
	indices: HashMap<DomainHash, HashMap<Address, u32>>,
	local_trust: HashMap<DomainHash, HashMap<u32, HashMap<u32, f32>>>,
	seed_trust: HashMap<DomainHash, HashMap<u32, f32>>,
	lt_sub_trees: HashMap<DomainHash, HashMap<u32, DenseIncrementalMerkleTree<Keccak256>>>,
	lt_master_tree: HashMap<DomainHash, DenseIncrementalMerkleTree<Keccak256>>,
	create_scores: HashMap<DomainHash, Vec<(TxHash, CreateScores)>>,
	compute_tree: HashMap<DomainHash, DenseMerkleTree<Keccak256>>,
	active_assignments: HashMap<DomainHash, Option<TxHash>>,
	commitments: HashMap<TxHash, CreateCommitment>,
}

impl VerificationJobRunner {
	pub fn new() -> Self {
		Self {
			count: HashMap::new(),
			indices: HashMap::new(),
			local_trust: HashMap::new(),
			seed_trust: HashMap::new(),
			lt_sub_trees: HashMap::new(),
			lt_master_tree: HashMap::new(),
			compute_tree: HashMap::new(),
			create_scores: HashMap::new(),
			active_assignments: HashMap::new(),
			commitments: HashMap::new(),
		}
	}

	pub fn update_trust(&mut self, domain: Domain, trust_entries: Vec<TrustEntry>) {
		let domain_indices = self.indices.get_mut(&domain.to_hash()).unwrap();
		let count = self.count.get_mut(&domain.to_hash()).unwrap();
		let lt_sub_trees = self.lt_sub_trees.get_mut(&domain.to_hash()).unwrap();
		let lt_master_tree = self.lt_master_tree.get_mut(&domain.to_hash()).unwrap();
		let lt = self.local_trust.get_mut(&domain.to_hash()).unwrap();
		let seed = self.seed_trust.get_mut(&domain.to_hash()).unwrap();
		for entry in trust_entries {
			let from_index = if let Some(i) = domain_indices.get(&entry.from) {
				*i
			} else {
				*count += 1;
				domain_indices.insert(entry.from.clone(), *count);
				*count
			};
			let to_index = if let Some(i) = domain_indices.get(&entry.to) {
				*i
			} else {
				*count += 1;
				domain_indices.insert(entry.to.clone(), *count);
				*count
			};
			lt.entry(from_index.clone()).and_modify(|e| {
				e.insert(to_index, entry.value);
			});

			let from_index = domain_indices.get(&entry.from).unwrap();
			let to_index = domain_indices.get(&entry.to).unwrap();
			let sub_tree = lt_sub_trees.get_mut(from_index).unwrap();
			let leaf = hash_leaf::<Keccak256>(entry.value.to_be_bytes().to_vec());
			sub_tree.insert_leaf(*to_index, leaf);

			let sub_tree_root = sub_tree.root();
			let seed_value = seed.get_mut(&to_index).unwrap();
			let seed_hash = hash_leaf::<Keccak256>(seed_value.to_be_bytes().to_vec());
			let leaf = hash_two::<Keccak256>(sub_tree_root, seed_hash);
			lt_master_tree.insert_leaf(*from_index, leaf);
		}
	}

	pub fn update_seed(&mut self, domain: Domain, seed_entries: Vec<ScoreEntry>) {
		let domain_indices = self.indices.get_mut(&domain.to_hash()).unwrap();
		let count = self.count.get_mut(&domain.to_hash()).unwrap();
		let lt_sub_trees = self.lt_sub_trees.get_mut(&domain.to_hash()).unwrap();
		let lt_master_tree = self.lt_master_tree.get_mut(&domain.to_hash()).unwrap();
		let seed = self.seed_trust.get_mut(&domain.to_hash()).unwrap();
		for entry in seed_entries {
			let index = if let Some(i) = domain_indices.get(&entry.id) {
				*i
			} else {
				*count += 1;
				domain_indices.insert(entry.id, *count);
				*count
			};
			seed.insert(index, entry.value);

			let sub_tree = lt_sub_trees.get(&index).unwrap();
			let sub_tree_root = sub_tree.root();
			let seed_hash = hash_leaf::<Keccak256>(entry.value.to_be_bytes().to_vec());
			let leaf = hash_two::<Keccak256>(sub_tree_root, seed_hash);
			lt_master_tree.insert_leaf(index, leaf);
		}
	}

	pub fn update_scores(
		&mut self, domain: Domain, tx_hash: TxHash, create_scores: CreateScores,
	) -> Option<(TxHash, bool)> {
		let score_values = self.create_scores.get_mut(&domain.to_hash()).unwrap();
		score_values.push((tx_hash, create_scores));

		let assignment = self.active_assignments.get_mut(&domain.to_hash()).unwrap();
		if let Some(assignment_id) = assignment {
			let commitment = self.commitments.get(&assignment_id.clone()).unwrap();
			let create_scores_txs = self.create_scores.get(&domain.to_hash()).unwrap();
			let scores_tx_hashes: Vec<TxHash> =
				create_scores_txs.iter().map(|(tx_hash, _)| tx_hash.clone()).collect();
			if commitment.scores_tx_hashes == scores_tx_hashes {
				let assgn_tx = assignment_id.clone();
				let lt_hash = commitment.lt_root_hash.clone();
				let cp_hash = commitment.compute_root_hash.clone();

				self.create_compute_tree(domain.clone());
				let (lt_root, compute_root) = self.get_root_hashes(domain);
				Some((assgn_tx, lt_hash == lt_root && cp_hash == compute_root))
			} else {
				None
			}
		} else {
			None
		}
	}

	pub fn update_assigment(&mut self, domain: Domain, id: TxHash) {
		self.active_assignments.insert(domain.to_hash(), Some(id));
	}

	pub fn update_commitment(
		&mut self, domain: Domain, commitment: CreateCommitment,
	) -> Option<(TxHash, bool)> {
		self.commitments.insert(
			commitment.job_run_assignment_tx_hash.clone(),
			commitment.clone(),
		);
		let assignment = self.active_assignments.get_mut(&domain.to_hash()).unwrap();
		if let Some(assignment_id) = assignment {
			let commitment = self.commitments.get(&assignment_id.clone()).unwrap();
			let create_scores_txs = self.create_scores.get(&domain.to_hash()).unwrap();
			let scores_tx_hashes: Vec<TxHash> =
				create_scores_txs.iter().map(|(tx_hash, _)| tx_hash.clone()).collect();
			if commitment.scores_tx_hashes == scores_tx_hashes {
				let assgn_tx = assignment_id.clone();
				let lt_hash = commitment.lt_root_hash.clone();
				let cp_hash = commitment.compute_root_hash.clone();

				self.create_compute_tree(domain.clone());
				let (lt_root, compute_root) = self.get_root_hashes(domain);
				Some((assgn_tx, lt_hash == lt_root && cp_hash == compute_root))
			} else {
				None
			}
		} else {
			None
		}
	}

	pub fn create_compute_tree(&mut self, domain: Domain) {
		let scores = self.create_scores.get(&domain.to_hash()).unwrap();
		let score_entries: Vec<ScoreEntry> =
			scores.iter().fold(Vec::new(), |mut acc, (_, scores)| {
				acc.extend_from_slice(&scores.entries);
				acc
			});
		let indices = self.indices.get(&domain.to_hash()).unwrap();
		let count = self.count.get(&domain.to_hash()).unwrap();
		let mut scores = vec![0.0; *count as usize];
		for score in score_entries {
			let index = indices.get(&score.id).unwrap();
			scores[*index as usize] = score.value;
		}
		let score_hashes: Vec<Hash> =
			scores.iter().map(|x| hash_leaf::<Keccak256>(x.to_be_bytes().to_vec())).collect();
		let compute_tree = DenseMerkleTree::<Keccak256>::new(score_hashes);
		self.compute_tree.insert(domain.to_hash(), compute_tree);
	}

	pub fn get_root_hashes(&self, domain: Domain) -> (Hash, Hash) {
		let lt_tree = self.lt_master_tree.get(&domain.to_hash()).unwrap();
		let compute_tree = self.compute_tree.get(&domain.to_hash()).unwrap();
		(lt_tree.root(), compute_tree.root())
	}
}
