use openrank_common::result::JobResult;
use std::collections::HashMap;

pub struct JobCoordinator {
    job_results: HashMap<u64, JobResult>,
    count: u64,
}

impl JobCoordinator {
    pub fn new() -> Self {
        Self { job_results: HashMap::new(), count: 0 }
    }

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

    pub fn get_count(&self) -> u64 {
        self.count
    }
}
