use parity_scale_codec::Encode;
use plonky2::{
    iop::witness::{PartialWitness, WitnessWrite},
    plonk::{circuit_builder::CircuitBuilder, circuit_data::CircuitConfig},
};
use plonky2_field::types::Field;
use sp_core::H256;
use trie_db::{
    node::{Node, NodeHandle},
    ChildReference, NodeCodec, TrieLayout,
};

use super::{
    header_parser::{self, HeaderParserInputTarget},
    nibble_parser::{self, NibbleParserInputTarget},
    BranchNodeDataPaddedTarget,
};
use crate::{
    common::{
        targets::{Blake2Target, HalfByteTarget, SingleTarget, TargetSet},
        BuilderExt,
    },
    consts::BLAKE2_DIGEST_SIZE,
    impl_parsable_target_set,
    prelude::*,
    storage_inclusion::storage_trie_proof::{
        node_parser::{
            branch_parser::child_node_array_parser::ChildNodeArrayParserTarget,
            compose_padded_node_data,
        },
        storage_address::PartialStorageAddressTarget,
    },
    ProofWithCircuitData,
};
use bitmap_parser::BitmapParserInputTarget;
use child_node_array_parser::ChildNodeArrayParser;

mod bitmap_parser;
mod child_node_array_parser;

impl_parsable_target_set! {
    pub struct BranchParserTarget {
        pub padded_node_data: BranchNodeDataPaddedTarget,
        pub node_data_length: SingleTarget,

        pub child_node_hash: Blake2Target,

        pub partial_address: PartialStorageAddressTarget,
        pub resulting_partial_address: PartialStorageAddressTarget,
    }
}

pub struct BranchParser {
    pub node_data: Vec<u8>,

    pub claimed_child_node_nibble: u8,
    pub partial_address_nibbles: Vec<u8>,
}

struct Metadata {
    children_data_offset: usize,
    children_lengths: Vec<usize>,
    claimed_child_index_in_array: usize,
    claimed_child_hash: [u8; BLAKE2_DIGEST_SIZE],
}

impl BranchParser {
    pub fn prove(self) -> ProofWithCircuitData<BranchParserTarget> {
        let metadata = self.parse_metadata();

        let child_node_parser_proof = ChildNodeArrayParser {
            initial_data: child_node_array_parser::InitialData {
                node_data: compose_padded_node_data(self.node_data),
                read_offset: metadata.children_data_offset,
                claimed_child_index_in_array: metadata.claimed_child_index_in_array,
                claimed_child_hash: metadata.claimed_child_hash,
            },
            children_lengths: metadata.children_lengths,
        }
        .prove();

        log::info!("Proving branch node parser...");

        let mut config = CircuitConfig::standard_recursion_config();
        config.num_wires = 160;
        config.num_routed_wires = 130;

        let mut builder = CircuitBuilder::new(config);
        let mut witness = PartialWitness::new();

        let node_data_target = BranchNodeDataPaddedTarget::add_virtual_safe(&mut builder);

        let partial_address_target = PartialStorageAddressTarget::add_virtual_unsafe(&mut builder);
        partial_address_target.set_witness(&self.partial_address_nibbles, &mut witness);

        let node_data_length_target: SingleTarget = builder.add_virtual_target().into();

        let claimed_child_node_nibble_target = builder.add_virtual_target();
        witness.set_target(
            claimed_child_node_nibble_target,
            F::from_canonical_u8(self.claimed_child_node_nibble),
        );
        let claimed_child_node_nibble_target =
            HalfByteTarget::from_target_safe(claimed_child_node_nibble_target, &mut builder);

        let child_node_hash_target = Blake2Target::add_virtual_safe(&mut builder);

        let first_node_data_block = node_data_target.constant_read(0);

        let parsed_node_header = {
            let first_bytes = first_node_data_block.constant_read_array(0);

            let input = HeaderParserInputTarget { first_bytes };
            header_parser::define(
                input,
                header_parser::HeaderDescriptor::branch_without_value(),
                &mut builder,
            )
        };

        let parsed_nibbles = {
            let input = NibbleParserInputTarget {
                first_node_data_block: first_node_data_block.clone(),
                read_offset: parsed_node_header.resulting_offset,
                nibble_count: parsed_node_header.nibble_count,
                partial_address: partial_address_target.clone(),
            };
            nibble_parser::define(input, &mut builder)
        };

        let child_nibble_address_part = PartialStorageAddressTarget::from_single_nibble_target(
            claimed_child_node_nibble_target,
            &mut builder,
        );
        let resulting_address = parsed_nibbles
            .partial_address
            .append(child_nibble_address_part, &mut builder);

        let parsed_bitmap = {
            let input = BitmapParserInputTarget {
                first_node_data_block,
                read_offset: parsed_nibbles.resulting_offset,
                claimed_child_node_nibble: claimed_child_node_nibble_target,
            };

            bitmap_parser::define(input, &mut builder)
        };

        {
            let ChildNodeArrayParserTarget {
                node_data,
                initial_read_offset,
                final_read_offset,
                overall_children_amount,
                claimed_child_index_in_array,
                claimed_child_hash,
            } = builder.recursively_verify_constant_proof(child_node_parser_proof, &mut witness);

            node_data.connect(&node_data_target, &mut builder);
            initial_read_offset.connect(&parsed_bitmap.resulting_offset, &mut builder);
            final_read_offset.connect(&node_data_length_target, &mut builder);
            overall_children_amount.connect(&parsed_bitmap.overall_children_amount, &mut builder);
            claimed_child_index_in_array.connect(&parsed_bitmap.child_index_in_array, &mut builder);
            claimed_child_hash.connect(&child_node_hash_target, &mut builder);
        }

        BranchParserTarget {
            padded_node_data: node_data_target,
            node_data_length: node_data_length_target,
            child_node_hash: child_node_hash_target,
            partial_address: partial_address_target,
            resulting_partial_address: resulting_address,
        }
        .register_as_public_inputs(&mut builder);

        let result = ProofWithCircuitData::from_builder(builder, witness);

        log::info!("Proven branch node parser");

        result
    }

