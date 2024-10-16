use crate::txs::TxHash;
use crate::{merkle::Hash, topics::DomainHash};
use alloy_rlp_derive::{RlpDecodable, RlpEncodable};

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
struct PendingDomainUpdate {
    domain_id: DomainHash,
    commitment_tx_hash: TxHash,
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
struct DomainUpdate {
    domain_id: DomainHash,
    commitment_tx_hash: TxHash,
    verification_results_tx_hashes: Vec<TxHash>,
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct ProposedBlock {
    previous_block_hash: TxHash,
    state_root: Hash,
    pending_domain_updates: Vec<PendingDomainUpdate>,
    timestamp: u64,
    block_height: u64,
}

#[derive(Debug, Clone, Default, RlpEncodable, RlpDecodable)]
pub struct FinalisedBlock {
    previous_block_hash: TxHash,
    state_root: Hash,
    domain_updates: Vec<DomainUpdate>,
    timestamp: u64,
    block_height: u64,
}
