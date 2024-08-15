use std::collections::HashMap;

use crate::error::{AlgoError, CommonError};

const PRE_TRUST_WEIGHT: f32 = 0.5;
const DELTA: f32 = 0.01;

fn pre_process(lt: &mut HashMap<(u32, u32), f32>) {
	for ((from, to), value) in lt {
		if from == to {
			*value = 0.;
		}
	}
}

fn normalise_lt(lt: &mut HashMap<(u32, u32), f32>) -> Result<(), CommonError> {
	let mut sum_map: HashMap<u32, f32> = HashMap::new();
	for ((from, _), value) in lt.iter() {
		let val = sum_map.get(from).unwrap_or(&0.0);
		sum_map.insert(*from, val + value);
	}

	for ((from, _), value) in lt {
		let sum = match sum_map.get(&from) {
			Some(s) => s,
			None => return Err(AlgoError::ZeroSum.into()),
		};
		*value /= sum;
	}
	Ok(())
}

fn normalise_seed(seed: &mut HashMap<u32, f32>) -> Result<(), CommonError> {
	let sum: f32 = seed.iter().map(|(_, v)| v).sum();
	if sum == 0.0 {
		return Err(AlgoError::ZeroSum.into());
	}
	for (_, value) in seed {
		*value /= sum;
	}
	Ok(())
}

pub fn positive_run<const NUM_ITER: usize>(
	mut lt: HashMap<(u32, u32), f32>, mut seed: HashMap<u32, f32>,
) -> Result<Vec<(u32, f32)>, CommonError> {
	pre_process(&mut lt);
	normalise_lt(&mut lt)?;
	normalise_seed(&mut seed)?;

	let mut scores = seed.clone();
	loop {
		let mut next_scores: HashMap<u32, f32> = HashMap::new();
		for ((from, to), value) in &lt {
			let origin_score = scores.get(from).unwrap_or(&0.0);
			let score = *value * origin_score;
			let to_score = next_scores.get(to).unwrap_or(&0.0);
			let final_to_score = to_score + score;
			next_scores.insert(*to, final_to_score);
		}
		for (i, v) in &mut next_scores {
			let pre_trust = seed.get(&i).unwrap_or(&0.0);
			let weighted_to_score = PRE_TRUST_WEIGHT * pre_trust + (*v * (1. - PRE_TRUST_WEIGHT));
			*v = weighted_to_score;
		}
		normalise_seed(&mut next_scores)?;
		if is_converged(&scores, &next_scores) {
			break;
		}
		scores = next_scores;
	}

	Ok(scores.into_iter().collect())
}

pub fn is_converged(scores: &HashMap<u32, f32>, next_scores: &HashMap<u32, f32>) -> bool {
	let mut is_converged = true;
	for (i, v) in scores {
		let next_score = next_scores.get(i).unwrap_or(&0.0);
		let curr_converged = (next_score - v).abs() < DELTA;
		is_converged &= curr_converged;
	}
	is_converged
}

pub fn convergence_check(
	mut lt: HashMap<(u32, u32), f32>, seed: &HashMap<u32, f32>, scores: &HashMap<u32, f32>,
) -> Result<bool, CommonError> {
	normalise_lt(&mut lt)?;
	let mut next_scores = HashMap::new();
	for ((from, to), value) in &lt {
		let origin_score = scores.get(from).unwrap_or(&0.0);
		let score = *value * origin_score;
		let to_score = next_scores.get(to).unwrap_or(&0.0);
		let final_to_score = to_score + score;
		next_scores.insert(*to, final_to_score);
	}
	for (i, v) in &mut next_scores {
		let pre_trust = seed.get(&i).unwrap_or(&0.0);
		let weighted_to_score = PRE_TRUST_WEIGHT * pre_trust + (*v * (1. - PRE_TRUST_WEIGHT));
		*v = weighted_to_score;
	}
	normalise_seed(&mut next_scores)?;

	Ok(is_converged(scores, &next_scores))
}
