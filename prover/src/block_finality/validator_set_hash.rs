//! ### Circuit that's used to prove correct hashing of validator set.

use plonky2::{
    iop::{
        target::Target,
        witness::{PartialWitness, WitnessWrite},
    },
    plonk::{circuit_builder::CircuitBuilder, circuit_data::CircuitConfig},
};
use plonky2_field::types::Field;
use sp_core::blake2_256;

use crate::{
    common::{
        generic_blake2::GenericBlake2,
        targets::{Blake2Target, PaddedValidatorSetTarget, TargetSet},
        BuilderExt, ProofWithCircuitData,
    },
    consts::{ED25519_PUBLIC_KEY_SIZE, MAX_VALIDATOR_COUNT},
    impl_target_set,
    prelude::*,
};

use self::consts::BLAKE2_DIGEST_SIZE;

impl_target_set! {
    /// Public inputs for `ValidatorSetHash`.
    pub struct ValidatorSetHashTarget {
        /// Blake2 hash of validator set.
        pub hash: Blake2Target,
        /// Validator set. It's padded to allow for generic validator set length to be processed.
        pub validator_set: PaddedValidatorSetTarget,
        /// Actual length of validator set.
        pub validator_set_length: Target
    }
}

pub struct ValidatorSetHash {
    /// All the validators participating in GRANDPA voting.
    pub validator_set: Vec<[u8; ED25519_PUBLIC_KEY_SIZE]>,
}

impl ValidatorSetHash {
    pub fn compute_hash(&self) -> [u8; BLAKE2_DIGEST_SIZE] {
        blake2_256(
            &self
                .validator_set
                .iter()
                .flatten()
                .copied()
                .collect::<Vec<_>>(),
        )
    }

    pub fn prove(self) -> ProofWithCircuitData<ValidatorSetHashTarget> {
        log::debug!("Proving correct hashing of validator set...");

        let validator_count = self.validator_set.len();

        const MAX_DATA_LENGTH_ESTIMATION: usize = ED25519_PUBLIC_KEY_SIZE * MAX_VALIDATOR_COUNT;
        let hasher_proof = GenericBlake2::new::<MAX_DATA_LENGTH_ESTIMATION>(
            self.validator_set.into_iter().flatten().collect(),
        )
        .prove();

        let mut builder = CircuitBuilder::<F, D>::new(CircuitConfig::standard_recursion_config());
        let mut pw = PartialWitness::new();

        let validator_count_target = builder.add_virtual_target();
        pw.set_target(
            validator_count_target,
            F::from_canonical_usize(validator_count),
        );

        let hasher_pis = builder.recursively_verify_constant_proof(&hasher_proof, &mut pw);
        let desired_data_len = builder.mul_const(
            F::from_canonical_usize(ED25519_PUBLIC_KEY_SIZE),
            validator_count_target,
        );
        builder.connect(desired_data_len, hasher_pis.length);

        let mut validator_set = hasher_pis.data.0.into_iter().flat_map(|byte| {
            byte.as_bit_targets(&mut builder)
                .0
                .into_iter()
                .rev()
                .map(|t| t.target)
        });
        let validator_set = PaddedValidatorSetTarget::parse(&mut validator_set);

        ValidatorSetHashTarget {
            hash: hasher_pis.hash,
            validator_set_length: validator_count_target,
            validator_set,
        }
        .register_as_public_inputs(&mut builder);

        let result = ProofWithCircuitData::prove_from_builder(builder, pw);

        log::debug!("Proven correct hashing of validator set");

        result
    }
}
