use plonky2::plonk::circuit_builder::CircuitBuilder;
use plonky2_field::types::Field;

use crate::{
    common::targets::{impl_target_set, ArrayTarget, ByteTarget, SingleTarget},
    prelude::*,
};

impl_target_set! {
    pub struct HeaderParserInputTarget {
        pub first_bytes: ArrayTarget<ByteTarget, 2>,
    }
}

impl_target_set! {
    pub struct HeaderParserOutputTarget {
        pub nibble_count: SingleTarget,
        pub resulting_offset: SingleTarget,
    }
}

pub struct HeaderDescriptor {
    masked_prefix: u8,
    prefix_length: usize,
}

impl HeaderDescriptor {
    pub fn branch_without_value() -> HeaderDescriptor {
        HeaderDescriptor {
            masked_prefix: 0b10_00_00_00,
            prefix_length: 2,
        }
    }

    pub fn hashed_value_leaf() -> HeaderDescriptor {
        HeaderDescriptor {
            masked_prefix: 0b00_10_00_00,
            prefix_length: 3,
        }
    }
}

pub fn define(
    input: HeaderParserInputTarget,
    descriptor: HeaderDescriptor,
    builder: &mut CircuitBuilder<F, D>,
) -> HeaderParserOutputTarget {
    let first_byte = input.first_bytes.0[0].clone();
    let second_byte = input.first_bytes.0[1].clone();

    let first_byte_bits = first_byte.to_bit_targets(builder);

    for bit_idx in 8 - descriptor.prefix_length..8 {
        let target = first_byte_bits.constant_read(bit_idx);
        let desired_value = (descriptor.masked_prefix >> bit_idx) % 2 == 1;
        let desired_value = builder.constant_bool(desired_value);
        builder.connect(target.target, desired_value.target);
    }

    let masked_prefix = builder.constant(F::from_canonical_u8(descriptor.masked_prefix));
    let first_byte_value = builder.sub(first_byte.to_target(), masked_prefix);
    let second_byte_value = second_byte.to_target();

    let max_first_byte_value = builder.constant(F::from_canonical_u8(
        0b11_11_11_11 >> descriptor.prefix_length,
    ));
    let max_second_byte_value = builder.constant(F::from_canonical_u8(0b11_11_11_11));

    let first_val_is_max = builder.is_equal(first_byte_value, max_first_byte_value);
    let second_val_is_max = builder.is_equal(second_byte_value, max_second_byte_value);

    // First case:
    // 0b[PREFIX][value != 0b11...11]
    let first_case_appliable = builder.not(first_val_is_max);

    // Second case:
    // 0b[PREFIX]11.. 0b[8-bit value < 255]
    let second_case_appliable = {
        let second_val_not_max = builder.not(second_val_is_max);
        builder.and(first_val_is_max, second_val_not_max)
    };

    // There are more cases, like 0b[PREFIX]11... 0b11111111 0b[8-bit value < 255]
    // but we don't process them.

    let any_case_appliable = builder.or(first_case_appliable, second_case_appliable);
    builder.assert_one(any_case_appliable.target);
    // after this check we can be sure that first_case_appliable = not(second_case_appliable)

    let first_case_value = first_byte_value;
    let second_case_value = builder.add(second_byte_value, max_first_byte_value);

    let one_target = builder.one();
    let two_target = builder.two();
    // if b { x } else { y } corresponding to docs.
    let nibble_count = builder.select(first_case_appliable, first_case_value, second_case_value);
    let resulting_offset = builder.select(first_case_appliable, one_target, two_target);

    HeaderParserOutputTarget {
        nibble_count: nibble_count.into(),
        resulting_offset: resulting_offset.into(),
    }
}

#[cfg(test)]
mod tests {
    use plonky2::{iop::witness::PartialWitness, plonk::circuit_data::CircuitConfig};

    use super::*;

