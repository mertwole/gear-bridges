//! ### Circuit that's used to prove that validator with particular index in validator set have
//! ### signed GRANDPA message.

use plonky2::{
    iop::{
        target::Target,
        witness::{PartialWitness, WitnessWrite},
    },
    plonk::{circuit_builder::CircuitBuilder, circuit_data::CircuitConfig},
};

use plonky2_field::types::Field;

use super::{single_validator_sign::SingleValidatorSign, GrandpaMessageTarget};
use crate::{
    block_finality::validator_set_hash::ValidatorSetHashTarget,
    common::{
        targets::{impl_target_set, Blake2Target, TargetSet},
        BuilderExt, ProofWithCircuitData,
    },
    consts::GRANDPA_VOTE_LENGTH,
    prelude::*,
};

impl_target_set! {
    /// Public inputs for `IndexedValidatorSign`.
    pub struct IndexedValidatorSignTarget {
        /// Blake2 hash of concatenated validator set public inputs.
        pub validator_set_hash: Blake2Target,
        /// Overall validator count in validator set.
        pub validator_count: Target,
        /// Validator index that have signed GRANDPA message.
        pub validator_idx: Target,
        /// GRANDPA message.
        pub message: GrandpaMessageTarget,
    }
}

pub struct IndexedValidatorSign {
    /// Public key corresponding to validator at specified index.
    pub public_key: [u8; consts::ED25519_PUBLIC_KEY_SIZE],
    /// Index of validator that've signed the message.
    pub index: usize,
    /// GRANDPA message.
    pub message: [u8; GRANDPA_VOTE_LENGTH],
    /// Signature corresponding to validator at specified index.
    pub signature: [u8; consts::ED25519_SIGNATURE_SIZE],
}

impl IndexedValidatorSign {
    pub fn prove(
        &self,
        valiadtor_set_hash_proof: &ProofWithCircuitData<ValidatorSetHashTarget>,
    ) -> ProofWithCircuitData<IndexedValidatorSignTarget> {
        log::debug!("    Proving indexed validator sign...");

        let sign_proof = SingleValidatorSign {
            public_key: self.public_key,
            signature: self.signature,
            message: self.message,
        }
        .prove();

        let mut builder = CircuitBuilder::new(CircuitConfig::standard_recursion_config());
        let mut witness = PartialWitness::new();

        let validator_set_hash_target =
            builder.recursively_verify_constant_proof(valiadtor_set_hash_proof, &mut witness);

        let index_target = builder.add_virtual_target();
        witness.set_target(index_target, F::from_canonical_usize(self.index));

        let validator = validator_set_hash_target
            .validator_set
            .random_read(index_target, &mut builder);

        let sign_target = builder.recursively_verify_constant_proof(&sign_proof, &mut witness);

        validator.connect(&sign_target.public_key, &mut builder);

        IndexedValidatorSignTarget {
            validator_set_hash: validator_set_hash_target.hash,
            validator_count: validator_set_hash_target.validator_set_length,

            validator_idx: index_target,
            message: sign_target.message,
        }
        .register_as_public_inputs(&mut builder);

        ProofWithCircuitData::prove_from_builder(builder, witness)
    }
}
