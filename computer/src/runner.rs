use crate::algo::positive_run;
use openrank_common::{
	topics::Domain,
	txs::{Address, CreateScores, OwnedNamespace, ScoreEntry, SeedUpdate, TrustUpdate},
};
use std::collections::HashMap;

struct ComputeJobRunner {
	count: u32,
	indices: HashMap<Address, u32>,
	local_trust: HashMap<OwnedNamespace, HashMap<u32, HashMap<u32, f32>>>,
	seed_trust: HashMap<OwnedNamespace, HashMap<u32, f32>>,
}

impl ComputeJobRunner {
	pub fn update_trust(&mut self, trust_update: TrustUpdate) {
		for entry in trust_update.entries {
			let from_index = if let Some(i) = self.indices.get(&entry.from) {
				*i
			} else {
				self.count += 1;
				self.indices.insert(entry.from, self.count);
				self.count
			};
			let to_index = if let Some(i) = self.indices.get(&entry.to) {
				*i
			} else {
				self.count += 1;
				self.indices.insert(entry.to, self.count);
				self.count
			};
			self.local_trust.entry(trust_update.trust_id.clone()).and_modify(|e| {
				e.entry(from_index.clone()).and_modify(|e| {
					e.insert(to_index, entry.value);
				});
			});
		}
	}

	pub fn update_seed(&mut self, seed_update: SeedUpdate) {
		for entry in seed_update.entries {
			let index = if let Some(i) = self.indices.get(&entry.id) {
				*i
			} else {
				self.count += 1;
				self.indices.insert(entry.id, self.count);
				self.count
			};
			let map = self.seed_trust.entry(seed_update.seed_id.clone()).or_insert(HashMap::new());
			map.insert(index, entry.value);
		}
	}

	pub fn compute(&self, domain: Domain) -> Vec<f32> {
		let trust_namespace = domain.trust_namespace();
		let seed_namespace = domain.seed_namespace();
		let lt = self.local_trust.get(&trust_namespace).unwrap();
		let seed = self.seed_trust.get(&seed_namespace).unwrap();
		positive_run::<30>(lt, seed)
	}

	pub fn compute_scores(&self, domain: Domain) -> Vec<CreateScores> {
		let index_to_address: HashMap<&u32, &Address> =
			self.indices.iter().map(|(k, v)| (v, k)).collect();
		let scores = self.compute(domain);
		let mut create_scores_txs = Vec::new();
		for (i, chunk) in scores.chunks(1000).enumerate() {
			let mut entries = Vec::new();
			for j in 0..chunk.len() {
				let index = (i * chunk.len() + j) as u32;
				let address = index_to_address.get(&index).unwrap();
				let score_entry = ScoreEntry::new((*address).clone(), chunk[i]);
				entries.push(score_entry);
			}
			let create_scores = CreateScores::new(entries);
			create_scores_txs.push(create_scores);
		}
		create_scores_txs
	}
}
