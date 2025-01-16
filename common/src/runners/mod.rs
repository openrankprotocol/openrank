use crate::{
    merkle::{self, hash_leaf, hash_two, incremental::DenseIncrementalMerkleTree, Hash},
    misc::{compute_localtrust_peer_range, compute_seedtrust_peer_range, create_localtrust_next_token, create_seedtrust_next_token, LocalTrustStateResponse, OutboundLocalTrust, SeedTrustStateResponse},
    topics::{Domain, DomainHash},
    tx::trust::{OwnedNamespace, ScoreEntry, TrustEntry},
};
use getset::Getters;
use sha3::Keccak256;
use std::{
    collections::HashMap,
    fmt::{Display, Formatter, Result as FmtResult},
};

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
}

impl BaseRunner {
    pub fn new(domains: &[Domain]) -> Self {
        let mut count = HashMap::new();
        let mut indices = HashMap::new();
        let mut local_trust = HashMap::new();
        let mut seed_trust = HashMap::new();
        let mut lt_sub_trees = HashMap::new();
        let mut lt_master_tree = HashMap::new();
        let mut st_master_tree = HashMap::new();
        let mut compute_results = HashMap::new();
        for domain in domains {
            let domain_hash = domain.to_hash();
            count.insert(domain_hash, 0);
            indices.insert(domain_hash, HashMap::new());
            local_trust.insert(domain.trust_namespace(), HashMap::new());
            seed_trust.insert(domain.trust_namespace(), HashMap::new());
            lt_sub_trees.insert(domain_hash, HashMap::new());
            lt_master_tree.insert(
                domain_hash,
                DenseIncrementalMerkleTree::<Keccak256>::new(32),
            );
            st_master_tree.insert(
                domain_hash,
                DenseIncrementalMerkleTree::<Keccak256>::new(32),
            );
            compute_results.insert(domain_hash, Vec::<f32>::new());
        }
        Self {
            count,
            indices,
            local_trust,
            seed_trust,
            lt_sub_trees,
            lt_master_tree,
            st_master_tree,
        }
    }

    pub fn update_trust(
        &mut self, domain: Domain, trust_entries: Vec<TrustEntry>,
    ) -> Result<(), Error> {
        let domain_indices = self
            .indices
            .get_mut(&domain.to_hash())
            .ok_or::<Error>(Error::IndicesNotFound(domain.to_hash()))?;
        let count = self
            .count
            .get_mut(&domain.to_hash())
            .ok_or::<Error>(Error::CountNotFound(domain.to_hash()))?;
        let lt_sub_trees = self.lt_sub_trees.get_mut(&domain.to_hash()).ok_or::<Error>(
            Error::LocalTrustSubTreesNotFoundWithDomain(domain.to_hash()),
        )?;
        let lt_master_tree = self
            .lt_master_tree
            .get_mut(&domain.to_hash())
            .ok_or::<Error>(Error::LocalTrustMasterTreeNotFound(domain.to_hash()))?;
        let lt = self
            .local_trust
            .get_mut(&domain.trust_namespace())
            .ok_or::<Error>(Error::LocalTrustNotFound(domain.trust_namespace()))?;
        let default_sub_tree = DenseIncrementalMerkleTree::<Keccak256>::new(32);
        for entry in trust_entries {
            let from_index = if let Some(i) = domain_indices.get(entry.from()) {
                *i
            } else {
                let curr_count = *count;
                domain_indices.insert(entry.from().clone(), curr_count);
                *count += 1;
                curr_count
            };
            let to_index = if let Some(i) = domain_indices.get(entry.to()) {
                *i
            } else {
                let curr_count = *count;
                domain_indices.insert(entry.to().clone(), curr_count);
                *count += 1;
                curr_count
            };

            let from_map = lt.entry(from_index).or_insert(OutboundLocalTrust::new());
            let is_zero = entry.value() == &0.0;
            let exists = from_map.contains_key(&to_index);
            if is_zero && exists {
                from_map.remove(&to_index);
            } else if !is_zero {
                from_map.insert(to_index, *entry.value());
            }

            lt_sub_trees.entry(from_index).or_insert_with(|| default_sub_tree.clone());
            let sub_tree = lt_sub_trees
                .get_mut(&from_index)
                .ok_or(Error::LocalTrustSubTreesNotFoundWithIndex(from_index))?;

            let leaf = hash_leaf::<Keccak256>(entry.value().to_be_bytes().to_vec());
            sub_tree.insert_leaf(to_index, leaf);

            let sub_tree_root = sub_tree.root().map_err(Error::Merkle)?;

            let leaf = hash_leaf::<Keccak256>(sub_tree_root.inner().to_vec());
            lt_master_tree.insert_leaf(from_index, leaf);
        }

        Ok(())
    }

