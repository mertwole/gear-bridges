//! ### Circuit that's used to parse storage data from `HashedValueLeaf` node.

use plonky2::{iop::target::Target, plonk::circuit_builder::CircuitBuilder};
use plonky2_field::types::Field;

use crate::{
    common::targets::{impl_target_set, ArrayTarget, Blake2Target, TargetSet},
    prelude::{consts::BLAKE2_DIGEST_SIZE, *},
    storage_inclusion::storage_trie_proof::node_parser::LeafNodeDataPaddedTarget,
};

impl_target_set! {
    pub struct HashedDataParserInputTarget {
        /// Encoded node data.
        pub node_data: LeafNodeDataPaddedTarget,
        /// From which offset to read stored data.
        pub read_offset: Target,
    }
}

impl_target_set! {
    pub struct HashedDataParserOutputTarget {
        /// Offset of remaining node data.
        pub resulting_offset: Target,
        /// Blake2 hash of stored data.
        pub data_hash: Blake2Target
    }
}

pub fn define(
    input: HashedDataParserInputTarget,
    builder: &mut CircuitBuilder<F, D>,
) -> HashedDataParserOutputTarget {
    log::debug!("    Composing hashed data parser");

    let hash_data: ArrayTarget<_, BLAKE2_DIGEST_SIZE> = input
        .node_data
        .random_read_array(input.read_offset, builder);

    let mut hash_data_bits = hash_data.0.iter().flat_map(|byte_target| {
        byte_target
            .as_bit_targets(builder)
            .0
            .into_iter()
            .map(|target| target.target)
            .rev()
    });
    let data_hash = Blake2Target::parse_exact(&mut hash_data_bits);

    let resulting_offset = builder.add_const(
        input.read_offset,
        F::from_canonical_usize(BLAKE2_DIGEST_SIZE),
    );

    HashedDataParserOutputTarget {
        resulting_offset,
        data_hash,
    }
}
