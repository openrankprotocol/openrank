use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};

use openrank_common::algos::AlgoError;
use openrank_common::db::DbError;
use openrank_common::merkle::MerkleError;
use openrank_common::topics::DomainHash;

#[derive(Debug)]
pub enum ComputeNodeError {
	ComputeMerkleError(MerkleError),
	ComputeAlgoError(AlgoError),
	SerdeError(alloy_rlp::Error),
	DbError(DbError),
	DomainNotFound(String),
	P2PError(String),
	ComputeInternalError(JobRunnerError),
}

impl StdError for ComputeNodeError {}

impl Display for ComputeNodeError {
	fn fmt(&self, f: &mut Formatter) -> FmtResult {
		match self {
			Self::ComputeMerkleError(err) => err.fmt(f),
			Self::ComputeAlgoError(err) => err.fmt(f),
			Self::SerdeError(err) => err.fmt(f),
			Self::DbError(err) => err.fmt(f),
			Self::DomainNotFound(domain) => write!(f, "Domain not found: {}", domain),
			Self::P2PError(err) => write!(f, "p2p error: {}", err),
			Self::ComputeInternalError(err) => write!(f, "internal error: {}", err),
		}
	}
}

#[derive(Debug)]
pub enum JobRunnerError {
	IndicesNotFound(DomainHash),
	CountNotFound(DomainHash),
	LocalTrustSubTreesNotFound(LocalTrustSubTreesError),
	LocalTrustMasterTreeNotFound(DomainHash),
	LocalTrustNotFound(DomainHash),
	SeedTrustNotFound(DomainHash),
	ComputeResultsNotFound(DomainHash),
	IndexToAddressNotFound(u32),
	ComputeTreeNotFound(DomainHash),
}

impl Display for JobRunnerError {
	fn fmt(&self, f: &mut Formatter) -> FmtResult {
		match self {
			Self::IndicesNotFound(domain) => {
				write!(f, "indices not found for domain: {:?}", domain)
			},
			Self::CountNotFound(domain) => write!(f, "count not found for domain: {:?}", domain),
			Self::LocalTrustSubTreesNotFound(err) => match err {
				LocalTrustSubTreesError::NotFoundWithDomain(domain) => {
					write!(
						f,
						"local_trust_sub_trees not found for domain: {:?}",
						domain
					)
				},
				LocalTrustSubTreesError::NotFoundWithIndex(index) => {
					write!(f, "local_trust_sub_trees not found for index: {}", index)
				},
			},
			Self::LocalTrustMasterTreeNotFound(domain) => {
				write!(
					f,
					"local_trust_master_tree not found for domain: {:?}",
					domain
				)
			},
			Self::LocalTrustNotFound(domain) => {
				write!(f, "local_trust not found for domain: {:?}", domain)
			},
			Self::SeedTrustNotFound(domain) => {
				write!(f, "seed_trust not found for domain: {:?}", domain)
			},
			Self::ComputeResultsNotFound(domain) => {
				write!(f, "compute_results not found for domain: {:?}", domain)
			},
			Self::IndexToAddressNotFound(index) => {
				write!(f, "index_to_address not found for index: {}", index)
			},
			Self::ComputeTreeNotFound(domain) => {
				write!(f, "compute_tree not found for domain: {:?}", domain)
			},
		}
	}
}

#[derive(Debug)]
pub enum LocalTrustSubTreesError {
	NotFoundWithDomain(DomainHash),
	NotFoundWithIndex(u32),
}
