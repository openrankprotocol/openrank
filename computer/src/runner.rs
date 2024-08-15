use openrank_common::{
	algos::et::positive_run,
	merkle::{
		fixed::DenseMerkleTree, hash_leaf, hash_two, incremental::DenseIncrementalMerkleTree, Hash,
	},
	topics::{Domain, DomainHash},
	txs::{Address, CreateScores, ScoreEntry, TrustEntry},
};
use sha3::Keccak256;
use std::collections::HashMap;

pub struct ComputeJobRunner {
	count: HashMap<DomainHash, u32>,
	indices: HashMap<DomainHash, HashMap<Address, u32>>,
	local_trust: HashMap<DomainHash, HashMap<(u32, u32), f32>>,
	seed_trust: HashMap<DomainHash, HashMap<u32, f32>>,
	lt_sub_trees: HashMap<DomainHash, HashMap<u32, DenseIncrementalMerkleTree<Keccak256>>>,
	lt_master_tree: HashMap<DomainHash, DenseIncrementalMerkleTree<Keccak256>>,
	compute_results: HashMap<DomainHash, Vec<(u32, f32)>>,
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
			compute_results.insert(domain, Vec::<(u32, f32)>::new());
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
				domain_indices.insert(entry.to.clone(), count.clone());
				*count += 1;
				curr_count
			};
			let old_value = lt.get(&(from_index, to_index)).unwrap_or(&0.0);
			lt.insert((from_index, to_index), entry.value + old_value);

			if !lt_sub_trees.contains_key(&from_index) {
				lt_sub_trees.insert(from_index, default_sub_tree.clone());
			}
			let sub_tree = lt_sub_trees.get_mut(&from_index).unwrap();
			let leaf = hash_leaf::<Keccak256>(entry.value.to_be_bytes().to_vec());
			sub_tree.insert_leaf(to_index, leaf);

			let sub_tree_root = sub_tree.root();
			let seed_value = seed.get(&to_index).unwrap_or(&0.0);
			let seed_hash = hash_leaf::<Keccak256>(seed_value.to_be_bytes().to_vec());
			let leaf = hash_two::<Keccak256>(sub_tree_root, seed_hash);
			lt_master_tree.insert_leaf(from_index, leaf);
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
				let curr_count = count.clone();
				domain_indices.insert(entry.id.clone(), curr_count);
				*count += 1;
				curr_count
			};

			if !lt_sub_trees.contains_key(&index) {
				lt_sub_trees.insert(index, default_sub_tree.clone());
			}
			let sub_tree = lt_sub_trees.get_mut(&index).unwrap();
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
		let res = match positive_run::<20>(lt.clone(), seed.clone()) {
			Ok(r) => r,
			Err(e) => {
				eprintln!("Positive run Error: {:?}", e);
				todo!("Error handling")
			},
		};
		self.compute_results.insert(domain.to_hash(), res);
	}

	pub fn create_compute_tree(&mut self, domain: Domain) {
		let scores = self.compute_results.get(&domain.to_hash()).unwrap();
		let score_hashes: Vec<Hash> =
			scores.iter().map(|(_, x)| hash_leaf::<Keccak256>(x.to_be_bytes().to_vec())).collect();
		let compute_tree = DenseMerkleTree::<Keccak256>::new(score_hashes);
		self.compute_tree.insert(domain.to_hash(), compute_tree);
	}

	pub fn get_create_scores(&self, domain: Domain) -> Vec<CreateScores> {
		let domain_indices = self.indices.get(&domain.to_hash()).unwrap();
		let scores = self.compute_results.get(&domain.to_hash()).unwrap();
		let index_to_address: HashMap<&u32, &Address> =
			domain_indices.iter().map(|(k, v)| (v, k)).collect();
		let mut create_scores_txs = Vec::new();
		for chunk in scores.chunks(1000) {
			let mut entries = Vec::new();
			for (index, val) in chunk {
				let address = index_to_address.get(&index).unwrap();
				let score_entry = ScoreEntry::new((*address).clone(), *val);
				entries.push(score_entry);
			}
			let create_scores = CreateScores::new(entries);
			create_scores_txs.push(create_scores);
		}
		create_scores_txs
	}

	pub fn get_root_hashes(&self, domain: Domain) -> (Hash, Hash) {
		let lt_tree = self.lt_master_tree.get(&domain.to_hash()).unwrap();
		let compute_tree = self.compute_tree.get(&domain.to_hash()).unwrap();
		(lt_tree.root(), compute_tree.root())
	}
}
