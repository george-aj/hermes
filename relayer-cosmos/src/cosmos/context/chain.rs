use ibc::core::ics04_channel::events::WriteAcknowledgement;
use ibc::core::ics04_channel::packet::Sequence;
use ibc::core::ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId};
use ibc::events::IbcEventType;
use ibc::signer::Signer;
use ibc::timestamp::Timestamp;
use ibc::Height;
use ibc_relayer::chain::cosmos::types::config::TxConfig;
use ibc_relayer::chain::cosmos::types::events::channel::extract_packet_and_write_ack_from_tx;
use ibc_relayer::keyring::KeyEntry;
use ibc_relayer_framework::traits::chain_context::{ChainContext, IbcChainContext};
use ibc_relayer_framework::traits::core::Async;
use ibc_relayer_framework::traits::core::ErrorContext;
use ibc_relayer_framework::traits::ibc_event_context::IbcEventContext;
use ibc_relayer_framework::traits::runtime::context::RuntimeContext;
use tendermint::abci::responses::Event;
use tendermint::abci::Event as AbciEvent;

use crate::cosmos::context::runtime::CosmosRuntimeContext;
use crate::cosmos::error::Error;
use crate::cosmos::message::CosmosIbcMessage;

#[derive(Clone)]
pub struct CosmosChainContext<Handle> {
    pub handle: Handle,
    pub signer: Signer,
    pub tx_config: TxConfig,
    pub key_entry: KeyEntry,
}

pub struct WriteAcknowledgementEvent(pub WriteAcknowledgement);

impl<Handle: Async> ErrorContext for CosmosChainContext<Handle> {
    type Error = Error;
}

impl<Handle: Async> RuntimeContext for CosmosChainContext<Handle> {
    type Runtime = CosmosRuntimeContext;

    fn runtime(&self) -> &CosmosRuntimeContext {
        &CosmosRuntimeContext
    }
}

impl<Handle: Async> ChainContext for CosmosChainContext<Handle> {
    type Height = Height;

    type Timestamp = Timestamp;

    type Message = CosmosIbcMessage;

    type Event = Event;
}

impl<Chain, Counterparty> IbcChainContext<CosmosChainContext<Counterparty>>
    for CosmosChainContext<Chain>
where
    Chain: Async,
    Counterparty: Async,
{
    type ClientId = ClientId;

    type ConnectionId = ConnectionId;

    type ChannelId = ChannelId;

    type PortId = PortId;

    type Sequence = Sequence;

    type IbcMessage = CosmosIbcMessage;

    type IbcEvent = Event;
}

impl TryFrom<AbciEvent> for WriteAcknowledgementEvent {
    type Error = ();

    fn try_from(event: AbciEvent) -> Result<Self, ()> {
        if let Ok(IbcEventType::WriteAck) = event.type_str.parse() {
            let (packet, write_ack) =
                extract_packet_and_write_ack_from_tx(&event).map_err(|_| ())?;

            let ack = WriteAcknowledgement {
                height: Height::new(0, 1).unwrap(),
                packet,
                ack: write_ack,
            };

            Ok(WriteAcknowledgementEvent(ack))
        } else {
            Err(())
        }
    }
}

impl<Chain, Counterparty> IbcEventContext<CosmosChainContext<Counterparty>>
    for CosmosChainContext<Chain>
where
    Chain: Async,
    Counterparty: Async,
{
    type WriteAcknowledgementEvent = WriteAcknowledgementEvent;
}