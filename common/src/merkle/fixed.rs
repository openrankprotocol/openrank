use super::{hash_two, Hash};
use sha3::Digest;
use std::{collections::HashMap, marker::PhantomData};

#[derive(Clone, Debug)]
/// MerkleTree structure
pub struct DenseMerkleTree<H>
where
	H: Digest,
{
	/// HashMap to keep the level and index of the nodes
	pub(crate) nodes: HashMap<u8, Vec<Hash>>,
	// Number of levels
	num_levels: u8,
	/// PhantomData for the hasher
	_h: PhantomData<H>,
}

impl<H> DenseMerkleTree<H>
where
	H: Digest,
{
	pub fn root(&self) -> Hash {
		self.nodes.get(&self.num_levels).unwrap()[0].clone()
	}

	/// Build a MerkleTree from given leaf nodes and height
	pub fn new(mut leaves: Vec<Hash>) -> Self {
		let next_power_of_two = leaves.len().next_power_of_two();
		if leaves.len() < next_power_of_two {
			let diff = next_power_of_two - leaves.len();
			leaves.extend(vec![Hash::default(); diff]);
		}
		let num_levels = (u32::BITS - next_power_of_two.leading_zeros()) as u8;

		let mut default = Vec::new();
		default.push(Hash::default());
		for i in 1..num_levels as usize {
			let h = hash_two::<H>(default[i - 1].clone(), default[i - 1].clone());
			default.push(h);
		}

		let mut tree = HashMap::new();
		tree.insert(0u8, leaves);

		for i in 0..num_levels as u8 {
			let nodes = tree.get(&i).unwrap();
			let next: Vec<Hash> = nodes
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

		Self { nodes: tree, num_levels, _h: PhantomData }
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
		let merkle = DenseMerkleTree::<Keccak256>::new(leaves);
		let root = merkle.root();

		assert_eq!(
			root.to_hex(),
			"27ae5ba08d7291c96c8cbddcc148bf48a6d68c7974b94356f53754ef6171d757".to_string()
		);
	}
}
