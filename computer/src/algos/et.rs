use std::collections::HashMap;

fn pre_process(lt: &mut HashMap<(u32, u32), f32>) {
	for ((from, to), value) in lt {
		if from == to {
			*value = 0.;
		}
	}
}

fn normalise(lt: &mut HashMap<(u32, u32), f32>) {
	let mut sum_map: HashMap<u32, f32> = HashMap::new();
	for ((from, _), value) in &mut *lt {
		sum_map.entry(*from).and_modify(|e| *e += *value);
	}

	for ((from, _), value) in lt {
		let sum = sum_map.get(&from).unwrap();
		*value /= sum;
	}
}

pub fn positive_run<const NUM_ITER: usize>(
	mut lt: HashMap<(u32, u32), f32>, seed: &HashMap<u32, f32>,
) -> Vec<f32> {
	pre_process(&mut lt);
	normalise(&mut lt);

	let mut scores = seed.clone();
	for _ in 0..NUM_ITER {
		let mut next_scores: HashMap<u32, f32> = HashMap::new();
		for ((from, to), value) in &lt {
			let origin_score = scores.get(from).unwrap();
			next_scores.entry(*to).and_modify(|e| *e += *value * origin_score);
		}
		scores = next_scores;
	}

	let mut scores: Vec<(u32, f32)> = scores.into_iter().collect();
	scores.sort_by_key(|e| e.0);
	scores.into_iter().map(|e| e.1).collect()
}
