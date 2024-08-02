use openrank_common::{
	merkle::{
		fixed::DenseMerkleTree, hash_leaf, hash_two, incremental::DenseIncrementalMerkleTree, Hash,
	},
	topics::{Domain, DomainHash},
	txs::{Address, CreateScores, ScoreEntry, TrustEntry},
};
use sha3::Keccak256;
use std::collections::HashMap;

use crate::algos::et::positive_run;

pub struct ComputeJobRunner {
	count: HashMap<DomainHash, u32>,
	indices: HashMap<DomainHash, HashMap<Address, u32>>,
	local_trust: HashMap<DomainHash, HashMap<(u32, u32), f32>>,
	seed_trust: HashMap<DomainHash, HashMap<u32, f32>>,
	lt_sub_trees: HashMap<DomainHash, HashMap<u32, DenseIncrementalMerkleTree<Keccak256>>>,
	lt_master_tree: HashMap<DomainHash, DenseIncrementalMerkleTree<Keccak256>>,
	compute_results: HashMap<DomainHash, Vec<f32>>,
	compute_tree: HashMap<DomainHash, DenseMerkleTree<Keccak256>>,
}

impl ComputeJobRunner {
	pub fn new(domains: Vec<DomainHash>) -> Self {
		let mut count = HashMap::new();
		let mut indices = HashMap::new();
		let mut local_trust = HashMap::new();
		let mut seed_trust = HashMap::new();
		let mut lt_sub_trees = HashMap::new();
		let mut lt_master_tree = HashMap::new();
		let mut compute_results = HashMap::new();
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
			compute_results.insert(domain, Vec::<f32>::new());
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

	pub fn update_trust(&mut self, domain: Domain, trust_entries: Vec<TrustEntry>) {
		let domain_indices = self.indices.get_mut(&domain.to_hash()).unwrap();
		let count = self.count.get_mut(&domain.to_hash()).unwrap();
		let lt_sub_trees = self.lt_sub_trees.get_mut(&domain.to_hash()).unwrap();
		let lt_master_tree = self.lt_master_tree.get_mut(&domain.to_hash()).unwrap();
		let lt = self.local_trust.get_mut(&domain.to_hash()).unwrap();
		let seed = self.seed_trust.get(&domain.to_hash()).unwrap();
		for entry in trust_entries {
			let from_index = if let Some(i) = domain_indices.get(&entry.from) {
				*i
			} else {
				domain_indices.insert(entry.from.clone(), *count);
				*count += 1;
				*count
			};
			let to_index = if let Some(i) = domain_indices.get(&entry.to) {
				*i
			} else {
				domain_indices.insert(entry.to.clone(), *count);
				*count += 1;
				*count
			};
			let old_value = lt.get(&(from_index, to_index)).unwrap_or(&0.0);
			lt.insert((from_index, to_index), entry.value + old_value);

			let from_index = domain_indices.get(&entry.from).unwrap();
			let to_index = domain_indices.get(&entry.to).unwrap();
			let sub_tree = lt_sub_trees
				.entry(*from_index)
				.or_insert(DenseIncrementalMerkleTree::<Keccak256>::new(32));
			let leaf = hash_leaf::<Keccak256>(entry.value.to_be_bytes().to_vec());
			sub_tree.insert_leaf(*to_index, leaf);

			let sub_tree_root = sub_tree.root();
			let seed_value = seed.get(&to_index).unwrap_or(&0.0);
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

		let default_sub_tree = DenseIncrementalMerkleTree::<Keccak256>::new(32);
		for entry in seed_entries {
			let index = if let Some(i) = domain_indices.get(&entry.id) {
				*i
			} else {
				domain_indices.insert(entry.id, *count);
				*count += 1;
				*count
			};

			let sub_tree = lt_sub_trees.get(&index).unwrap_or(&default_sub_tree);
			let sub_tree_root = sub_tree.root();
			let seed_hash = hash_leaf::<Keccak256>(entry.value.to_be_bytes().to_vec());
			let leaf = hash_two::<Keccak256>(sub_tree_root, seed_hash);
			lt_master_tree.insert_leaf(index, leaf);

			seed.insert(index, entry.value);
		}
	}

	pub fn compute(&mut self, domain: Domain) {
		let lt = self.local_trust.get(&domain.to_hash()).unwrap();
		let seed = self.seed_trust.get(&domain.to_hash()).unwrap();
		let res = positive_run::<10>(lt.clone(), seed.clone());
		println!("res: {}", res.iter().sum::<f32>());
		self.compute_results.insert(domain.to_hash(), res);
	}

	pub fn create_compute_tree(&mut self, domain: Domain) {
		let scores = self.compute_results.get(&domain.to_hash()).unwrap();
		let score_hashes: Vec<Hash> =
			scores.iter().map(|x| hash_leaf::<Keccak256>(x.to_be_bytes().to_vec())).collect();
		let compute_tree = DenseMerkleTree::<Keccak256>::new(score_hashes);
		self.compute_tree.insert(domain.to_hash(), compute_tree);
	}

	pub fn get_create_scores(&self, domain: Domain) -> Vec<CreateScores> {
		let namespace_indices = self.indices.get(&domain.to_hash()).unwrap();
		let scores = self.compute_results.get(&domain.to_hash()).unwrap();
		println!("scores: {}", scores.iter().sum::<f32>());
		let index_to_address: HashMap<&u32, &Address> =
			namespace_indices.iter().map(|(k, v)| (v, k)).collect();
		let mut create_scores_txs = Vec::new();
		for (i, chunk) in scores.chunks(1000).enumerate() {
			let mut entries = Vec::new();
			for j in 0..chunk.len() {
				let index = (i * chunk.len() + j) as u32;
				let address = index_to_address.get(&index).unwrap();
				let score_entry = ScoreEntry::new((*address).clone(), chunk[j]);
				entries.push(score_entry);
			}
			let create_scores = CreateScores::new(entries);
			create_scores_txs.push(create_scores);
		}
		let mut score_entries: Vec<ScoreEntry> =
			create_scores_txs.clone().into_iter().map(|x| x.entries).flatten().collect();
		// score_entries.sort_by(|a, b| a.value.partial_cmp(&b.value).unwrap());
		let sum: f32 = score_entries.iter().map(|x| x.value).sum();
		println!("sum: {}", sum);
		create_scores_txs
	}

	pub fn get_root_hashes(&self, domain: Domain) -> (Hash, Hash) {
		let lt_tree = self.lt_master_tree.get(&domain.to_hash()).unwrap();
		let compute_tree = self.compute_tree.get(&domain.to_hash()).unwrap();
		(lt_tree.root(), compute_tree.root())
	}
}
