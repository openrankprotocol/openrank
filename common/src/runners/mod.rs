use crate::{
    merkle::{
        fixed::DenseMerkleTree, incremental::DenseIncrementalMerkleTree,
    },
    misc::OutboundLocalTrust,
    topics::DomainHash,
    tx::{
        trust::OwnedNamespace,
        TxHash,
    },
};
use getset::Getters;
use sha3::Keccak256;
use std::collections::HashMap;

pub mod compute_runner;
pub mod verification_runner;

#[derive(Getters)]
#[getset(get = "pub")]
pub struct BaseRunner {
    count: HashMap<DomainHash, u64>,
    indices: HashMap<DomainHash, HashMap<String, u64>>,
    local_trust: HashMap<OwnedNamespace, HashMap<u64, OutboundLocalTrust>>,
    seed_trust: HashMap<OwnedNamespace, HashMap<u64, f32>>,
    lt_sub_trees: HashMap<DomainHash, HashMap<u64, DenseIncrementalMerkleTree<Keccak256>>>,
    lt_master_tree: HashMap<DomainHash, DenseIncrementalMerkleTree<Keccak256>>,
    st_master_tree: HashMap<DomainHash, DenseIncrementalMerkleTree<Keccak256>>,
    compute_tree: HashMap<DomainHash, HashMap<TxHash, DenseMerkleTree<Keccak256>>>,
}
