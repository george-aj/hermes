use color_eyre::eyre::Context;
use prost::Message;
use sqlx::PgPool;
use tracing::{info, trace};

use tendermint_rpc::abci::transaction::Hash;
use tendermint_rpc::abci::{self, responses::Codespace, Data, tag::Tag, Event, Gas, Info, Log};
use tendermint_proto::abci::TxResult;
use tendermint_rpc::endpoint::tx::Response as ResultTx;
use tendermint_rpc::endpoint::tx_search::Response as TxSearchResponse;

use ibc_relayer_types::core::ics02_client::height::Height;
use ibc_relayer_types::core::ics04_channel::events::{SendPacket, WriteAcknowledgement};
use ibc_relayer_types::core::ics04_channel::packet::Packet;
use ibc_relayer_types::core::ics24_host::identifier::ChainId;
use ibc_relayer_types::events::{self, IbcEvent, WithBlockDataType};
use ibc_relayer_types::Height as ICSHeight;

use crate::chain::cosmos::types::events::channel::parse_timeout_height;
use crate::chain::cosmos::types::events::from_tx_response_event;
use crate::chain::cosmos::types::tx::{TxStatus, TxSyncResult};

use crate::chain::requests::*;

use crate::error::Error;
use crate::event::IbcEventWithHeight;
use crate::snapshot::SnapshotStore;

/// This function queries transactions for events matching certain criteria.
/// 1. Client Update request - returns a vector with at most one update client event
/// 2. Packet event request - returns at most one packet event for each sequence specified
///    in the request.
fn filter_matching_event(
    event: Event,
    height: Height,
    request: &QueryPacketEventDataRequest,
) -> Option<IbcEventWithHeight> {
    fn matches_packet(request: &QueryPacketEventDataRequest, packet: &Packet) -> bool {
        packet.source_port == request.source_port_id
            && packet.source_channel == request.source_channel_id
            && packet.destination_port == request.destination_port_id
            && packet.destination_channel == request.destination_channel_id
            && request.sequences.contains(&packet.sequence)
    }

    if event.type_str != request.event_id.as_str() {
        return None;
    }

    let ibc_event = from_tx_response_event(height, &event)?;

    match ibc_event.event {
        IbcEvent::SendPacket(ref send_ev) if matches_packet(request, &send_ev.packet) => {
            Some(ibc_event)
        }
        IbcEvent::WriteAcknowledgement(ref ack_ev) if matches_packet(request, &ack_ev.packet) => {
            Some(ibc_event)
        }
        _ => None,
    }
}

pub fn all_ibc_events_from_tx_search_response(
    height: ICSHeight,
    result: abci::DeliverTx,
) -> Vec<IbcEventWithHeight> {
    let mut events = vec![];
    for event in result.events {
        if let Some(ibc_ev) = from_tx_response_event(height, &event) {
            events.push(ibc_ev);
        }
    }
    events
}

fn event_attribute_to_tag(a: tendermint_proto::abci::EventAttribute) -> Result<Tag, Error> {
    let key = String::from_utf8(Vec::from(a.key)).unwrap();
    let value = String::from_utf8(Vec::from(a.value)).unwrap();

    Ok(Tag {
        key: key.into(),
        value: value.into(),
    })
}

fn proto_to_abci_event(e: tendermint_proto::abci::Event) -> Result<Event, Error> {
    let attributes = e
        .attributes
        .into_iter()
        .filter_map(|a| event_attribute_to_tag(a).ok())
        .collect::<Vec<Tag>>();

    Ok(Event {
        type_str: e.r#type,
        attributes,
    })
}

pub fn proto_to_deliver_tx(
    deliver_tx: tendermint_proto::abci::ResponseDeliverTx,
) -> Result<abci::DeliverTx, Error> {
    let events = deliver_tx
        .events
        .into_iter()
        .filter_map(|r| proto_to_abci_event(r).ok())
        .collect();

    Ok(abci::DeliverTx {
        code: deliver_tx.code.into(),
        data: Data::from(Vec::from(deliver_tx.data)),
        log: Log::new(deliver_tx.log),
        info: Info::new(deliver_tx.info),
        gas_wanted: Gas::from(deliver_tx.gas_wanted as u64),
        gas_used: Gas::from(deliver_tx.gas_used as u64),
        codespace: Codespace::new(deliver_tx.codespace),
        events,
    })
}

