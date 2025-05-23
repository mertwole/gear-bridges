use super::{
    beacon::{BlockHeader as BeaconBlockHeader, SyncCommittee},
    network::Network,
    utils, TreeHash,
};
use ring::digest::{Context as RingContext, SHA256 as RingSHA256};

/// According to Ethereum spec [v1.4.0](https://github.com/ethereum/consensus-specs/blob/v1.4.0/specs/altair/light-client/sync-protocol.md#constants).
///
/// It is the result of applying [get_generalized_index_length](https://github.com/ethereum/consensus-specs/blob/v1.4.0/ssz/merkle-proofs.md#get_generalized_index_length) to the `CURRENT_SYNC_COMMITTEE_GINDEX` value.
pub const DEPTH_CURRENT_SYNC_COMMITTEE: u32 = 5;
/// According to Ethereum spec [v1.4.0](https://github.com/ethereum/consensus-specs/blob/v1.4.0/specs/altair/light-client/sync-protocol.md#constants).
///
/// It is the result of applying [get_subtree_index](https://github.com/ethereum/consensus-specs/blob/v1.4.0/specs/altair/light-client/sync-protocol.md#get_subtree_index) to the `CURRENT_SYNC_COMMITTEE_GINDEX` value.
pub const INDEX_CURRENT_SYNC_COMMITTEE: u32 = 22;
/// According to Ethereum spec [v1.4.0](https://github.com/ethereum/consensus-specs/blob/v1.4.0/specs/altair/light-client/sync-protocol.md#constants).
///
/// It is the result of applying [get_generalized_index_length](https://github.com/ethereum/consensus-specs/blob/v1.4.0/ssz/merkle-proofs.md#get_generalized_index_length) to the `NEXT_SYNC_COMMITTEE_GINDEX` value.
pub const DEPTH_NEXT_SYNC_COMMITTEE: u32 = 5;
/// According to Ethereum spec [v1.4.0](https://github.com/ethereum/consensus-specs/blob/v1.4.0/specs/altair/light-client/sync-protocol.md#constants).
///
/// It is the result of applying [get_subtree_index](https://github.com/ethereum/consensus-specs/blob/v1.4.0/specs/altair/light-client/sync-protocol.md#get_subtree_index) to the `NEXT_SYNC_COMMITTEE_GINDEX` value.
pub const INDEX_NEXT_SYNC_COMMITTEE: u32 = 23;
/// According to Ethereum spec [v1.4.0](https://github.com/ethereum/consensus-specs/blob/v1.4.0/specs/altair/light-client/sync-protocol.md#constants).
///
/// It is the result of applying [get_generalized_index_length](https://github.com/ethereum/consensus-specs/blob/v1.4.0/ssz/merkle-proofs.md#get_generalized_index_length) to the `FINALIZED_ROOT_GINDEX` value.
pub const DEPTH_FINALITY: u32 = 6;
/// According to Ethereum spec [v1.4.0](https://github.com/ethereum/consensus-specs/blob/v1.4.0/specs/altair/light-client/sync-protocol.md#constants).
///
/// It is the result of applying [get_subtree_index](https://github.com/ethereum/consensus-specs/blob/v1.4.0/specs/altair/light-client/sync-protocol.md#get_subtree_index) to the `FINALIZED_ROOT_GINDEX` value.
pub const INDEX_FINALITY: u32 = 41;

/// According to Ethereum spec [v1.4.0](https://github.com/ethereum/consensus-specs/blob/v1.4.0/specs/phase0/beacon-chain.md#is_valid_merkle_branch).
pub fn is_valid_merkle_branch(
    leaf: [u8; 32],
    branch: &[[u8; 32]],
    depth: u32,
    index: u32,
    root: &[u8; 32],
) -> bool {
    let mut value = leaf;

    let mut hasher = RingContext::new(&RingSHA256);
    let mut iter = branch.iter();
    for i in 0..depth {
        let Some(next_node) = iter.next() else {
            return false;
        };

        let (node_first, node_second) = match (index / 2u32.pow(i)) % 2 {
            0 => (value.as_ref(), next_node.as_ref()),
            _ => (next_node.as_ref(), value.as_ref()),
        };

        hasher.update(node_first);
        hasher.update(node_second);

        value.copy_from_slice(hasher.finish().as_ref());
        hasher = RingContext::new(&RingSHA256);
    }

    value == *root
}

pub fn is_current_committee_proof_valid(
    network: &Network,
    attested_header: &BeaconBlockHeader,
    current_committee: &SyncCommittee,
    current_committee_branch: &[[u8; 32]],
) -> bool {
    let (depth, index) = depth_index_current(network, attested_header.slot);
    let leaf_hash = current_committee.tree_hash_root();
    let state_root = attested_header.state_root;

    is_valid_merkle_branch(
        leaf_hash.0,
        current_committee_branch,
        depth,
        index,
        &state_root.0,
    )
}

