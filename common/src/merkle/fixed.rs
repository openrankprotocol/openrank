use crate::merkle::{self, hash_two, Hash};
use getset::Getters;
use rayon::iter::{IndexedParallelIterator, IntoParallelRefIterator, ParallelIterator};
use sha3::Digest;
use std::{collections::HashMap, marker::PhantomData};

#[derive(Clone, Debug, Getters)]
#[getset(get = "pub")]
/// Dense Merkle tree.
/// The dense tree is a tree where leaf nodes are compressed to be next to each other
/// which makes it more efficient to store and traverse.
/// The tree is built from the fixed vector of leaves in the order they are given,
/// and cannot be modified after creation.
pub struct DenseMerkleTree<H>
where
    H: Digest,
{
    /// HashMap to keep the level and index of the nodes.
    nodes: HashMap<u8, Vec<Hash>>,
    // Number of levels
    num_levels: u8,
    /// PhantomData for the hasher
    _h: PhantomData<H>,
}

impl<H> DenseMerkleTree<H>
where
    H: Digest,
{
    /// Returns the root of the tree.
    pub fn root(&self) -> Result<Hash, merkle::Error> {
        self.nodes.get(&self.num_levels).map(|h| h[0].clone()).ok_or(merkle::Error::RootNotFound)
    }

    /// Builds a Merkle tree from the given leaf nodes.
    pub fn new(mut leaves: Vec<Hash>) -> Result<Self, merkle::Error> {
        let next_power_of_two = leaves.len().next_power_of_two();
        if leaves.len() < next_power_of_two {
            let diff = next_power_of_two - leaves.len();
            leaves.extend(vec![Hash::default(); diff]);
        }
        let num_levels = (u64::BITS - next_power_of_two.leading_zeros()) as u8;

        let mut default = Vec::new();
        default.push(Hash::default());
        for i in 1..num_levels as usize {
            let h = hash_two::<H>(default[i - 1].clone(), default[i - 1].clone());
            default.push(h);
        }

        let mut tree = HashMap::new();
        tree.insert(0u8, leaves);

        for i in 0..num_levels {
            let nodes = tree.get(&i).ok_or(merkle::Error::NodesNotFound)?;
            let next: Vec<Hash> = nodes
                .par_iter()
                .chunks(2)
                .map(|chunk| {
                    if chunk.len() == 2 {
                        hash_two::<H>(chunk[0].clone(), chunk[1].clone())
                    } else {
                        hash_two::<H>(chunk[0].clone(), default[i as usize].clone())
                    }
                })
                .collect();
            tree.insert(i + 1, next);
        }

        Ok(Self { nodes: tree, num_levels, _h: PhantomData })
    }
}

#[cfg(test)]
mod test {
    use crate::merkle::{fixed::DenseMerkleTree, Hash};
    use sha3::Keccak256;

    #[test]
    fn should_build_fixed_tree() {
        // Testing build_tree and find_path functions with arity 2
        let leaves = vec![
            Hash::default(),
            Hash::default(),
            Hash::default(),
            Hash::default(),
            Hash::default(),
            Hash::default(),
            Hash::default(),
            Hash::default(),
            Hash::default(),
            Hash::default(),
            Hash::default(),
            Hash::default(),
            Hash::default(),
            Hash::default(),
            Hash::default(),
            Hash::default(),
            Hash::default(),
            Hash::default(),
            Hash::default(),
            Hash::default(),
        ];
        let merkle = DenseMerkleTree::<Keccak256>::new(leaves).unwrap();
        let root = merkle.root().unwrap();

        assert_eq!(
            root.to_hex(),
            "887c22bd8750d34016ac3c66b5ff102dacdd73f6b014e710b51e8022af9a1968".to_string()
        );
    }
}
