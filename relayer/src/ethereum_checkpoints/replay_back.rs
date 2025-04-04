use super::*;
use ethereum_beacon_client::{self, BeaconClient};

pub struct Args<'a> {
    pub beacon_client: &'a BeaconClient,
    pub client: &'a GearApi,
    pub program_id: [u8; 32],
    pub gas_limit: u64,
    pub replay_back: Option<ReplayBack>,
    pub checkpoint: (Slot, Hash256),
    pub sync_update: SyncCommitteeUpdate,
    pub size_batch: u64,
}

pub async fn execute(args: Args<'_>) -> AnyResult<()> {
    let Args {
        beacon_client,
        client,
        program_id,
        gas_limit,
        replay_back,
        checkpoint,
        sync_update,
        size_batch,
    } = args;
    log::info!("Replaying back started");

    let (mut slot_start, _) = checkpoint;
    if let Some(ReplayBack {
        finalized_header,
        last_header: slot_end,
    }) = replay_back
    {
        let slots_batch_iter = SlotsBatchIter::new(slot_start, slot_end, size_batch)
            .ok_or(anyhow!("Failed to create slots_batch::Iter with slot_start = {slot_start}, slot_end = {slot_end}."))?;

        replay_back_slots(
            beacon_client,
            client,
            program_id,
            gas_limit,
            slots_batch_iter,
        )
        .await?;

        slot_start = finalized_header;
    }

    let period_start = 1 + eth_utils::calculate_period(slot_start);
    let updates = beacon_client
        .get_updates(period_start, MAX_REQUEST_LIGHT_CLIENT_UPDATES)
        .await
        .map_err(|e| anyhow!("Failed to get updates for period {period_start}: {e:?}"))?;

    let slot_last = sync_update.finalized_header.slot;
    for update in updates {
        let slot_end = update.data.finalized_header.slot;
        let mut slots_batch_iter = SlotsBatchIter::new(slot_start, slot_end, size_batch)
            .ok_or(anyhow!("Failed to create slots_batch::Iter with slot_start = {slot_start}, slot_end = {slot_end}."))?;

        slot_start = slot_end;

        let signature = <G2 as ark_serialize::CanonicalDeserialize>::deserialize_compressed(
            &update.data.sync_aggregate.sync_committee_signature.0 .0[..],
        )
        .map_err(|e| anyhow!("Failed to deserialize point on G2 (replay back): {e:?}"))?;

        let sync_update =
            ethereum_beacon_client::utils::sync_update_from_update(signature, update.data);
        replay_back_slots_start(
            beacon_client,
            client,
            program_id,
            gas_limit,
            slots_batch_iter.next(),
            sync_update,
        )
        .await?;

        replay_back_slots(
            beacon_client,
            client,
            program_id,
            gas_limit,
            slots_batch_iter,
        )
        .await?;

        if slot_end == slot_last {
            // the provided sync_update is a sync committee update
            return Ok(());
        }
    }

    let mut slots_batch_iter = SlotsBatchIter::new(slot_start, slot_last, size_batch)
        .ok_or(anyhow!("Failed to create slots_batch::Iter with slot_start = {slot_start}, slot_last = {slot_last}."))?;

    replay_back_slots_start(
        beacon_client,
        client,
        program_id,
        gas_limit,
        slots_batch_iter.next(),
        sync_update,
    )
    .await?;

    replay_back_slots(
        beacon_client,
        client,
        program_id,
        gas_limit,
        slots_batch_iter,
    )
    .await?;

    log::info!("Replaying back finished");

    Ok(())
}

async fn replay_back_slots(
    beacon_client: &BeaconClient,
    client: &GearApi,
    program_id: [u8; 32],
    gas_limit: u64,
    slots_batch_iter: SlotsBatchIter,
) -> AnyResult<()> {
    for (slot_start, slot_end) in slots_batch_iter {
        log::debug!("slot_start = {slot_start}, slot_end = {slot_end}");
        replay_back_slots_inner(
            beacon_client,
            client,
            program_id,
            slot_start,
            slot_end,
            gas_limit,
        )
        .await?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn replay_back_slots_inner(
    beacon_client: &BeaconClient,
    client: &GearApi,
    program_id: [u8; 32],
    slot_start: Slot,
    slot_end: Slot,
    gas_limit: u64,
) -> AnyResult<()> {
    let payload = Handle::ReplayBack(beacon_client.request_headers(slot_start, slot_end).await?);

    let mut listener = client.subscribe().await?;

    let (message_id, _) = client
        .send_message(program_id.into(), payload, gas_limit, 0)
        .await
        .map_err(|e| anyhow!("Failed to send ReplayBack message: {e:?}"))?;

    let (_message_id, payload, _value) = listener
        .reply_bytes_on(message_id)
        .await
        .map_err(|e| anyhow!("Failed to get reply to ReplayBack message: {e:?}"))?;
    let payload =
        payload.map_err(|e| anyhow!("Failed to get replay payload to ReplayBack: {e:?}"))?;
    let result_decoded = HandleResult::decode(&mut &payload[..])
        .map_err(|e| anyhow!("Failed to decode HandleResult of ReplayBack: {e:?}"))?;

    log::debug!("replay_back_slots_inner; result_decoded = {result_decoded:?}");

    match result_decoded {
        HandleResult::ReplayBack(Some(_)) => Ok(()),
        HandleResult::ReplayBack(None) => Err(anyhow!("Replaying back wasn't started")),
        _ => Err(anyhow!("Wrong handle result to ReplayBack")),
    }
}

#[allow(clippy::too_many_arguments)]
async fn replay_back_slots_start(
    beacon_client: &BeaconClient,
    client: &GearApi,
    program_id: [u8; 32],
    gas_limit: u64,
    slots: Option<(Slot, Slot)>,
    sync_update: SyncCommitteeUpdate,
) -> AnyResult<()> {
    let Some((slot_start, slot_end)) = slots else {
        return Ok(());
    };

    let payload = Handle::ReplayBackStart {
        sync_update,
        headers: beacon_client.request_headers(slot_start, slot_end).await?,
    };

    let mut listener = client.subscribe().await?;

    let (message_id, _) = client
        .send_message(program_id.into(), payload, gas_limit, 0)
        .await
        .map_err(|e| anyhow!("Failed to send ReplayBackStart message: {e:?}"))?;

    let (_message_id, payload, _value) = listener
        .reply_bytes_on(message_id)
        .await
        .map_err(|e| anyhow!("Failed to get reply to ReplayBackStart message: {e:?}"))?;
    let payload =
        payload.map_err(|e| anyhow!("Failed to get replay payload to ReplayBackStart: {e:?}"))?;
    let result_decoded = HandleResult::decode(&mut &payload[..])
        .map_err(|e| anyhow!("Failed to decode HandleResult of ReplayBackStart: {e:?}"))?;

    log::debug!("replay_back_slots_start; result_decoded = {result_decoded:?}");

    match result_decoded {
        HandleResult::ReplayBackStart(Ok(_)) => Ok(()),
        HandleResult::ReplayBackStart(Err(e)) => Err(anyhow!("ReplayBackStart failed: {e:?}")),
        _ => Err(anyhow!("Wrong handle result to ReplayBackStart")),
    }
}
