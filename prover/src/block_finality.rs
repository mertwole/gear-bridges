use itertools::Itertools;
use plonky2::{
    iop::{
        target::{BoolTarget, Target},
        witness::{PartialWitness, WitnessWrite},
    },
    plonk::{circuit_builder::CircuitBuilder, circuit_data::CircuitConfig},
};

use plonky2_ed25519::gadgets::eddsa::make_verify_circuits as ed25519_circuit;
use plonky2_field::types::Field;
use rayon::{
    iter::{IndexedParallelIterator, IntoParallelIterator, ParallelIterator},
    ThreadPoolBuilder,
};
use std::sync::mpsc::channel;

use crate::{
    common::{
        array_to_bits,
        targets::{
            BitArrayTarget, Blake2Target, Ed25519PublicKeyTarget, Sha256Target, SingleTarget,
            TargetSetOperations, TargetSetWitnessOperations,
        },
        ProofCompositionBuilder, ProofCompositionTargets, TargetSet,
    },
    consts::{
        GRANDPA_VOTE_LENGTH, GRANDPA_VOTE_LENGTH_IN_BITS, PROCESSED_VALIDATOR_COUNT,
        VALIDATOR_COUNT,
    },
    prelude::*,
    validator_set_hash::{ValidatorSetHash, ValidatorSetHashTarget},
    ProofWithCircuitData,
};

const VALIDATOR_SIGN_PROVER_THREAD_MAX_STACK_SIZE: usize = 65_536 * 64;

#[derive(Clone)]
pub struct BlockFinalityTarget {
    pub validator_set_hash: Sha256Target,
    pub message: GrandpaVoteTarget,
}

impl TargetSet for BlockFinalityTarget {
    fn parse(raw: &mut impl Iterator<Item = Target>) -> Self {
        Self {
            validator_set_hash: Sha256Target::parse(raw),
            message: GrandpaVoteTarget::parse(raw),
        }
    }
}

// Assume the layout for vote:
// - ???                    (1 byte)
// - block hash             (32 bytes)
// - block number           (4 bytes)
// - round number           (8 bytes)
// - authority set id       (8 bytes)
#[derive(Clone)]
pub struct GrandpaVoteTarget {
    _aux_data: BitArrayTarget<8>,
    pub block_hash: Blake2Target,
    _aux_data_2: BitArrayTarget<160>,
}

impl TargetSet for GrandpaVoteTarget {
    fn parse(raw: &mut impl Iterator<Item = Target>) -> Self {
        Self {
            _aux_data: BitArrayTarget::parse(raw),
            block_hash: Blake2Target::parse(raw),
            _aux_data_2: BitArrayTarget::parse(raw),
        }
    }
}

#[derive(Clone)]
pub struct PreCommit {
    pub public_key: [u8; consts::ED25519_PUBLIC_KEY_SIZE],
    pub signature: [u8; consts::ED25519_SIGNATURE_SIZE],
}

#[derive(Clone)]
pub struct BlockFinality {
    pub validator_set: [[u8; consts::ED25519_PUBLIC_KEY_SIZE]; VALIDATOR_COUNT],
    pub pre_commits: Vec<PreCommit>,
    pub message: [u8; GRANDPA_VOTE_LENGTH],
}

impl BlockFinality {
    pub fn prove(&self) -> ProofWithCircuitData<BlockFinalityTarget> {
        log::info!("Proving block finality...");

        let processed_pre_commits: Vec<_> = self
            .pre_commits
            .iter()
            .filter_map(|pc| {
                let validator_idx = self.validator_set.iter().position(|v| v == &pc.public_key);
                validator_idx.and_then(|validator_idx| {
                    Some(ProcessedPreCommit {
                        validator_idx,
                        signature: pc.signature,
                    })
                })
            })
            .take(PROCESSED_VALIDATOR_COUNT)
            .collect();

        let validator_set_hash_proof = ValidatorSetHash {
            validator_set: self.validator_set.clone(),
        }
        .prove();

        let validator_signs_proof = ValidatorSignsChain {
            validator_set: self.validator_set.clone(),
            pre_commits: processed_pre_commits,
            message: self.message.clone(),
        }
        .prove();

        log::info!("Composing block finality and validator set hash proofs...");

        let composition_builder =
            ProofCompositionBuilder::new(validator_set_hash_proof, validator_signs_proof);

        let targets_op = |builder: &mut CircuitBuilder<F, D>,
                          targets: ProofCompositionTargets<_, _>| {
            let validator_set_hash_public_inputs: ValidatorSetHashTarget =
                targets.first_proof_public_inputs;
            let validator_signs_public_inputs: ValidatorSignsChainTarget =
                targets.second_proof_public_inputs;

            validator_set_hash_public_inputs
                .hash
                .register_as_public_inputs(builder);

            validator_signs_public_inputs
                .message
                .register_as_public_inputs(builder);

            for (validator_1, validator_2) in validator_set_hash_public_inputs
                .validator_set
                .iter()
                .zip(validator_signs_public_inputs.validator_set.iter())
            {
                validator_1.connect(validator_2, builder);
            }
        };

        composition_builder
            .operation_with_targets(targets_op)
            .build()
    }
}

