use crate::algos;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::collections::{HashMap, HashSet};

/// The trust weight given to the seed trust vector in the trust matrix calculation.
const PRE_TRUST_WEIGHT: f32 = 0.5;

/// The threshold value used for convergence check in the trust matrix calculation.
///
/// If the absolute difference between the current score and the next score is
/// less than `DELTA`, the score has converged.
const DELTA: f32 = 0.01;

fn get_all_peers(lt: &HashMap<(u64, u64), f32>, seed: &HashMap<u64, f32>) -> HashSet<u64> {
    let mut all_peers = HashSet::new();
    for ((from, to), _) in lt.iter() {
        all_peers.insert(*from);
        all_peers.insert(*to);
    }

    for (i, _) in seed.iter() {
        all_peers.insert(*i);
    }

    all_peers
}

/// Pre-processes a mutable local trust matrix `lt` by modifying it in-place:
///
/// - Removes self-trust (diagonal entries), as prohibited by EigenTrust.
fn pre_process(lt: &mut HashMap<(u64, u64), f32>, seed: &mut HashMap<u64, f32>) {
    let all_peers = get_all_peers(lt, seed);
    // Calculate the sum of all seed trust values.
    let sum: f32 = seed.par_iter().map(|(_, v)| v).sum();

    if sum == 0.0 {
        for i in all_peers.iter() {
            seed.insert(*i, 1.0);
        }
    }

    // Calculate the sum of each row in the local trust matrix.
    let mut outbound_sum_map: HashMap<u64, f32> = HashMap::new();
    for ((from, _), value) in lt.iter() {
        let out_val = outbound_sum_map.get(from).unwrap_or(&0.0);
        outbound_sum_map.insert(*from, out_val + value);
    }

    for from in all_peers.iter() {
        let sum = outbound_sum_map.get(from).unwrap_or(&0.0);
        // If peer does not have outbound trust,
        // his trust will be distributed to seed peers based on their seed/pre-trust
        if *sum == 0.0 {
            for (to, value) in seed.iter() {
                lt.insert((*from, *to), *value);
            }
        }
    }

    let mut inbound_sum_map: HashMap<u64, f32> = HashMap::new();
    for ((_, to), value) in lt.iter() {
        let in_val = inbound_sum_map.get(to).unwrap_or(&0.0);
        inbound_sum_map.insert(*to, in_val + value);
    }

    // Set the trust value to 0 for all self-trust entries in the local trust matrix.
    lt.retain(|(from, to), _| {
        if from == to {
            return false;
        }
        let sum = inbound_sum_map.get(from).unwrap_or(&0.0);
        if *sum == 0.0 {
            return false;
        }
        true
    });
}

/// Normalizes the `lt` matrix by dividing each element by the sum of its row.
fn normalise_lt(lt: &mut HashMap<(u64, u64), f32>) -> Result<(), algos::Error> {
    let sum_map = lt
        .par_iter()
        .fold(
            || HashMap::new(),
            |mut sum_map, ((from, _), value)| {
                let val = sum_map.get(from).unwrap_or(&0.0);
                sum_map.insert(*from, val + value);
                sum_map
            },
        )
        .reduce(
            || HashMap::new(),
            |mut acc, sum_map| {
                for (i, v) in sum_map {
                    if acc.contains_key(&i) {
                        let val = acc.get(&i).unwrap();
                        acc.insert(i, v + val);
                    } else {
                        acc.insert(i, v);
                    }
                }
                acc
            },
        );

    // Divide each element in the local trust matrix by the sum of its row.
    for ((from, _), value) in lt {
        let sum = sum_map.get(from).ok_or(algos::Error::ZeroSum)?;
        if *sum == 0.0 {
            return Err(algos::Error::ZeroSum);
        }
        *value /= sum;
    }

    Ok(())
}

/// Normalizes the scores, to eliminate the rounding error
fn normalise_scores(scores: &HashMap<u64, f32>) -> Result<HashMap<u64, f32>, algos::Error> {
    // Calculate the sum of all seed trust values.
    let sum: f32 = scores.par_iter().map(|(_, v)| v).sum();

    let scores = scores
        .par_iter()
        .fold(
            || HashMap::new(),
            |mut scores, (i, value)| {
                scores.insert(*i, *value / sum);
                scores
            },
        )
        .reduce(
            || HashMap::new(),
            |mut acc, scores| {
                acc.extend(scores);
                acc
            },
        );
    Ok(scores)
}

/// Performs the positive EigenTrust algorithm on the given local trust matrix (`lt`) and seed trust values (`seed`).
/// The algorithm iteratively updates the scores of each node until convergence.
/// It returns a vector of tuples containing the node ID and the final score.
pub fn positive_run(
    mut lt: HashMap<(u64, u64), f32>, mut seed: HashMap<u64, f32>,
) -> Result<Vec<(u64, f32)>, algos::Error> {
    pre_process(&mut lt, &mut seed);
    let seed = normalise_scores(&seed)?;
    normalise_lt(&mut lt)?;

    // Initialize the scores of each node to the seed trust values.
    let mut scores = seed.clone();
    // Iterate until convergence.
    loop {
        // Calculate the next scores of each node.
        let next_scores = iteration(&lt, &seed, &scores);
        // Normalise the next scores.
        let next_scores = normalise_scores(&next_scores)?;
        // Check for convergence.
        if is_converged(&scores, &next_scores) {
            break;
        }
        // Update the scores.
        scores = next_scores;
    }

    // Convert the scores to a vector of tuples and return it.
    Ok(scores.into_iter().collect())
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
    mut lt: HashMap<(u64, u64), f32>, mut seed: HashMap<u64, f32>, scores: &HashMap<u64, f32>,
) -> Result<bool, algos::Error> {
    pre_process(&mut lt, &mut seed);
    let seed = normalise_scores(&seed)?;
    normalise_lt(&mut lt)?;
    // Calculate the next scores of each node
    let next_scores = iteration(&lt, &seed, scores);
    // Normalize the weighted next scores
    let next_scores = normalise_scores(&next_scores)?;

    // Check if the scores have converged
    Ok(is_converged(scores, &next_scores))
}

fn iteration(
    lt: &HashMap<(u64, u64), f32>, seed: &HashMap<u64, f32>, scores: &HashMap<u64, f32>,
) -> HashMap<u64, f32> {
    lt.par_iter()
        .fold(
            || HashMap::new(),
            |mut next_scores, ((from, to), value)| {
                let origin_score = scores.get(from).unwrap_or(&0.0);
                let score = *value * origin_score;
                let to_score = next_scores.get(to).unwrap_or(&0.0);
                let final_to_score = to_score + score;
                next_scores.insert(*to, final_to_score);
                next_scores
            },
        )
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
        .reduce(
            || HashMap::new(),
            |mut acc, next| {
                for (i, v) in next {
                    if acc.contains_key(&i) {
                        let val = acc.get(&i).unwrap();
                        acc.insert(i, v + val);
                    } else {
                        acc.insert(i, v);
                    }
                }
                acc
            },
        )
}
