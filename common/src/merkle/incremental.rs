use sha3::Digest;
use std::{collections::HashMap, marker::PhantomData};

use super::{hash_two, next_index, num_to_bits_vec, Hash};

#[derive(Clone, Debug)]
/// MerkleTree structure
pub struct DenseIncrementalMerkleTree<H>
where
	H: Digest,
{
	/// HashMap to keep the level and index of the nodes
	pub(crate) nodes: HashMap<(u8, u32), Hash>,
	/// Default nodes
	default: HashMap<(u8, u32), Hash>,
	// Number of levels
	num_levels: u8,
	/// PhantomData for the hasher
	_h: PhantomData<H>,
}

impl<H> DenseIncrementalMerkleTree<H>
where
	H: Digest,
{
	pub fn root(&self) -> Hash {
		match self.nodes.get(&(self.num_levels, 0)) {
			Some(h) => h.clone(),
			None => {
				eprintln!("Tree is empty");
				todo!("Tree is empty")
			},
		}
	}

	/// Build a MerkleTree from given leaf nodes and height
	pub fn new(num_levels: u8) -> Self {
		let mut default: HashMap<(u8, u32), Hash> = HashMap::new();
		default.insert((0, 0), Hash::default());
		for i in 0..num_levels as usize {
			let h = hash_two::<H>(
				default[&(i as u8, 0u32)].clone(),
				default[&(i as u8, 0u32)].clone(),
			);
			default.insert(((i + 1) as u8, 0), h);
		}

		Self { nodes: default.clone(), default, num_levels, _h: PhantomData }
	}

	pub fn insert_leaf(&mut self, index: u32, leaf: Hash) {
		let max_size = 2u32.pow(self.num_levels as u32) - 1;
		assert!(index < max_size);
		let bits = num_to_bits_vec(index);

		self.nodes.insert((0, index), leaf.clone());

		let mut curr_index = index;
		let mut curr_node = leaf;
		for i in 0..self.num_levels {
			let (left, right) = if bits[i as usize] {
				let n_key = (i, curr_index - 1);
				let n = self.nodes.get(&n_key).unwrap_or(&self.default[&(i, 0)]);
				(n.clone(), curr_node)
			} else {
				let n_key = (i, curr_index + 1);
				let n = self.nodes.get(&n_key).unwrap_or(&self.default[&(i, 0)]);
				(curr_node, n.clone())
			};

			let h = hash_two::<H>(left, right);
			curr_node = h;
			curr_index = next_index(curr_index);

			self.nodes.insert((i + 1, curr_index), curr_node.clone());
		}
	}

	pub fn insert_batch(&mut self, mut index: u32, leaves: Vec<Hash>) {
		for leaf in leaves {
			self.insert_leaf(index, leaf);
			index += 1;
		}
	}
}

#[cfg(test)]
mod test {
	use crate::merkle::{incremental::DenseIncrementalMerkleTree, Hash};
	use sha3::Keccak256;

	#[test]
	fn should_build_incremental_tree() {
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
		let mut merkle = DenseIncrementalMerkleTree::<Keccak256>::new(32);
		merkle.insert_batch(0, leaves);
		let root = merkle.root();

		assert_eq!(
			root.to_hex(),
			"27ae5ba08d7291c96c8cbddcc148bf48a6d68c7974b94356f53754ef6171d757".to_string()
		);
	}
}