#[derive(Clone)]
struct ProcessedPreCommit {
    validator_idx: usize,
    signature: [u8; consts::ED25519_SIGNATURE_SIZE],
}

#[derive(Clone)]
struct ValidatorSignsChainTarget {
    validator_idx: SingleTarget,
    validator_set: [Ed25519PublicKeyTarget; VALIDATOR_COUNT],
    message: BitArrayTarget<GRANDPA_VOTE_LENGTH_IN_BITS>,
}

impl TargetSet for ValidatorSignsChainTarget {
    fn parse(raw: &mut impl Iterator<Item = Target>) -> Self {
        Self {
            validator_idx: SingleTarget::parse(raw),
            validator_set: (0..VALIDATOR_COUNT)
                .map(|_| Ed25519PublicKeyTarget::parse(raw))
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            message: BitArrayTarget::parse(raw),
        }
    }
}

struct ValidatorSignsChain {
    validator_set: [[u8; consts::ED25519_PUBLIC_KEY_SIZE]; VALIDATOR_COUNT],
    pre_commits: Vec<ProcessedPreCommit>,
    message: [u8; GRANDPA_VOTE_LENGTH],
}

impl ValidatorSignsChain {
    pub fn prove(&self) -> ProofWithCircuitData<ValidatorSignsChainTarget> {
        log::info!("Proving validator signs chain...");

        let mut pre_commits = self.pre_commits.clone();
        pre_commits.sort_by(|a, b| a.validator_idx.cmp(&b.validator_idx));

        let (sender, receiver) = channel();

        let thread_pool = ThreadPoolBuilder::new()
            .stack_size(VALIDATOR_SIGN_PROVER_THREAD_MAX_STACK_SIZE)
            .build()
            .unwrap();

        pre_commits.into_par_iter().enumerate().for_each_with(
            sender,
            |sender, (id, pre_commit)| {
                thread_pool.scope(|_| {
                    let proof = IndexedValidatorSign {
                        validator_set: self.validator_set.clone(),
                        index: pre_commit.validator_idx,
                        signature: pre_commit.signature,
                        message: self.message.clone(),
                    }
                    .prove();

                    sender.send((id, proof)).unwrap();
                });
            },
        );

        receiver
            .iter()
            .sorted_by(|a, b| a.0.cmp(&b.0))
            .into_iter()
            .map(|(_, proof)| proof)
            .reduce(|acc, x| ComposedValidatorSigns {}.prove(acc, x))
            .unwrap()
    }
}

struct ComposedValidatorSigns {}

impl ComposedValidatorSigns {
    fn prove(
        &self,
        previous_composed_proof: ProofWithCircuitData<ValidatorSignsChainTarget>,
        indexed_sign_proof: ProofWithCircuitData<ValidatorSignsChainTarget>,
    ) -> ProofWithCircuitData<ValidatorSignsChainTarget> {
        log::info!("    Proving validator signs composition...");

        let composition_builder =
            ProofCompositionBuilder::new(previous_composed_proof, indexed_sign_proof);

        let targets_op = |builder: &mut CircuitBuilder<F, D>,
                          targets: ProofCompositionTargets<_, _>| {
            let previous_composed_proof_public_inputs: ValidatorSignsChainTarget =
                targets.first_proof_public_inputs;
            let indexed_sign_proof_public_inputs: ValidatorSignsChainTarget =
                targets.second_proof_public_inputs;

            indexed_sign_proof_public_inputs
                .validator_idx
                .register_as_public_inputs(builder);

            for validator in &indexed_sign_proof_public_inputs.validator_set {
                validator.register_as_public_inputs(builder);
            }
            indexed_sign_proof_public_inputs
                .message
                .register_as_public_inputs(builder);

            previous_composed_proof_public_inputs
                .message
                .connect(&indexed_sign_proof_public_inputs.message, builder);

            for (validator_1, validator_2) in previous_composed_proof_public_inputs
                .validator_set
                .iter()
                .zip(indexed_sign_proof_public_inputs.validator_set.iter())
            {
                validator_1.connect(validator_2, builder);
            }

            let new_index_sub_latest = builder.sub(
                indexed_sign_proof_public_inputs.validator_idx.to_target(),
                previous_composed_proof_public_inputs
                    .validator_idx
                    .to_target(),
            );
            let one = builder.one();
            let to_compare_with_0 = builder.sub(new_index_sub_latest, one); // assert >= 0.
            builder.range_check(to_compare_with_0, 32);
        };

        composition_builder
            .operation_with_targets(targets_op)
            .build()
    }
}

