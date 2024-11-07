use getset::Getters;
use openrank_common::tx::compute;
use std::collections::HashMap;

/// Coordinator role for the OpenRank network.
/// Responsible for sequencing job results.
#[derive(Default, Getters)]
#[getset(get = "pub")]
pub struct JobCoordinator {
    /// A map of all job results.
    job_results: HashMap<u64, compute::Result>,
    /// Total count of all job results.
    /// Used for assigning unique ordered sequence numbers to JobResult's.
    count: u64,
}

impl JobCoordinator {
    /// Create new instance of JobCoordinator.
    pub fn new() -> Self {
        Self { job_results: HashMap::new(), count: 0 }
    }

    /// Add a JobResult to memory and increase the counter in case
    /// it has not been seen before.
    pub fn add_job_result(&mut self, compute_result: &mut compute::Result) {
        if compute_result.seq_number().is_none() {
            compute_result.set_seq_number(self.count);
            self.count += 1;
        }
        let seq_number = compute_result.seq_number().unwrap();
        self.job_results.insert(seq_number, compute_result.clone());
        if seq_number > self.count {
            self.count = seq_number;
        }
    }
}
