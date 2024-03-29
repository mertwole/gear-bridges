extern crate pretty_env_logger;

use clap::{Args, Parser, Subcommand};
use intermediate_proof_storage::{PersistentMockProofStorage, ProofStorage};
use pretty_env_logger::env_logger::fmt::TimestampPrecision;
use std::path::PathBuf;

use gear_rpc_client::{BlockInclusionProof, GearApi};
use prover::{
    common::targets::ParsableTargetSet,
    final_proof::FinalProof,
    latest_validator_set::{LatestValidatorSet, LatestValidatorSetTarget},
    message_sent::MessageSent,
    next_validator_set::NextValidatorSet,
    prelude::GENESIS_AUTHORITY_SET_ID,
    storage_inclusion::{BranchNodeData, StorageInclusion},
};

const DEFAULT_VARA_RPC: &str = "wss://testnet-archive.vara-network.io:443";

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: CliCommands,
}

#[derive(Subcommand)]
enum CliCommands {
    /// Generate zk-proofs
    #[clap(visible_alias("p"))]
    #[command(subcommand)]
    Prove(ProveCommands),
}

#[derive(Subcommand)]
enum ProveCommands {
    /// Generate genesis proof
    #[clap(visible_alias("g"))]
    Genesis {
        #[clap(flatten)]
        args: ProveArgs,
    },
    /// Prove that validator set has changed
    #[clap(visible_alias("v"))]
    ValidatorSetChange {
        #[clap(flatten)]
        args: ProveArgs,
    },
    /// Generate final proof
    #[clap(visible_alias("w"))]
    Wrapped {
        #[clap(flatten)]
        args: ProveArgs,
        /// Where to write proof with public inputs
        #[arg(
            long = "proof-with-public-inputs-path",
            default_value = "./gnark-wrapper/data/proof_with_public_inputs.json"
        )]
        proof_with_public_inputs_path: PathBuf,
        /// Where to write common circuit data
        #[arg(
            long = "common-circuit-data-path",
            default_value = "./gnark-wrapper/data/common_circuit_data.json"
        )]
        common_circuit_data_path: PathBuf,
        /// Where to write verifier only circuit data
        #[arg(
            long = "verifier-only-circuit-data-path",
            default_value = "./gnark-wrapper/data/verifier_only_circuit_data.json"
        )]
        verifier_only_circuit_data_path: PathBuf,
    },
}

#[derive(Args)]
struct ProveArgs {
    #[clap(flatten)]
    vara_endpoint: VaraEndpointArg,
}

#[derive(Args)]
struct VaraEndpointArg {
    /// Address of the VARA RPC endpoint
    #[arg(
        long = "vara-endpoint",
        default_value = DEFAULT_VARA_RPC
    )]
    vara_endpoint: String,
}