#[derive(Debug, sqlx::FromRow)]
struct SqlTxResult {
    tx_hash: String,
    tx_result: Vec<u8>,
}

async fn tx_result_by_hash(pool: &PgPool, hash: &str) -> Result<TxResult, Error> {
    let bytes = sqlx::query_scalar::<_, Vec<u8>>(
        "SELECT tx_result FROM tx_results WHERE tx_hash = $1 LIMIT 1",
    )
    .bind(hash)
    .fetch_one(pool)
    .await
    .map_err(Error::sqlx)?;

    let tx_result = TxResult::decode(bytes.as_slice())
        .wrap_err("failed to decode tx result")
        .unwrap();

    Ok(tx_result)
}

async fn tx_result_by_header_fields(
    pool: &PgPool,
    search: &QueryClientEventRequest,
) -> Result<(TxResult, String), Error> {
    let result = sqlx::query_as::<_, SqlTxResult>(
        "SELECT tx_hash, tx_result \
        FROM ibc_tx_client_events WHERE \
        type = $1 and \
        client_id = $2 and \
        consensus_height = $3 \
        LIMIT 1",
    )
    .bind(search.event_id.as_str())
    .bind(search.client_id.as_str())
    .bind(format!("{}", search.consensus_height.revision_height()))
    .fetch_one(pool)
    .await
    .map_err(Error::sqlx)?;

    let tx_result = tendermint_proto::abci::TxResult::decode(result.tx_result.as_slice())
        .wrap_err("failed to decode tx result")
        .unwrap();

    Ok((tx_result, result.tx_hash))
}

#[tracing::instrument(skip(pool))]
pub async fn header_search(
    pool: &PgPool,
    search: &QueryClientEventRequest,
) -> Result<TxSearchResponse, Error> {
    info!(
        client_id = %search.client_id,
        consensus_height = %search.consensus_height,
        "got header search"
    );

    let (raw_tx_result, hash) = tx_result_by_header_fields(pool, search).await?;
    let deliver_tx = raw_tx_result.result.unwrap();
    let tx_result = proto_to_deliver_tx(deliver_tx)?;

    trace!(tx_result.events = ? &tx_result.events, "got events");

    let txs = vec![ResultTx {
        hash: hash.parse().unwrap(), // TODO: validate hash earlier
        height: raw_tx_result.height.try_into().unwrap(),
        index: raw_tx_result.index,
        tx_result,
        tx: tendermint_rpc::abci::Transaction::from(Vec::from(raw_tx_result.tx)),
        proof: None,
    }];

    Ok(TxSearchResponse {
        txs,
        total_count: 1,
    })
}

// Extracts from the Tx the update client event for the requested client and height.
// Note: in the Tx, there may have been multiple events, some of them may be
// for update of other clients that are not relevant to the request.
// For example, if we're querying for a transaction that includes the update for client X at
// consensus height H, it is possible that the transaction also includes an update client
// for client Y at consensus height H'. This is the reason the code iterates all event fields in the
// returned Tx to retrieve the relevant ones.
// Returns `None` if no matching event was found.
fn update_client_events_from_tx_search_response(
    chain_id: &ChainId,
    request: &QueryClientEventRequest,
    response: ResultTx,
) -> Option<IbcEventWithHeight> {
    let height = ICSHeight::new(chain_id.version(), u64::from(response.height)).unwrap();
    if let QueryHeight::Specific(query_height) = request.query_height {
        if height > query_height {
            return None;
        }
    }

    response
        .tx_result
        .events
        .into_iter()
        .filter(|event| event.type_str == request.event_id.as_str())
        .flat_map(|event| from_tx_response_event(height, &event))
        .flat_map(|event| match event.event {
            IbcEvent::UpdateClient(update) => Some(update),
            _ => None,
        })
        .find(|update| {
            update.common.client_id == request.client_id
                && update.common.consensus_height == request.consensus_height
        })
        .map(|update| IbcEventWithHeight::new(IbcEvent::UpdateClient(update), height))
}