struct IndexedValidatorSign {
    validator_set: [[u8; consts::ED25519_PUBLIC_KEY_SIZE]; VALIDATOR_COUNT],
    index: usize,
    message: [u8; GRANDPA_VOTE_LENGTH],
    signature: [u8; consts::ED25519_SIGNATURE_SIZE],
}

impl IndexedValidatorSign {
    fn prove(&self) -> ProofWithCircuitData<ValidatorSignsChainTarget> {
        log::info!("    Proving indexed validator sign...");

        let selector_proof = ValidatorSelector {
            validator_set: self.validator_set.clone(),
            index: self.index,
        }
        .prove();

        let sign_proof = SingleValidatorSign {
            public_key: self.validator_set[self.index],
            signature: self.signature,
            message: self.message.clone(),
        }
        .prove();

        let composition_builder = ProofCompositionBuilder::new(selector_proof, sign_proof);

        let targets_op = |builder: &mut CircuitBuilder<F, D>,
                          targets: ProofCompositionTargets<_, _>| {
            let selector_proof_public_inputs: ValidatorSelectorTarget =
                targets.first_proof_public_inputs;
            let sign_proof_public_inputs: SingleValidatorSignTarget =
                targets.second_proof_public_inputs;

            selector_proof_public_inputs
                .index
                .register_as_public_inputs(builder);
            for validator in &selector_proof_public_inputs.validator_set {
                validator.register_as_public_inputs(builder);
            }

            sign_proof_public_inputs
                .message
                .register_as_public_inputs(builder);

            selector_proof_public_inputs
                .validator
                .connect(&sign_proof_public_inputs.public_key, builder);
        };

        composition_builder
            .operation_with_targets(targets_op)
            .build()
    }
}

#[derive(Clone)]
struct SingleValidatorSignTarget {
    message: BitArrayTarget<GRANDPA_VOTE_LENGTH_IN_BITS>,
    public_key: Ed25519PublicKeyTarget,
}

impl TargetSet for SingleValidatorSignTarget {
    fn parse(raw: &mut impl Iterator<Item = Target>) -> Self {
        Self {
            message: BitArrayTarget::parse(raw),
            public_key: Ed25519PublicKeyTarget::parse(raw),
        }
    }
}

struct SingleValidatorSign {
    public_key: [u8; consts::ED25519_PUBLIC_KEY_SIZE],
    signature: [u8; consts::ED25519_SIGNATURE_SIZE],
    message: [u8; GRANDPA_VOTE_LENGTH],
}

impl SingleValidatorSign {
    fn prove(&self) -> ProofWithCircuitData<SingleValidatorSignTarget> {
        log::info!("        Proving single validator sign...");

        let mut builder = CircuitBuilder::<F, D>::new(CircuitConfig::wide_ecc_config());

        let targets = ed25519_circuit(&mut builder, self.message.len());

        for target in &targets.msg {
            builder.register_public_input(target.target);
        }

        for target in &targets.pk {
            builder.register_public_input(target.target);
        }

        let mut pw = PartialWitness::new();

        let pk_bits = array_to_bits(&self.public_key).into_iter();
        for (target, value) in targets.pk.iter().zip(pk_bits) {
            pw.set_bool_target(*target, value);
        }

        let signature_bits = array_to_bits(&self.signature).into_iter();
        for (target, value) in targets.sig.iter().zip(signature_bits) {
            pw.set_bool_target(*target, value);
        }

        let msg_bits = array_to_bits(&self.message).into_iter();
        for (target, value) in targets.msg.iter().zip(msg_bits) {
            pw.set_bool_target(*target, value);
        }

        let proof = ProofWithCircuitData::from_builder(builder, pw);

        log::info!("        Proven single validator sign...");

        proof
    }
}