    pub fn update_seed(
        &mut self, domain: Domain, seed_entries: Vec<ScoreEntry>,
    ) -> Result<(), Error> {
        let domain_indices = self
            .indices
            .get_mut(&domain.to_hash())
            .ok_or::<Error>(Error::IndicesNotFound(domain.to_hash()))?;
        let count = self
            .count
            .get_mut(&domain.to_hash())
            .ok_or::<Error>(Error::CountNotFound(domain.to_hash()))?;
        let st_master_tree = self
            .st_master_tree
            .get_mut(&domain.to_hash())
            .ok_or::<Error>(Error::SeedTrustMasterTreeNotFound(domain.to_hash()))?;
        let seed = self
            .seed_trust
            .get_mut(&domain.seed_namespace())
            .ok_or::<Error>(Error::SeedTrustNotFound(domain.seed_namespace()))?;
        for entry in seed_entries {
            let index = if let Some(i) = domain_indices.get(entry.id()) {
                *i
            } else {
                let curr_count = *count;
                domain_indices.insert(entry.id().clone(), curr_count);
                *count += 1;
                curr_count
            };
            let is_zero = entry.value() == &0.0;
            let exists = seed.contains_key(&index);
            if is_zero && exists {
                seed.remove(&index);
            } else if !is_zero {
                seed.insert(index, *entry.value());
            }

            let leaf = hash_leaf::<Keccak256>(entry.value().to_be_bytes().to_vec());
            st_master_tree.insert_leaf(index, leaf);
        }

        Ok(())
    }

    pub fn get_base_root_hashes(&self, domain: &Domain) -> Result<Hash, Error> {
        let lt_tree = self
            .lt_master_tree
            .get(&domain.to_hash())
            .ok_or::<Error>(Error::LocalTrustMasterTreeNotFound(domain.to_hash()))?;
        let st_tree = self
            .st_master_tree
            .get(&domain.to_hash())
            .ok_or::<Error>(Error::SeedTrustMasterTreeNotFound(domain.to_hash()))?;
        let lt_tree_root = lt_tree.root().map_err(Error::Merkle)?;
        let st_tree_root = st_tree.root().map_err(Error::Merkle)?;
        let tree_roots = hash_two::<Keccak256>(lt_tree_root, st_tree_root);
        Ok(tree_roots)
    }

    pub fn get_lt_state(
        &self, domain: &Domain, page_size: Option<usize>, next_token: Option<String>,
    ) -> Result<LocalTrustStateResponse, Error> {
        let lt = self
            .local_trust
            .get(&domain.trust_namespace())
            .ok_or(Error::LocalTrustNotFound(domain.trust_namespace()))?;

        let lt_peers_cnt = lt.len() as u64;
        let (from_peer_start, from_peer_end, to_peer_start, to_peer_end) =
            compute_localtrust_peer_range(lt_peers_cnt, page_size, next_token)?;
        if from_peer_start >= lt_peers_cnt {
            return Ok(LocalTrustStateResponse::new(vec![], None));
        }
        
        let mut result = vec![];
        if from_peer_start == from_peer_end {
            let lt_row =
                lt.get(&from_peer_start).ok_or(Error::LocalTrustNotFound(domain.trust_namespace()))?;
            for to_peer_id in to_peer_start..to_peer_end {
                let lt_entry_value = lt_row
                    .get(&to_peer_id)
                    .ok_or(Error::LocalTrustNotFound(domain.trust_namespace()))?;
                result.push((from_peer_start, to_peer_id, lt_entry_value));
            }
        } else {
            let lt_row =
                lt.get(&from_peer_start).ok_or(Error::LocalTrustNotFound(domain.trust_namespace()))?;
            for to_peer_id in to_peer_start..lt_peers_cnt {
                let lt_entry_value = lt_row
                    .get(&to_peer_id)
                    .ok_or(Error::LocalTrustNotFound(domain.trust_namespace()))?;
                result.push((from_peer_start, to_peer_id, lt_entry_value));
            }
         
            for from_peer_id in from_peer_start + 1..from_peer_end {
                let lt_row =
                    lt.get(&from_peer_id).ok_or(Error::LocalTrustNotFound(domain.trust_namespace()))?;
                for to_peer_id in 0..lt_peers_cnt {
                    let lt_entry_value = lt_row
                        .get(&to_peer_id)
                        .ok_or(Error::LocalTrustNotFound(domain.trust_namespace()))?;
                    result.push((from_peer_id, to_peer_id, lt_entry_value));
                }    
            }
            
            if from_peer_end != lt_peers_cnt {
                let lt_row =
                    lt.get(&from_peer_end).ok_or(Error::LocalTrustNotFound(domain.trust_namespace()))?;
                for to_peer_id in 0..to_peer_end {
                    let lt_entry_value = lt_row
                        .get(&to_peer_id)
                        .ok_or(Error::LocalTrustNotFound(domain.trust_namespace()))?;
                    result.push((from_peer_end, to_peer_id, lt_entry_value));
                }
            }
        }

        let next_token = create_localtrust_next_token(lt_peers_cnt, from_peer_end, to_peer_end);

        Ok(LocalTrustStateResponse::new(result, next_token))
    }

