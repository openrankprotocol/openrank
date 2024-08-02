use std::collections::HashMap;

const PRE_TRUST_WEIGHT: f32 = 0.5;

fn pre_process(lt: &mut HashMap<(u32, u32), f32>) {
	for ((from, to), value) in lt {
		if from == to {
			*value = 0.;
		}
	}
}

fn normalise_lt(lt: &mut HashMap<(u32, u32), f32>) {
	let mut sum_map: HashMap<u32, f32> = HashMap::new();
	for ((from, _), value) in lt.iter() {
		let val = sum_map.get(from).unwrap_or(&0.0);
		sum_map.insert(*from, val + value);
	}

	for ((from, _), value) in lt {
		let sum = sum_map.get(&from).unwrap();
		*value /= sum;
	}
}

fn normalise_seed(seed: &mut HashMap<u32, f32>) {
	let sum: f32 = seed.iter().map(|(_, v)| v).sum();
	for (_, value) in seed {
		*value /= sum;
	}
}

pub fn positive_run<const NUM_ITER: usize>(
	mut lt: HashMap<(u32, u32), f32>, mut seed: HashMap<u32, f32>,
) -> Vec<f32> {
	pre_process(&mut lt);
	normalise_lt(&mut lt);
	normalise_seed(&mut seed);

	let mut scores = seed.clone();
	for _ in 0..NUM_ITER {
		let mut next_scores: HashMap<u32, f32> = HashMap::new();
		for ((from, to), value) in &lt {
			let origin_score = scores.get(from).unwrap_or(&0.0);
			let score = *value * origin_score;
			let to_score = next_scores.get(to).unwrap_or(&0.0);
			let final_to_score = to_score + score;
			let pre_trust = seed.get(to).unwrap_or(&0.0);
			let weighted_to_score =
				PRE_TRUST_WEIGHT * pre_trust + (final_to_score * (1. - PRE_TRUST_WEIGHT));
			next_scores.insert(*to, weighted_to_score);
		}
		normalise_seed(&mut next_scores);
		scores = next_scores;
	}

	let mut scores: Vec<(u32, f32)> = scores.into_iter().collect();
	scores.sort_by_key(|e| e.0);
	scores.into_iter().map(|e| e.1).collect()
}