#[tokio::main]
async fn main() {
    pretty_env_logger::formatted_builder()
        .filter_level(log::LevelFilter::Info)
        .format_target(false)
        .format_timestamp(Some(TimestampPrecision::Seconds))
        .init();

    let cli = Cli::parse();

    let mut proof_storage = PersistentMockProofStorage::new("./proof_storage".into());

    match cli.command {
        CliCommands::Prove(prove_command) => match prove_command {
            ProveCommands::Genesis { args } => {
                let api = GearApi::new(&args.vara_endpoint.vara_endpoint).await;
                let (block, current_epoch_block_finality) = api
                    .fetch_finality_proof_for_session(GENESIS_AUTHORITY_SET_ID)
                    .await;

                let next_validator_set_inclusion_proof =
                    api.fetch_next_session_keys_inclusion_proof(block).await;
                let next_validator_set_storage_data = next_validator_set_inclusion_proof
                    .storage_inclusion_proof
                    .storage_data
                    .clone();
                let next_validator_set_inclusion_proof =
                    parse_rpc_inclusion_proof(next_validator_set_inclusion_proof);

                let change_from_genesis = NextValidatorSet {
                    current_epoch_block_finality,
                    next_validator_set_inclusion_proof,
                    next_validator_set_storage_data,
                };

                let genesis_proof = LatestValidatorSet {
                    change_proof: change_from_genesis,
                }
                .prove_genesis();

                proof_storage
                    .init(genesis_proof.verifier_circuit_data(), genesis_proof.proof())
                    .unwrap();
            }
            ProveCommands::ValidatorSetChange { args } => {
                let api = GearApi::new(&args.vara_endpoint.vara_endpoint).await;

                let latest_proof = proof_storage
                    .get_latest_proof()
                    .expect("No latest proof found");
                let latest_proof_public_inputs = LatestValidatorSetTarget::parse_public_inputs(
                    &mut latest_proof.public_inputs.clone().into_iter(),
                );

                let validator_set_id = latest_proof_public_inputs.current_set_id;

                let (block, current_epoch_block_finality) =
                    api.fetch_finality_proof_for_session(validator_set_id).await;

                let next_validator_set_inclusion_proof =
                    api.fetch_next_session_keys_inclusion_proof(block).await;
                let next_validator_set_storage_data = next_validator_set_inclusion_proof
                    .storage_inclusion_proof
                    .storage_data
                    .clone();
                let next_validator_set_inclusion_proof =
                    parse_rpc_inclusion_proof(next_validator_set_inclusion_proof);

                let next_change = NextValidatorSet {
                    current_epoch_block_finality,
                    next_validator_set_inclusion_proof,
                    next_validator_set_storage_data,
                };

                let validator_set_change_proof = LatestValidatorSet {
                    change_proof: next_change,
                }
                .prove_recursive(latest_proof);

                proof_storage
                    .update(validator_set_change_proof.proof())
                    .unwrap();
            }
            ProveCommands::Wrapped {
                args,
                proof_with_public_inputs_path,
                common_circuit_data_path,
                verifier_only_circuit_data_path,
            } => {
                let api = GearApi::new(&args.vara_endpoint.vara_endpoint).await;

                let latest_proof = proof_storage
                    .get_latest_proof()
                    .expect("No latest proof found");
                let latest_proof_public_inputs = LatestValidatorSetTarget::parse_public_inputs(
                    &mut latest_proof.public_inputs.clone().into_iter(),
                );

                let block = api
                    .search_for_validator_set_block(latest_proof_public_inputs.current_set_id)
                    .await;
                let (block, block_finality) = api.fetch_finality_proof(block).await;

                let sent_message_inclusion_proof =
                    api.fetch_sent_message_inclusion_proof(block).await;
                let inclusion_proof: StorageInclusion = todo!();

                let message_sent = MessageSent {
                    block_finality,
                    inclusion_proof,
                };

                let final_proof = FinalProof {
                    current_validator_set: todo!(), // latest_proof here
                    message_sent,
                }
                .prove();

                let final_serialized = final_proof.export_wrapped();

                std::fs::write(
                    proof_with_public_inputs_path,
                    final_serialized.proof_with_public_inputs,
                )
                .unwrap();
                std::fs::write(
                    common_circuit_data_path,
                    final_serialized.common_circuit_data,
                )
                .unwrap();
                std::fs::write(
                    verifier_only_circuit_data_path,
                    final_serialized.verifier_only_circuit_data,
                )
                .unwrap();
            }
        },
    };
}

fn parse_rpc_inclusion_proof(proof: BlockInclusionProof) -> StorageInclusion {
    StorageInclusion {
        block_header_data: proof.encoded_header,
        branch_node_data: proof
            .storage_inclusion_proof
            .branch_nodes_data
            .into_iter()
            .rev()
            .map(|d| BranchNodeData {
                data: d.encoded_node,
                child_nibble: d.child_nibble,
            })
            .collect(),
        leaf_node_data: proof.storage_inclusion_proof.encoded_leaf_node,
    }
}