async fn tx_results_by_packet_fields(
    pool: &PgPool,
    search: &QueryPacketEventDataRequest,
) -> Result<Vec<(i64, TxResult, String)>, Error> {
    // Convert from `[Sequence(1), Sequence(2)]` to String `"('1', '2')"`
    let seqs = search
        .clone()
        .sequences
        .into_iter()
        .map(|i| format!("'{}'", i))
        .collect::<Vec<String>>();
    let seqs_string = format!("({})", seqs.join(", "));

    let sql_select_string = format!(
        "SELECT DISTINCT tx_hash, tx_result FROM ibc_tx_packet_events WHERE \
        packet_sequence IN {} and \
        type = $1 and \
        packet_src_channel = $2 and \
        packet_src_port = $3",
        seqs_string
    );

    let results = sqlx::query_as::<_, SqlTxResult>(sql_select_string.as_str())
        .bind(search.event_id.as_str())
        .bind(search.source_channel_id.to_string())
        .bind(search.source_port_id.to_string())
        .fetch_all(pool)
        .await
        .map_err(Error::sqlx)?;

    let tx_result = results
        .into_iter()
        .map(|result| {
            let tx_res = tendermint_proto::abci::TxResult::decode(result.tx_result.as_slice())
                .wrap_err("failed to decode tx result")
                .unwrap();
            (tx_res.height, tx_res, result.tx_hash)
        })
        .collect();

    Ok(tx_result)
}

#[tracing::instrument(skip(pool))]
pub async fn tx_search_response_from_packet_query(
    pool: &PgPool,
    search: &QueryPacketEventDataRequest,
) -> Result<TxSearchResponse, Error> {
    trace!("tx_search_response_from_packet_query");

    let results = tx_results_by_packet_fields(pool, search).await?;
    let total_count = results.len() as u32;

    let txs = results
        .into_iter()
        .map(|result| {
            let (height, raw_tx_result, hash) = result;
            let deliver_tx = raw_tx_result.result.unwrap();
            trace!(tx_result.events = ? &deliver_tx.events, "got events");

            let tx_result = proto_to_deliver_tx(deliver_tx).unwrap();

            ResultTx {
                hash: hash.parse().unwrap(),
                height: height.try_into().unwrap(),
                index: raw_tx_result.index,
                tx_result,
                tx: tendermint_rpc::abci::Transaction::from(Vec::from(raw_tx_result.tx)),
                proof: None,
            }
        })
        .collect();

    Ok(TxSearchResponse { txs, total_count })
}

// Extract the packet events from the query_txs RPC responses.
fn packet_events_from_tx_search_response(
    chain_id: &ChainId,
    request: &QueryPacketEventDataRequest,
    responses: Vec<ResultTx>,
) -> Vec<IbcEventWithHeight> {
    let mut events = vec![];

    for response in responses {
        let height = ICSHeight::new(chain_id.version(), u64::from(response.height)).unwrap();

        if let QueryHeight::Specific(specific_query_height) = request.height.get() {
            if height > specific_query_height {
                continue;
            }
        };

        let mut new_events = response
            .tx_result
            .events
            .into_iter()
            .filter_map(|ev| filter_matching_event(ev, height, request))
            .collect::<Vec<_>>();

        events.append(&mut new_events)
    }

    events
}

#[tracing::instrument(skip(pool))]
pub async fn query_packets_from_tendermint(
    pool: &PgPool,
    chain_id: &ChainId,
    request: &mut QueryPacketEventDataRequest,
) -> Result<Vec<IbcEventWithHeight>, Error> {
    crate::time!("query_packets_from_tendermint: query packet events");

    // Get the txs from the Tx events.
    let responses = tx_search_response_from_packet_query(pool, request).await?;
    // Extract the Tx packet events. Filter out the ones that don't match the request height.
    let mut tx_events = packet_events_from_tx_search_response(chain_id, request, responses.txs);

    let recvd_sequences: Vec<_> = tx_events
        .iter()
        .filter_map(|eh| eh.event.packet().map(|p| p.sequence))
        .collect();

    request
        .sequences
        .retain(|seq| !recvd_sequences.contains(seq));

    // For the rest of the sequences try to get the events from the block events
    let mut block_events = vec![];
    if !request.sequences.is_empty() {
        block_events = block_search_response_from_packet_query(pool, chain_id, request).await?;
    }

    tx_events.append(&mut block_events);
    Ok(tx_events)
}

