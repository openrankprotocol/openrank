use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::collections::{HashMap, HashSet};

use crate::misc::SingleLT;

/// The trust weight given to the seed trust vector in the trust matrix calculation.
const PRE_TRUST_WEIGHT: f32 = 0.5;

/// The threshold value used for convergence check in the trust matrix calculation.
///
/// If the absolute difference between the current score and the next score is
/// less than `DELTA`, the score has converged.
const DELTA: f32 = 0.01;

fn find_reachable_peers(lt: &HashMap<u64, SingleLT>, seed: &HashMap<u64, f32>) -> HashSet<u64> {
    let mut to_visit: Vec<&u64> = seed.keys().collect();
    let mut visited = HashSet::new();
    while let Some(i) = to_visit.pop() {
        if visited.contains(i) {
            continue;
        }
        visited.insert(*i);
        for (j, v) in lt.get(i).unwrap().outbound_trust_scores() {
            if !visited.contains(j) && *v > 0.0 {
                to_visit.push(j);
            }
        }
    }
    visited
}

/// Pre-processes a mutable local trust matrix `lt` by modifying it in-place:
///
/// - Removes self-trust (diagonal entries), as prohibited by EigenTrust.
fn pre_process(lt: &mut HashMap<u64, SingleLT>, seed: &mut HashMap<u64, f32>, count: u64) {
    // Calculate the sum of all seed trust values.
    let sum: f32 = seed.par_iter().map(|(_, v)| v).sum();

    if sum == 0.0 {
        for i in 0..count {
            seed.insert(i, 1.0);
        }
    }

    for from in 0..count {
        let sum = lt.get(&from).map(|lt| lt.outbound_sum()).unwrap_or(&0.0);
        // If peer does not have outbound trust,
        // his trust will be distributed to seed peers based on their seed/pre-trust
        if *sum == 0.0 {
            let single_lt = SingleLT::from_score_map(seed);
            lt.insert(from, single_lt);
        }
    }

    let reachable = find_reachable_peers(lt, seed);
    lt.retain(|from, _| reachable.contains(from));
}

/// Normalizes the `lt` matrix by dividing each element by the sum of its row.
fn normalise_lt(lt: &HashMap<u64, SingleLT>) -> HashMap<u64, SingleLT> {
    lt.par_iter()
        .fold(HashMap::new, |mut lt_norm, (from, from_map)| {
            let from_map_norm = from_map.norm();
            lt_norm.insert(*from, from_map_norm);
            lt_norm
        })
        .reduce(HashMap::new, |mut acc, lt_norm| {
            acc.extend(lt_norm);
            acc
        })
}

/// Normalizes the scores, to eliminate the rounding error
fn normalise_scores(scores: &HashMap<u64, f32>) -> HashMap<u64, f32> {
    // Calculate the sum of all seed trust values.
    let sum: f32 = scores.par_iter().map(|(_, v)| v).sum();

    scores
        .par_iter()
        .fold(HashMap::new, |mut scores, (i, value)| {
            scores.insert(*i, *value / sum);
            scores
        })
        .reduce(HashMap::new, |mut acc, scores| {
            acc.extend(scores);
            acc
        })
}

/// Performs the positive EigenTrust algorithm on the given local trust matrix (`lt`) and seed trust values (`seed`).
/// The algorithm iteratively updates the scores of each node until convergence.
/// It returns a vector of tuples containing the node ID and the final score.
pub fn positive_run(
    mut lt: HashMap<u64, SingleLT>, mut seed: HashMap<u64, f32>, count: u64,
) -> Vec<(u64, f32)> {
    pre_process(&mut lt, &mut seed, count);
    seed = normalise_scores(&seed);
    lt = normalise_lt(&lt);

    // Initialize the scores of each node to the seed trust values.
    let mut scores = seed.clone();
    // Iterate until convergence.
    loop {
        // Calculate the n+1 scores of each node.
        let n_plus_1_scores = iteration(&lt, &seed, &scores);
        // Normalise n+1 scores.
        let n_plus_1_scores = normalise_scores(&n_plus_1_scores);
        // Calculate the n+2 scores of each node.
        let n_plus_2_scores = iteration(&lt, &seed, &n_plus_1_scores);
        // Normalise n+2 scores
        let n_plus_2_scores = normalise_scores(&n_plus_2_scores);
        // Check for convergence.
        if is_converged(&n_plus_1_scores, &n_plus_2_scores) {
            // Convert the scores to a vector of tuples and return it.
            return n_plus_1_scores.into_iter().collect();
        }
        // Update the scores.
        scores = n_plus_2_scores;
    }
}

