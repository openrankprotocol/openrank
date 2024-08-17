use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};

use openrank_common::algos::AlgoError;
use openrank_common::db::DbError;
use openrank_common::merkle::MerkleError;
use openrank_common::topics::DomainHash;
use openrank_common::txs::TxHash;

#[derive(Debug)]
pub enum VerifierNodeError {
	ComputeMerkleError(MerkleError),
	ComputeAlgoError(AlgoError),
	SerdeError(alloy_rlp::Error),
	DbError(DbError),
	DomainNotFound(String),
	P2PError(String),
	ComputeInternalError(JobRunnerError),
}

impl StdError for VerifierNodeError {}

impl Display for VerifierNodeError {
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
	ComputeTreeNotFound(ComputeTreesError),
	CreateScoresNotFound(DomainHash),
	ActiveAssignmentsNotFound(DomainHash),
	CompletedAssignmentsNotFound(DomainHash),
	CommitmentNotFound(TxHash),
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
			Self::ComputeTreeNotFound(err) => match err {
				ComputeTreesError::NotFoundWithDomain(domain) => {
					write!(f, "compute_tree not found for domain: {:?}", domain)
				},
				ComputeTreesError::NotFoundWithTxHash(tx_hash) => {
					write!(f, "compute_tree not found for tx_hash: {:?}", tx_hash)
				},
			},
			Self::CreateScoresNotFound(domain) => {
				write!(f, "create_scores not found for domain: {:?}", domain)
			},
			Self::ActiveAssignmentsNotFound(domain) => {
				write!(f, "active_assignments not found for domain: {:?}", domain)
			},
			Self::CompletedAssignmentsNotFound(domain) => {
				write!(
					f,
					"completed_assignments not found for domain: {:?}",
					domain
				)
			},
			Self::CommitmentNotFound(assigment_id) => {
				write!(
					f,
					"commitment not found for assignment_id: {:?}",
					assigment_id
				)
			},
		}
	}
}

#[derive(Debug)]
pub enum LocalTrustSubTreesError {
	NotFoundWithDomain(DomainHash),
	NotFoundWithIndex(u32),
}

#[derive(Debug)]
pub enum ComputeTreesError {
	NotFoundWithDomain(DomainHash),
	NotFoundWithTxHash(TxHash),
}