#[tracing::instrument(skip(pool))]
pub async fn query_txs_from_tendermint(
    pool: &PgPool,
    chain_id: &ChainId,
    search: &QueryTxRequest,
) -> Result<Vec<IbcEventWithHeight>, Error> {
    match search {
        QueryTxRequest::Client(request) => {
            let mut response = header_search(pool, request).await?;
            if response.txs.is_empty() {
                return Ok(vec![]);
            }

            // the response must include a single Tx as specified in the query.
            assert!(
                response.txs.len() <= 1,
                "client_event_from_tx_search_response: unexpected number of txs"
            );

            let tx = response.txs.remove(0);

            let event = update_client_events_from_tx_search_response(chain_id, request, tx);

            Ok(event.into_iter().collect())
        }

        QueryTxRequest::Transaction(tx) => {
            let hash = tx.0.to_string();
            let raw_tx_result = tx_result_by_hash(pool, hash.as_str()).await?;
            let height =
                ICSHeight::new(chain_id.version(), raw_tx_result.height.try_into().unwrap())
                    .unwrap();

            let deliver_tx = raw_tx_result.result.unwrap();

            let tx_result = proto_to_deliver_tx(deliver_tx)?;
            if tx_result.code.is_err() {
                return Ok(vec![IbcEventWithHeight::new(
                    IbcEvent::ChainError(format!(
                        "deliver_tx for {} reports error: code={:?}, log={:?}",
                        hash, tx_result.code, tx_result.log
                    )),
                    height,
                )]);
            }

            Ok(all_ibc_events_from_tx_search_response(height, tx_result))
        }
    }
}

#[tracing::instrument(skip(snapshot))]
pub async fn query_packets_from_ibc_snapshots(
    pool: &PgPool,
    snapshot: &dyn SnapshotStore,
    chain_id: &ChainId,
    request: &mut QueryPacketEventDataRequest,
) -> Result<Vec<IbcEventWithHeight>, Error> {
    crate::time!("query_packets_from_ibc_snapshots");
    match request.event_id {
        // Only query for sent packet events is currently supported with snapshots.
        WithBlockDataType::SendPacket => {
            let (height, all_packets) = snapshot.query_sent_packets(request.height.get()).await?;

            let events = all_packets
                .into_iter()
                .filter_map(|packet| {
                    if packet.source_port == request.source_port_id
                        && packet.source_channel == request.source_channel_id
                        && request.sequences.contains(&packet.sequence)
                    {
                        Some(IbcEventWithHeight::new(
                            IbcEvent::SendPacket(SendPacket { packet }),
                            height,
                        ))
                    } else {
                        None
                    }
                })
                .collect();
            Ok(events)
        }
        // All other queries go to the chain for now.
        _ => query_packets_from_tendermint(pool, chain_id, request).await,
    }
}

//#[tracing::instrument(skip(pool))]
pub async fn query_txs_from_ibc_snapshots(
    pool: &PgPool,
    chain_id: &ChainId,
    search: &QueryTxRequest,
) -> Result<Vec<IbcEventWithHeight>, Error> {
    match search {
        // TODO - implement this to actually query for client updates or Tx hash from snapshots.
        QueryTxRequest::Client(request) => query_txs_from_tendermint(pool, chain_id, search).await,
        QueryTxRequest::Transaction(tx) => query_txs_from_tendermint(pool, chain_id, search).await,
    }
}

async fn abci_tx_results_by_hashes(
    pool: &PgPool,
    hashes: Vec<Hash>,
) -> Result<Vec<(i64, TxResult, String)>, Error> {
    // Convert from `[Sequence(1), Sequence(2)]` to String `"('1', '2')"`
    let hash_string = hashes
        .into_iter()
        .map(|i| format!("'{}'", i))
        .collect::<Vec<String>>();
    let hashes_psql = format!("({})", hash_string.join(", "));
    let sql_select_string = format!(
        "SELECT DISTINCT tx_hash, tx_result FROM tx_results WHERE \
        tx_hash IN {}",
        hashes_psql
    );

    let results = sqlx::query_as::<_, SqlTxResult>(sql_select_string.as_str())
        .fetch_all(pool)
        .await
        .map_err(Error::sqlx)?;

    let tx_result = results
        .into_iter()
        .map(|result| {
            let tx_res = tendermint_proto::abci::TxResult::decode(result.tx_result.as_slice())
                .wrap_err("failed to decode tx result")
                .unwrap();
            (tx_res.height, tx_res, result.tx_hash)
        })
        .collect();

    Ok(tx_result)
}

