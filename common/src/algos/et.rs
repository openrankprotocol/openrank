use std::collections::HashMap;

use super::AlgoError;

/// The trust weight given to the seed trust vector in the trust matrix calculation.
const PRE_TRUST_WEIGHT: f32 = 0.5;

/// The threshold value used for convergence check in the trust matrix calculation.
///
/// If the absolute difference between the current score and the next score is
/// less than `DELTA`, the score has converged.
const DELTA: f32 = 0.01;

/// Pre-processes a mutable local trust matrix `lt` by modifying it in-place:
///
/// - Removes self-trust (diagonal entries), as prohibited by EigenTrust.
fn pre_process(lt: &mut HashMap<(u32, u32), f32>) {
    // Set the trust value to 0 for all self-trust entries in the local trust matrix.
    for ((from, to), value) in lt {
        if *from == *to {
            *value = 0.;
        }
    }
}

/// Normalizes the local trust matrix by dividing each element by the sum of its row.
///
/// # Arguments
///
/// * `lt` - A mutable reference to the local trust matrix.
fn normalise_lt(lt: &mut HashMap<(u32, u32), f32>) -> Result<(), AlgoError> {
    // Calculate the sum of each row in the local trust matrix.
    let mut sum_map: HashMap<u32, f32> = HashMap::new();
    for ((from, _), value) in lt.iter() {
        let val = sum_map.get(from).unwrap_or(&0.0);
        sum_map.insert(*from, val + value);
    }

    // Divide each element in the local trust matrix by the sum of its row.
    for ((from, _), value) in lt {
        let sum = sum_map.get(&from).ok_or(AlgoError::ZeroSum)?;
        if *sum == 0.0 {
            return Err(AlgoError::ZeroSum);
        }
        *value /= sum;
    }
    Ok(())
}

/// Normalises the seed trust values by dividing each value by the sum of all seed trust values.
///
/// # Arguments
///
/// * `seed` - A mutable reference to the HashMap containing the seed trust values.
fn normalise_seed(seed: &mut HashMap<u32, f32>) -> Result<(), AlgoError> {
    // Calculate the sum of all seed trust values.
    let sum: f32 = seed.iter().map(|(_, v)| v).sum();

    // Divide each seed trust value by the sum to normalise.
    if sum == 0.0 {
        return Err(AlgoError::ZeroSum);
    }
    for (_, value) in seed {
        *value /= sum;
    }
    Ok(())
}

/// Run the positive run EigenTrust algorithm on the given local trust matrix and seed trust values.
///
/// The algorithm iteratively updates the scores of each node until convergence.
///
/// # Arguments
///
/// * `lt` - A mutable reference to the local trust matrix.
/// * `seed` - A mutable reference to the HashMap containing the seed trust values.
///
/// # Returns
///
/// A vector of tuples containing the node ID and the final score.
pub fn positive_run<const NUM_ITER: usize>(
    mut lt: HashMap<(u32, u32), f32>, mut seed: HashMap<u32, f32>,
) -> Result<Vec<(u32, f32)>, AlgoError> {
    pre_process(&mut lt);
    normalise_lt(&mut lt)?;
    normalise_seed(&mut seed)?;

    // Initialize the scores of each node to the seed trust values.
    let mut scores = seed.clone();
    // Iterate until convergence.
    loop {
        // Calculate the next scores of each node.
        let mut next_scores: HashMap<u32, f32> = HashMap::new();
        for ((from, to), value) in &lt {
            let origin_score = scores.get(from).unwrap_or(&0.0);
            let score = *value * origin_score;
            let to_score = next_scores.get(to).unwrap_or(&0.0);
            let final_to_score = to_score + score;
            next_scores.insert(*to, final_to_score);
        }
        // Calculate the weighted next scores of each node.
        for (i, v) in &mut next_scores {
            let pre_trust = seed.get(&i).unwrap_or(&0.0);
            let weighted_to_score = PRE_TRUST_WEIGHT * pre_trust + (*v * (1. - PRE_TRUST_WEIGHT));
            *v = weighted_to_score;
        }
        // Normalise the next scores.
        normalise_seed(&mut next_scores)?;
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

/// Checks if the scores have converged.
///
/// # Arguments
///
/// * `scores`: The previous scores of the nodes.
/// * `next_scores`: The next scores of the nodes.
///
/// # Returns
///
/// `true` if the scores have converged and `false` otherwise.
pub fn is_converged(scores: &HashMap<u32, f32>, next_scores: &HashMap<u32, f32>) -> bool {
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

/// Checks if the scores have converged after a single iteration of the algorithm.
///
/// # Arguments
///
/// * `lt` - The local trust matrix.
/// * `seed` - The seed trust values.
/// * `scores` - The previous scores of the nodes.
///
/// # Returns
///
/// `true` if the scores have converged and `false` otherwise.
pub fn convergence_check(
    mut lt: HashMap<(u32, u32), f32>, seed: &HashMap<u32, f32>, scores: &HashMap<u32, f32>,
) -> Result<bool, AlgoError> {
    // Normalize the local trust matrix
    normalise_lt(&mut lt)?;
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
        let pre_trust = seed.get(&i).unwrap_or(&0.0);
        let weighted_to_score = PRE_TRUST_WEIGHT * pre_trust + (*v * (1. - PRE_TRUST_WEIGHT));
        *v = weighted_to_score;
    }
    // Normalize the weighted next scores
    normalise_seed(&mut next_scores)?;

    // Check if the scores have converged
    Ok(is_converged(scores, &next_scores))
}