pub fn is_finality_proof_valid(
    network: &Network,
    attested_header: &BeaconBlockHeader,
    finality_header: &BeaconBlockHeader,
    finality_branch: &[[u8; 32]],
) -> bool {
    let (depth, index) = depth_index_finality(network, attested_header.slot);
    let leaf_hash = finality_header.tree_hash_root();
    let state_root = attested_header.state_root;

    is_valid_merkle_branch(leaf_hash.0, finality_branch, depth, index, &state_root.0)
}

pub fn is_next_committee_proof_valid(
    network: &Network,
    attested_header: &BeaconBlockHeader,
    next_committee: &SyncCommittee,
    next_committee_branch: &[[u8; 32]],
) -> bool {
    let (depth, index) = depth_index_next(network, attested_header.slot);
    let leaf_hash = next_committee.tree_hash_root();
    let state_root = attested_header.state_root;

    is_valid_merkle_branch(
        leaf_hash.0,
        next_committee_branch,
        depth,
        index,
        &state_root.0,
    )
}

/// According to Ethereum spec [v1.5.0](https://github.com/ethereum/consensus-specs/blob/v1.5.0-beta.2/specs/electra/light-client/sync-protocol.md#helper-functions).
///
/// Returns (`depth`, `index`) pair of the generalized merkle index to check a merkle proof of
/// a current sync committee.
pub const fn depth_index_current(network: &Network, slot: u64) -> (u32, u32) {
    let epoch_electra = network.epoch_electra();
    let epoch = utils::calculate_epoch(slot);

    if epoch >= epoch_electra {
        return (
            electra::DEPTH_CURRENT_SYNC_COMMITTEE,
            electra::INDEX_CURRENT_SYNC_COMMITTEE,
        );
    }

    (DEPTH_CURRENT_SYNC_COMMITTEE, INDEX_CURRENT_SYNC_COMMITTEE)
}

/// According to Ethereum spec [v1.5.0](https://github.com/ethereum/consensus-specs/blob/v1.5.0-beta.2/specs/electra/light-client/sync-protocol.md#helper-functions).
///
/// Returns (`depth`, `index`) pair of the generalized merkle index to check a merkle proof of
/// a next sync committee.
pub const fn depth_index_next(network: &Network, slot: u64) -> (u32, u32) {
    let epoch_electra = network.epoch_electra();
    let epoch = utils::calculate_epoch(slot);

    if epoch >= epoch_electra {
        return (
            electra::DEPTH_NEXT_SYNC_COMMITTEE,
            electra::INDEX_NEXT_SYNC_COMMITTEE,
        );
    }

    (DEPTH_NEXT_SYNC_COMMITTEE, INDEX_NEXT_SYNC_COMMITTEE)
}

/// According to Ethereum spec [v1.5.0](https://github.com/ethereum/consensus-specs/blob/v1.5.0-beta.2/specs/electra/light-client/sync-protocol.md#helper-functions).
///
/// Returns (`depth`, `index`) pair of the generalized merkle index to check a merkle proof of
/// a finality update.
pub const fn depth_index_finality(network: &Network, slot: u64) -> (u32, u32) {
    let epoch_electra = network.epoch_electra();
    let epoch = utils::calculate_epoch(slot);

    if epoch >= epoch_electra {
        return (electra::DEPTH_FINALITY, electra::INDEX_FINALITY);
    }

    (DEPTH_FINALITY, INDEX_FINALITY)
}

/// According to Ethereum spec [v1.5.0](https://github.com/ethereum/consensus-specs/blob/v1.5.0-beta.2/specs/electra/light-client/sync-protocol.md#new-constants).
///
/// The module contains new constants introduced by the Electra fork. For details see [`DEPTH_CURRENT_SYNC_COMMITTEE`],
/// [`INDEX_CURRENT_SYNC_COMMITTEE`], [`DEPTH_NEXT_SYNC_COMMITTEE`], [`INDEX_NEXT_SYNC_COMMITTEE`],
/// [`DEPTH_FINALITY`] and [`INDEX_FINALITY`].
pub mod electra {
    pub const DEPTH_CURRENT_SYNC_COMMITTEE: u32 = 6;
    pub const INDEX_CURRENT_SYNC_COMMITTEE: u32 = 22;
    pub const DEPTH_NEXT_SYNC_COMMITTEE: u32 = 6;
    pub const INDEX_NEXT_SYNC_COMMITTEE: u32 = 23;
    pub const DEPTH_FINALITY: u32 = 7;
    pub const INDEX_FINALITY: u32 = 41;
}