async fn rpc_tx_results_by_hashes(
    pool: &PgPool,
    hashes: Vec<Hash>,
) -> Result<TxSearchResponse, Error> {
    trace!("search_pending_txs_by_hashes {:?}", hashes);

    let results = abci_tx_results_by_hashes(pool, hashes).await?;
    let total_count = results.len() as u32;

    let txs = results
        .into_iter()
        .map(|result| {
            let (height, raw_tx_result, hash) = result;
            let deliver_tx = raw_tx_result.result.unwrap();
            trace!(tx_result.events = ? &deliver_tx.events, "got events");

            let tx_result = proto_to_deliver_tx(deliver_tx).unwrap();

            ResultTx {
                hash: hash.parse().unwrap(),
                height: height.try_into().unwrap(),
                index: raw_tx_result.index,
                tx_result,
                tx: tendermint_rpc::abci::Transaction::from(Vec::from(raw_tx_result.tx)),
                proof: None,
            }
        })
        .collect();

    Ok(TxSearchResponse { txs, total_count })
}

fn all_ibc_events_from_tx_result_batch(
    chain_id: &ChainId,
    responses: Vec<ResultTx>,
) -> Vec<Vec<IbcEventWithHeight>> {
    let mut events = vec![];

    for response in responses {
        let height = ICSHeight::new(chain_id.version(), u64::from(response.height)).unwrap();

        let new_events = if response.tx_result.code.is_err() {
            vec![IbcEventWithHeight::new(
                IbcEvent::ChainError(format!(
                    "deliver_tx on chain {} for Tx hash {} reports error: code={:?}, log={:?}",
                    chain_id, response.hash, response.tx_result.code, response.tx_result.log
                )),
                height,
            )]
        } else {
            response
                .tx_result
                .events
                .into_iter()
                .filter_map(|ev| from_tx_response_event(height, &ev))
                .collect::<Vec<_>>()
        };
        events.push(new_events)
    }
    events
}

#[tracing::instrument(skip(pool, tx_sync_results))]
pub async fn query_hashes_and_update_tx_sync_events(
    pool: &PgPool,
    chain_id: &ChainId,
    tx_sync_results: &mut [TxSyncResult],
) -> Result<(), Error> {
    // get the hashes of the transactions for which events have not been retrieved yet
    let unsolved_hashes = tx_sync_results
        .iter_mut()
        .filter(|result| matches!(result.status, TxStatus::Pending { .. }))
        .map(|res| res.response.hash)
        .collect();

    // query the chain with all unsolved hashes
    let responses = rpc_tx_results_by_hashes(pool, unsolved_hashes).await?;

    // get the hashes for found transactions
    let solved_hashes = responses
        .txs
        .iter()
        .map(|res| res.hash)
        .collect::<Vec<Hash>>();

    if solved_hashes.is_empty() {
        return Ok(());
    }

    // extract the IBC events from all transactions that were solved
    let solved_txs_events = all_ibc_events_from_tx_result_batch(chain_id, responses.txs);

    // get the pending results for the solved transactions where the results should be stored
    let mut solved_results = tx_sync_results
        .iter_mut()
        .filter(|result| solved_hashes.contains(&result.response.hash))
        .collect::<Vec<&mut TxSyncResult>>();

    for (tx_sync_result, events) in solved_results.iter_mut().zip(solved_txs_events.iter()) {
        // Transaction was included in a block. Check if it was an error.
        let tx_chain_error = events
            .iter()
            .find(|event| matches!(event.event, IbcEvent::ChainError(_)));

        if let Some(err) = tx_chain_error {
            // Save the error for all messages in the transaction
            tx_sync_result.events = vec![err.clone(); tx_sync_result.events.len()];
        } else {
            tx_sync_result.events = events.clone();
        }

        tx_sync_result.status = TxStatus::ReceivedResponse;
    }
    Ok(())
}

