use async_trait::async_trait;

use crate::traits::core::Async;
use crate::traits::ibc_event_context::IbcEventContext;
use crate::traits::ibc_message_sender::{
    IbcMessageSenderContext, IbcMessageSenderExt, MismatchIbcEventsCountError,
};
use crate::traits::messages::receive_packet::{ReceivePacketMessageBuilder, ReceivePacketRelayer};
use crate::traits::target::DestinationTarget;
use crate::types::aliases::{Height, Packet, WriteAcknowledgementEvent};

pub struct BaseReceivePacketRelayer;

#[async_trait]
impl<Context, Error, Message, Event, AckEvent> ReceivePacketRelayer<Context>
    for BaseReceivePacketRelayer
where
    Context: ReceivePacketMessageBuilder<Context>,
    Context: IbcMessageSenderContext<DestinationTarget, Error = Error>,
    Context::DstChain: IbcEventContext<
        Context::SrcChain,
        IbcMessage = Message,
        IbcEvent = Event,
        WriteAcknowledgementEvent = AckEvent,
    >,
    Context::Error: From<MismatchIbcEventsCountError>,
    Message: Async,
    AckEvent: TryFrom<Event>,
{
    async fn relay_receive_packet(
        &self,
        context: &Context,
        source_height: &Height<Context::SrcChain>,
        packet: &Packet<Context>,
    ) -> Result<Option<WriteAcknowledgementEvent<Context::DstChain, Context::SrcChain>>, Error>
    {
        let message = context
            .build_receive_packet_message(source_height, &packet)
            .await?;

        let events = context.send_message(message).await?;

        let ack_event = events.into_iter().find_map(|ev| ev.try_into().ok());

        Ok(ack_event)
    }
}
