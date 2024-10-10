use openrank_common::result::JobResult;
use std::collections::HashMap;

/// Coordinator role for the OpenRank network.
/// Responsible for sequencing job results.
pub struct JobCoordinator {
    /// A map of all job results.
    job_results: HashMap<u64, JobResult>,
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
    pub fn add_job_result(&mut self, job_result: &mut JobResult) {
        if job_result.seq_number.is_none() {
            job_result.set_seq_number(self.count);
            self.count += 1;
        }
        let seq_number = job_result.seq_number.unwrap();
        self.job_results.insert(seq_number, job_result.clone());
        if seq_number > self.count {
            self.count = seq_number;
        }
    }
}