#[tracing::instrument(skip(pool, tx_sync_results))]
pub async fn query_hashes_and_update_tx_sync_results(
    pool: &PgPool,
    chain_id: &ChainId,
    tx_sync_results: &mut [TxSyncResult],
) -> Result<(), Error> {
    for result in tx_sync_results.iter_mut() {
        if result.response.code.is_err() {
            let height = Height::new(1, 1).unwrap(); // FIXME

            result.events = vec![IbcEventWithHeight::new(IbcEvent::ChainError(format!(
                "check_tx (broadcast_tx_sync) on chain {} for Tx hash {} reports error: code={:?}, log={:?}",
                chain_id, result.response.hash, result.response.code, result.response.log)), height); result.events.len()]
        }
    }

    query_hashes_and_update_tx_sync_events(pool, chain_id, tx_sync_results).await
}

#[derive(Debug, sqlx::FromRow)]
struct SqlPacketBlockEvents {
    block_id: i64,
    r#type: String,
    packet_src_port: String,
    packet_sequence: String,
    packet_dst_port: String,
    packet_dst_channel: String,
    packet_src_channel: String,
    packet_timeout_height: String,
    packet_timeout_timestamp: String,
    packet_data: String,
    packet_ack: String,
}

async fn block_results_by_packet_fields(
    pool: &PgPool,
    search: &QueryPacketEventDataRequest,
) -> Result<Vec<SqlPacketBlockEvents>, Error> {
    // Convert from `[Sequence(1), Sequence(2)]` to String `"('1', '2')"`
    let seqs = search
        .clone()
        .sequences
        .into_iter()
        .map(|i| format!("'{}'", i))
        .collect::<Vec<String>>();
    let seqs_string = format!("({})", seqs.join(", "));

    let sql_select_string = format!(
        "SELECT DISTINCT * FROM ibc_block_events WHERE \
        packet_sequence IN {} and \
        type = $1 and \
        packet_src_channel = $2 and \
        packet_src_port = $3",
        seqs_string
    );

    let results = sqlx::query_as::<_, SqlPacketBlockEvents>(sql_select_string.as_str())
        .bind(search.event_id.as_str())
        .bind(search.source_channel_id.to_string())
        .bind(search.source_port_id.to_string())
        .fetch_all(pool)
        .await
        .map_err(Error::sqlx)?;

    Ok(results)
}

fn ibc_packet_event_from_sql_block_query(
    chain_id: &ChainId,
    event: &SqlPacketBlockEvents,
) -> Option<IbcEventWithHeight> {
    let height =
        ICSHeight::new(chain_id.version(), u64::try_from(event.block_id).unwrap()).unwrap();
    let packet = Packet {
        sequence: event.packet_sequence.parse().unwrap(),
        source_port: event.packet_src_port.parse().unwrap(),
        source_channel: event.packet_src_channel.parse().unwrap(),
        destination_port: event.packet_dst_port.parse().unwrap(),
        destination_channel: event.packet_dst_channel.parse().unwrap(),
        data: Vec::from(event.packet_data.as_bytes()),
        timeout_height: parse_timeout_height(&event.packet_timeout_height).unwrap(),
        timeout_timestamp: event.packet_timeout_timestamp.parse().unwrap(),
    };
    let ibc_event = match event.r#type.as_str() {
        events::SEND_PACKET_EVENT => Some(IbcEvent::SendPacket(SendPacket { packet })),
        events::WRITE_ACK_EVENT => Some(IbcEvent::WriteAcknowledgement(WriteAcknowledgement {
            packet,
            ack: Vec::from(event.packet_ack.as_bytes()),
        })),
        _ => None,
    };
    ibc_event.map(|ibc_event| IbcEventWithHeight::new(ibc_event, height))
}

#[tracing::instrument(skip(pool))]
pub async fn block_search_response_from_packet_query(
    pool: &PgPool,
    chain_id: &ChainId,
    request: &QueryPacketEventDataRequest,
) -> Result<Vec<IbcEventWithHeight>, Error> {
    trace!("block_search_response_from_packet_query");

    let results = block_results_by_packet_fields(pool, request).await?;
    let total_count = results.len() as u32;

    let events = results
        .into_iter()
        .filter_map(|result| ibc_packet_event_from_sql_block_query(chain_id, &result))
        .filter_map(|event| {
            let request_height = request.height.get();
            match request_height {
                QueryHeight::Latest => Some(event),
                QueryHeight::Specific(height) if event.height <= height => Some(event),
                _ => None,
            }
        })
        .collect();

    Ok(events)
}
