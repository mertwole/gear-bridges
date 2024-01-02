extern crate pretty_env_logger;

use gear_rpc_client::GearApi;
use prover::{message_sent::MessageSent, next_validator_set::NextValidatorSet};
use tokio::runtime::Builder;

fn main() {
    let runtime = Builder::new_multi_thread()
        .thread_stack_size(2 * 1024 * 1024)
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(async {
        main_inner().await;
    });
}

async fn main_inner() {
    pretty_env_logger::init();

    let api = GearApi::new().await;
    let block = api.latest_finalized_block().await;

    let now = std::time::Instant::now();

    let proof = MessageSent {
        block_finality: api.fetch_finality_proof(block).await,
        inclusion_proof: api.fetch_sent_message_merkle_proof(block).await,
    }
    .prove();

    panic!(
        "verified: {} in {}ms",
        proof.verify(),
        now.elapsed().as_millis()
    );

    // let now = std::time::Instant::now();

    // let block = api.latest_finalized_block().await;

    // let proof = NextValidatorSet {
    //     current_epoch_block_finality: api.fetch_finality_proof(block).await,
    //     next_validator_set_inclusion_proof: api.fetch_next_authorities_merkle_proof(block).await,
    // }
    // .prove();

    // let serialized = proof.serialize();

    // println!(
    //     "{} \n\n {} \n\n {}",
    //     serialized.common_circuit_data,
    //     serialized.proof_with_public_inputs,
    //     serialized.verifier_only_circuit_data
    // );

    // println!(
    //     "verified: {} in {}ms",
    //     proof.verify(),
    //     now.elapsed().as_millis()
    // );
}