    #[test]
    fn test_branch_header_parser() {
        test_case(
            &[0b10_01_00_00, 0],
            HeaderDescriptor::branch_without_value(),
            Some(ExpectedData {
                nibble_count: 16,
                resulting_offset: 1,
            }),
        );

        test_case(
            &[0b10_11_11_11, 0b_00_00_00_00],
            HeaderDescriptor::branch_without_value(),
            Some(ExpectedData {
                nibble_count: 63,
                resulting_offset: 2,
            }),
        );

        test_case(
            &[0b10_11_11_11, 0b_10_00_00_00],
            HeaderDescriptor::branch_without_value(),
            Some(ExpectedData {
                nibble_count: 63 + 128,
                resulting_offset: 2,
            }),
        );
    }

    #[test]
    #[should_panic(
        expected = "Partition containing Wire(Wire { row: 4, column: 3 }) was set twice with different values: 1 != 0"
    )]
    fn test_branch_header_parser_value_overflow_panics() {
        test_case(
            &[0b10_11_11_11, 0b11_11_11_11],
            HeaderDescriptor::branch_without_value(),
            None,
        );
    }

    #[test]
    #[should_panic(
        expected = "Partition containing Wire(Wire { row: 0, column: 8 }) was set twice with different values: 1 != 0"
    )]
    fn test_branch_header_parser_wrong_prefix_panics() {
        test_case(
            &[0b00_00_00_11, 0],
            HeaderDescriptor::branch_without_value(),
            None,
        );
    }

    #[test]
    fn test_leaf_header_parser() {
        test_case(
            &[0b00_11_00_00, 0],
            HeaderDescriptor::hashed_value_leaf(),
            Some(ExpectedData {
                nibble_count: 16,
                resulting_offset: 1,
            }),
        );

        test_case(
            &[0b00_11_11_11, 0b_00_00_00_00],
            HeaderDescriptor::hashed_value_leaf(),
            Some(ExpectedData {
                nibble_count: 31,
                resulting_offset: 2,
            }),
        );

        test_case(
            &[0b00_11_11_11, 0b_10_00_00_00],
            HeaderDescriptor::hashed_value_leaf(),
            Some(ExpectedData {
                nibble_count: 31 + 128,
                resulting_offset: 2,
            }),
        );
    }

    #[test]
    #[should_panic(
        expected = "Partition containing Wire(Wire { row: 4, column: 3 }) was set twice with different values: 1 != 0"
    )]
    fn test_leaf_header_parser_value_overflow_panics() {
        test_case(
            &[0b00_11_11_11, 0b11_11_11_11],
            HeaderDescriptor::hashed_value_leaf(),
            None,
        );
    }

    #[test]
    #[should_panic(
        expected = "Partition containing Wire(Wire { row: 0, column: 6 }) was set twice with different values: 1 != 0"
    )]
    fn test_leaf_header_parser_wrong_prefix_panics() {
        test_case(
            &[0b10_00_00_11, 0],
            HeaderDescriptor::hashed_value_leaf(),
            None,
        );
    }

    struct ExpectedData {
        nibble_count: u64,
        resulting_offset: u64,
    }

    fn test_case(
        first_bytes: &[u8; 2],
        header_descriptor: HeaderDescriptor,
        expected_data: Option<ExpectedData>,
    ) {
        let config = CircuitConfig::standard_recursion_config();
        let mut builder = CircuitBuilder::<F, D>::new(config);
        let pw = PartialWitness::new();

        let byte_targets = first_bytes
            .into_iter()
            .map(|value| ByteTarget::constant(*value, &mut builder))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();

        let input_target = HeaderParserInputTarget {
            first_bytes: ArrayTarget(byte_targets),
        };

        let output_target = define(input_target, header_descriptor, &mut builder);

        if let Some(ExpectedData {
            nibble_count,
            resulting_offset,
        }) = expected_data
        {
            let nibble_count = builder.constant(F::from_canonical_u64(nibble_count));
            let resulting_offset = builder.constant(F::from_canonical_u64(resulting_offset));

            builder.connect(nibble_count, output_target.nibble_count.to_target());
            builder.connect(resulting_offset, output_target.resulting_offset.to_target());
        }

        let circuit = builder.build::<C>();
        let proof = circuit.prove(pw);

        let proof = proof.expect("Failed to prove");
        circuit.verify(proof).expect("Failed to verify");
    }
}
