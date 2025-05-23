use primitive_types::U256;

use ethereum_client::EthApi;
use gear_rpc_client::GearApi;
use tokio::sync::mpsc::unbounded_channel;

use crate::message_relayer::common::{
    ethereum::{
        block_listener::BlockListener as EthereumBlockListener,
        merkle_root_extractor::MerkleRootExtractor, message_sender::MessageSender,
    },
    GearBlockNumber, MessageInBlock,
};

pub async fn relay(
    gear_api: GearApi,
    eth_api: EthApi,
    message_nonce: U256,
    gear_block: u32,
    from_eth_block: Option<u64>,
) {
    let from_eth_block = if let Some(block) = from_eth_block {
        block
    } else {
        eth_api
            .finalized_block_number()
            .await
            .expect("Failed to get finalized block number on ethereum")
    };

    let gear_block_hash = gear_api
        .block_number_to_hash(gear_block)
        .await
        .expect("Failed to fetch block hash by number");

    let message_queued_events = gear_api
        .message_queued_events(gear_block_hash)
        .await
        .expect("Failed to fetch MessageQueued events from gear block");

    let message = message_queued_events
        .into_iter()
        .find(|m| U256::from_little_endian(&m.nonce_le) == message_nonce)
        .unwrap_or_else(|| {
            panic!(
                "Message with nonce {} is not found in gear block {}",
                message_nonce, gear_block
            )
        });

    let message_in_block = MessageInBlock {
        message,
        block: GearBlockNumber(gear_block),
        block_hash: gear_block_hash,
    };

    let (queued_messages_sender, queued_messages_receiver) = unbounded_channel();

    let ethereum_block_listener = EthereumBlockListener::new(eth_api.clone(), from_eth_block);
    let merkle_root_extractor = MerkleRootExtractor::new(eth_api.clone(), gear_api.clone());
    let message_sender = MessageSender::new(eth_api, gear_api);

    let ethereum_blocks = ethereum_block_listener.run().await;
    let merkle_roots = merkle_root_extractor.run(ethereum_blocks).await;
    message_sender
        .run(queued_messages_receiver, merkle_roots)
        .await;

    queued_messages_sender
        .send(message_in_block)
        .expect("Failed to send message to channel");
}