/// Given the previous scores (`scores`) and the next scores (`next_scores`), checks if the scores have converged.
/// It returns `true` if the scores have converged and `false` otherwise.
pub fn is_converged(scores: &HashMap<u64, f32>, next_scores: &HashMap<u64, f32>) -> bool {
    // Iterate over the scores and check if they have converged.
    scores
        .par_iter()
        .fold(
            || true,
            |is_converged, (i, v)| {
                // Get the next score of the node.
                let next_score = next_scores.get(i).unwrap_or(&0.0);
                // Check if the score has converged.
                let curr_converged = (next_score - v).abs() < DELTA;
                is_converged & curr_converged
            },
        )
        .reduce(|| true, |x, b| x & b)
}

/// Same as `is_converged`, but accepts the scores map in it's original form, where peers are identified by a `String`.
pub fn is_converged_org(scores: &HashMap<String, f32>, next_scores: &HashMap<String, f32>) -> bool {
    scores
        .par_iter()
        .fold(
            || true,
            |is_converged, (i, v)| {
                let next_score = next_scores.get(i).unwrap_or(&0.0);
                let curr_converged = (next_score - v).abs() < DELTA;
                is_converged & curr_converged
            },
        )
        .reduce(|| true, |x, b| x & b)
}

/// It performs a single iteration of the positive run EigenTrust algorithm on the given local trust matrix (`lt`),
/// seed trust values (`seed`), and previous scores (`scores`).
/// It returns `true` if the scores have converged and `false` otherwise.
pub fn convergence_check(
    mut lt: HashMap<u64, SingleLT>, mut seed: HashMap<u64, f32>, scores: &HashMap<u64, f32>,
    count: u64,
) -> bool {
    pre_process(&mut lt, &mut seed, count);
    seed = normalise_scores(&seed);
    lt = normalise_lt(&lt);
    // Calculate the next scores of each node
    let next_scores = iteration(&lt, &seed, scores);
    // Normalize the weighted next scores
    let next_scores = normalise_scores(&next_scores);

    // Check if the scores have converged
    is_converged(scores, &next_scores)
}

fn iteration(
    lt: &HashMap<u64, SingleLT>, seed: &HashMap<u64, f32>, scores: &HashMap<u64, f32>,
) -> HashMap<u64, f32> {
    lt.par_iter()
        .fold(HashMap::new, |mut next_scores, (from, from_map)| {
            let origin_score = scores.get(from).unwrap_or(&0.0);
            for (to, value) in from_map.outbound_trust_scores() {
                let score = *value * origin_score;
                let to_score = next_scores.get(to).unwrap_or(&0.0);
                let final_to_score = to_score + score;
                next_scores.insert(*to, final_to_score);
            }
            next_scores
        })
        .map(|mut next_scores| {
            // Calculate the weighted next scores of each node
            for (i, v) in &mut next_scores {
                let pre_trust = seed.get(i).unwrap_or(&0.0);
                let weighted_to_score =
                    PRE_TRUST_WEIGHT * pre_trust + (*v * (1. - PRE_TRUST_WEIGHT));
                *v = weighted_to_score;
            }
            next_scores
        })
        .reduce(HashMap::new, |mut acc, next| {
            for (i, v) in next {
                if acc.contains_key(&i) {
                    let val = acc.get(&i).unwrap();
                    acc.insert(i, v + val);
                } else {
                    acc.insert(i, v);
                }
            }
            acc
        })
}