#[derive(Clone)]
struct ValidatorSelectorTarget {
    index: SingleTarget,
    validator_set: [Ed25519PublicKeyTarget; VALIDATOR_COUNT],
    validator: Ed25519PublicKeyTarget,
}

impl TargetSet for ValidatorSelectorTarget {
    fn parse(raw: &mut impl Iterator<Item = Target>) -> Self {
        Self {
            index: SingleTarget::parse(raw),
            validator_set: (0..VALIDATOR_COUNT)
                .map(|_| Ed25519PublicKeyTarget::parse(raw))
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            validator: Ed25519PublicKeyTarget::parse(raw),
        }
    }
}

struct ValidatorSelector {
    validator_set: [[u8; consts::ED25519_PUBLIC_KEY_SIZE]; VALIDATOR_COUNT],
    index: usize,
}

impl ValidatorSelector {
    fn prove(&self) -> ProofWithCircuitData<ValidatorSelectorTarget> {
        log::info!("        Proving validator selection...");

        let mut builder = CircuitBuilder::<F, D>::new(CircuitConfig::wide_ecc_config());

        let targets: ValidatorSelectorTarget =
            validator_selector_circuit(&mut builder, self.validator_set.len());

        builder.register_public_input(targets.index.to_target());

        for validator in &targets.validator_set {
            validator.register_as_public_inputs(&mut builder);
        }

        targets.validator.register_as_public_inputs(&mut builder);

        let mut pw = PartialWitness::new();

        pw.set_target(
            targets.index.to_target(),
            F::from_canonical_u32(self.index as u32),
        );

        for (validator_target, validator_data) in
            targets.validator_set.iter().zip(self.validator_set.iter())
        {
            validator_target.set_partial_witness(validator_data, &mut pw);
        }

        targets
            .validator
            .set_partial_witness(&self.validator_set[self.index], &mut pw);

        ProofWithCircuitData::from_builder(builder, pw)
    }
}

fn validator_selector_circuit(
    builder: &mut CircuitBuilder<F, D>,
    validator_count: usize,
) -> ValidatorSelectorTarget {
    let mut validator_set_targets = Vec::with_capacity(validator_count);
    for _ in 0..validator_count {
        let pk_targets: [BoolTarget; consts::ED25519_PUBLIC_KEY_SIZE_IN_BITS] = (0
            ..consts::ED25519_PUBLIC_KEY_SIZE_IN_BITS)
            .map(|_| builder.add_virtual_bool_target_safe())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        validator_set_targets.push(pk_targets);
    }
    let validator_set_targets: [_; VALIDATOR_COUNT] = validator_set_targets.try_into().unwrap();

    let index_target = builder.add_virtual_target();

    let mut equality_targets = Vec::with_capacity(validator_count);
    for i in 0..validator_count {
        let i_target = builder.constant(F::from_canonical_u32(i as u32));
        let eq_target = builder.is_equal(i_target, index_target);
        equality_targets.push(eq_target);
    }

    let mut validator_targets = Vec::with_capacity(consts::ED25519_PUBLIC_KEY_SIZE_IN_BITS);
    for bit_idx in 0..consts::ED25519_PUBLIC_KEY_SIZE_IN_BITS {
        let target = validator_set_targets
            .iter()
            .zip(equality_targets.iter())
            .map(|(validator, equality)| builder.and(validator[bit_idx], *equality))
            .collect::<Vec<_>>()
            .into_iter()
            .reduce(|acc, x| {
                let not_acc = builder.not(acc);
                let not_x = builder.not(x);
                let not_res = builder.and(not_acc, not_x);
                builder.not(not_res)
            })
            .unwrap();

        validator_targets.push(target);
    }
    let validator_targets: [_; consts::ED25519_PUBLIC_KEY_SIZE_IN_BITS] =
        validator_targets.try_into().unwrap();

    ValidatorSelectorTarget {
        validator_set: validator_set_targets
            .into_iter()
            .map(|v| v.into())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap(),
        index: index_target.into(),
        validator: validator_targets.into(),
    }
}
