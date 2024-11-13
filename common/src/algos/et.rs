use crate::algos;
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
fn pre_process(lt: &mut HashMap<(u64, u64), f32>) {
    // Set the trust value to 0 for all self-trust entries in the local trust matrix.
    for ((from, to), value) in lt {
        if from == to {
            *value = 0.;
        }
    }
}

/// Normalizes the `lt` matrix by dividing each element by the sum of its row.
fn normalise_lt(
    all_peers: &HashSet<u64>, lt: &mut HashMap<(u64, u64), f32>, seed: &HashMap<u64, f32>,
) -> Result<(), algos::Error> {
    // Calculate seed sum for later use
    let seed_sum: f32 = seed.iter().map(|(_, v)| v).sum();

    // Calculate the sum of each row in the local trust matrix.
    let mut sum_map: HashMap<u64, f32> = HashMap::new();
    for ((from, _), value) in lt.iter() {
        let val = sum_map.get(from).unwrap_or(&0.0);
        sum_map.insert(*from, val + value);
    }

    for from in all_peers.iter() {
        let sum = sum_map.get(from).unwrap_or(&0.0);
        // If peer does not have outbound trust,
        // his trust will be distributed to seed peers based on their seed/pre-trust
        if *sum == 0.0 {
            for (to, value) in seed {
                lt.insert((*from, *to), *value);
            }
        }
        sum_map.insert(*from, seed_sum);
    }

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

/// Normalizes the seed trust (`seed`) values by dividing each value by the sum of all seed trust values.
fn normalise_seed(
    all_peers: &HashSet<u64>, seed: &mut HashMap<u64, f32>,
) -> Result<(), algos::Error> {
    // Calculate the sum of all seed trust values.
    let sum: f32 = seed.iter().map(|(_, v)| v).sum();

    // Divide each seed trust value by the sum to normalise.
    if sum == 0.0 {
        for i in all_peers {
            seed.insert(*i, 1.0);
        }
    }

    let sum: f32 = seed.iter().map(|(_, v)| v).sum();

    for value in seed.values_mut() {
        *value /= sum;
    }
    Ok(())
}

/// Performs the positive EigenTrust algorithm on the given local trust matrix (`lt`) and seed trust values (`seed`).
/// The algorithm iteratively updates the scores of each node until convergence.
/// It returns a vector of tuples containing the node ID and the final score.
pub fn positive_run<const NUM_ITER: usize>(
    mut lt: HashMap<(u64, u64), f32>, mut seed: HashMap<u64, f32>,
) -> Result<Vec<(u64, f32)>, algos::Error> {
    let all_peers = get_all_peers(&lt, &seed);
    pre_process(&mut lt);
    normalise_seed(&all_peers, &mut seed)?;
    normalise_lt(&all_peers, &mut lt, &seed)?;

    // Initialize the scores of each node to the seed trust values.
    let mut scores = seed.clone();
    // Iterate until convergence.
    loop {
        // Calculate the next scores of each node.
        let mut next_scores: HashMap<u64, f32> = HashMap::new();
        for ((from, to), value) in &lt {
            let origin_score = scores.get(from).unwrap_or(&0.0);
            let score = *value * origin_score;
            let to_score = next_scores.get(to).unwrap_or(&0.0);
            let final_to_score = to_score + score;
            next_scores.insert(*to, final_to_score);
        }
        // Calculate the weighted next scores of each node.
        for (i, v) in &mut next_scores {
            let pre_trust = seed.get(i).unwrap_or(&0.0);
            let weighted_to_score = PRE_TRUST_WEIGHT * pre_trust + (*v * (1. - PRE_TRUST_WEIGHT));
            *v = weighted_to_score;
        }
        // Normalise the next scores.
        normalise_seed(&all_peers, &mut next_scores)?;
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
    // Initialize a boolean flag to track if the scores have converged.
    let mut is_converged = true;
    // Iterate over the scores and check if they have converged.
    for (i, v) in scores {
        // Get the next score of the node.
        let next_score = next_scores.get(i).unwrap_or(&0.0);
        // Check if the score has converged.
        let curr_converged = (next_score - v).abs() < DELTA;
        is_converged &= curr_converged;
    }
    // Return the convergence flag.
    is_converged
}

/// Same as `is_converged`, but accepts the scores map in it's original form, where peers are identified by a `String`.
pub fn is_converged_org(scores: &HashMap<String, f32>, next_scores: &HashMap<String, f32>) -> bool {
    let mut is_converged = true;
    for (i, v) in scores {
        let next_score = next_scores.get(i).unwrap_or(&0.0);
        let curr_converged = (next_score - v).abs() < DELTA;
        is_converged &= curr_converged;
    }
    is_converged
}

/// It performs a single iteration of the positive run EigenTrust algorithm on the given local trust matrix (`lt`),
/// seed trust values (`seed`), and previous scores (`scores`).
/// It returns `true` if the scores have converged and `false` otherwise.
pub fn convergence_check(
    mut lt: HashMap<(u64, u64), f32>, mut seed: HashMap<u64, f32>, scores: &HashMap<u64, f32>,
) -> Result<bool, algos::Error> {
    let all_peers = get_all_peers(&lt, &seed);
    pre_process(&mut lt);
    normalise_seed(&all_peers, &mut seed)?;
    normalise_lt(&all_peers, &mut lt, &seed)?;
    // Calculate the next scores of each node
    let mut next_scores = HashMap::new();
    for ((from, to), value) in &lt {
        let origin_score = scores.get(from).unwrap_or(&0.0);
        let score = *value * origin_score;
        let to_score = next_scores.get(to).unwrap_or(&0.0);
        let final_to_score = to_score + score;
        next_scores.insert(*to, final_to_score);
    }

    // Calculate the weighted next scores of each node
    for (i, v) in &mut next_scores {
        let pre_trust = seed.get(i).unwrap_or(&0.0);
        let weighted_to_score = PRE_TRUST_WEIGHT * pre_trust + (*v * (1. - PRE_TRUST_WEIGHT));
        *v = weighted_to_score;
    }
    // Normalize the weighted next scores
    normalise_seed(&all_peers, &mut next_scores)?;

    // Check if the scores have converged
    Ok(is_converged(scores, &next_scores))
}