    fn parse_metadata(&self) -> Metadata {
        type TrieCodec = <sp_trie::LayoutV1<sp_core::Blake2Hasher> as TrieLayout>::Codec;
        let node = TrieCodec::decode(&self.node_data).unwrap();

        if let Node::NibbledBranch(_, children, value) = node {
            assert!(value.is_none(), "Non-empty value is not supported");

            let children: [Option<ChildReference<H256>>; 16] =
                children.map(|child| child.map(|child| child.try_into().unwrap()));

            let claimed_child_hash = if let Some(ChildReference::Hash(child_hash)) =
                &children[self.claimed_child_node_nibble as usize]
            {
                child_hash.0
            } else {
                panic!("Unsupported claimed child");
            };

            let mut claimed_child_index_in_array = 0;
            for child_idx in 0..self.claimed_child_node_nibble {
                if children[child_idx as usize].is_some() {
                    claimed_child_index_in_array += 1;
                }
            }

            let mut children_lengths = vec![];
            for child in children {
                let serialized_size = match child {
                    Some(ChildReference::Hash(hash)) => hash.as_bytes().encode().len(),
                    Some(ChildReference::Inline(data, len)) => data[..len].encode().len(),
                    None => continue,
                };
                children_lengths.push(serialized_size);
            }

            let all_children_length: usize = children_lengths.iter().sum();
            let children_data_offset = self.node_data.len() - all_children_length;

            Metadata {
                children_data_offset,
                children_lengths,
                claimed_child_index_in_array,
                claimed_child_hash,
            }
        } else {
            panic!("Unexpected node type: expected NibbledBranch")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::iter;
    use trie_db::{ChildReference, NibbleSlice};

    use super::*;
    use crate::common::{pad_byte_vec, targets::ParsableTargetSet};

    #[test]
    fn test_branch_node_parser_single_child() {
        test_case(NibbleSlice::new(&[]), single_claimed_child([0; 32], 0), 0);

        test_case(
            NibbleSlice::new(&[]),
            single_claimed_child([0xA; 32], 15),
            15,
        );
    }

    #[test]
    fn test_branch_node_parser_all_children() {
        let all_children = [Some(ChildReference::Hash(H256([0; 32]))); 16];

        test_case(NibbleSlice::new(&[]), all_children, 15);
    }

    #[test]
    fn test_branch_node_parser_nibbles() {
        test_case(
            NibbleSlice::new(&[0x22, 0xBB, 0x00, 0xDD]),
            single_claimed_child([0; 32], 0),
            0,
        );

        test_case(
            NibbleSlice::new_offset(&[0x02, 0xBB, 0x00, 0xDD], 1),
            single_claimed_child([0; 32], 15),
            15,
        );
    }

    fn single_claimed_child(
        hash: [u8; BLAKE2_DIGEST_SIZE],
        position: usize,
    ) -> [Option<ChildReference<H256>>; 16] {
        vec![None; position]
            .into_iter()
            .chain(iter::once(Some(ChildReference::Hash(H256(hash)))))
            .chain(iter::repeat(None))
            .take(16)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
    }

    fn test_case(
        nibbles: NibbleSlice,
        children: [Option<ChildReference<H256>>; 16],
        claimed_child_node_nibble: u8,
    ) {
        type TrieCodec = <sp_trie::LayoutV1<sp_core::Blake2Hasher> as TrieLayout>::Codec;

        let node_data = TrieCodec::branch_node_nibbled(
            nibbles.right_iter(),
            nibbles.len(),
            children.into_iter(),
            None,
        );

        let claimed_child_hash = if let Some(ChildReference::Hash(hash)) =
            children[claimed_child_node_nibble as usize]
        {
            hash.0
        } else {
            panic!("Invalid claimed_child_node_nibble");
        };

        let circuit_input = BranchParser {
            node_data,
            claimed_child_node_nibble,
            partial_address_nibbles: vec![],
        };

        let nibble_count = nibbles.len();
        let expected_address_nibbles = (0..nibble_count)
            .map(|idx| nibbles.at(idx))
            .chain(std::iter::once(claimed_child_node_nibble))
            .collect::<Vec<_>>();

        let proof = circuit_input.prove();
        let pis = BranchParserTarget::parse_public_inputs_exact(&mut proof.pis().into_iter());

        assert!(proof.verify());

        assert_eq!(
            pis.resulting_partial_address.length,
            expected_address_nibbles.len() as u64
        );
        assert_eq!(
            pis.resulting_partial_address.address,
            pad_byte_vec(expected_address_nibbles)
        );
    }
}