    pub fn get_st_state(
        &self, domain: &Domain, page_size: Option<usize>, next_token: Option<String>,
    ) -> Result<SeedTrustStateResponse, Error> {
        let st = self
            .seed_trust
            .get(&domain.seed_namespace())
            .ok_or(Error::SeedTrustNotFound(domain.seed_namespace()))?;

        let st_peers_cnt = st.len();
        let (start_peer, end_peer) = compute_seedtrust_peer_range(st_peers_cnt, page_size, next_token)?;

        let mut result = vec![];
        for peer_id in start_peer..end_peer {
            let seed =
                st.get(&peer_id).ok_or(Error::SeedTrustNotFound(domain.seed_namespace()))?;
            result.push((peer_id, *seed));
        }

        let next_token = create_seedtrust_next_token(st_peers_cnt, end_peer);

        Ok(SeedTrustStateResponse::new(result, next_token))
    }
}

#[derive(Debug)]
pub enum Error {
    IndicesNotFound(DomainHash),
    CountNotFound(DomainHash),

    LocalTrustSubTreesNotFoundWithDomain(DomainHash),
    LocalTrustSubTreesNotFoundWithIndex(u64),

    LocalTrustMasterTreeNotFound(DomainHash),
    SeedTrustMasterTreeNotFound(DomainHash),

    LocalTrustNotFound(OwnedNamespace),

    SeedTrustNotFound(OwnedNamespace),

    DomainIndexNotFound(String),

    Merkle(merkle::Error),

    Base64Decode(base64::DecodeError),
    Misc(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Self::IndicesNotFound(domain) => {
                write!(f, "indices not found for domain: {:?}", domain)
            },
            Self::CountNotFound(domain) => write!(f, "count not found for domain: {:?}", domain),

            Self::LocalTrustSubTreesNotFoundWithDomain(domain) => {
                write!(
                    f,
                    "local_trust_sub_trees not found for domain: {:?}",
                    domain
                )
            },
            Self::LocalTrustSubTreesNotFoundWithIndex(index) => {
                write!(f, "local_trust_sub_trees not found for index: {}", index)
            },

            Self::LocalTrustMasterTreeNotFound(domain) => {
                write!(
                    f,
                    "local_trust_master_tree not found for domain: {:?}",
                    domain
                )
            },
            Self::SeedTrustMasterTreeNotFound(domain) => {
                write!(
                    f,
                    "seed_trust_master_tree not found for domain: {:?}",
                    domain
                )
            },
            Self::LocalTrustNotFound(domain) => {
                write!(f, "local_trust not found for domain: {:?}", domain)
            },
            Self::SeedTrustNotFound(domain) => {
                write!(f, "seed_trust not found for domain: {:?}", domain)
            },
            Self::DomainIndexNotFound(address) => {
                write!(f, "domain_indice not found for address: {:?}", address)
            },
            Self::Merkle(err) => err.fmt(f),
            Self::Base64Decode(err) => err.fmt(f),
            Self::Misc(msg) => write!(f, "misc error: {}", msg),
        }
    }
}
