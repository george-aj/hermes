use async_trait::async_trait;

use crate::base::one_for_all::traits::chain::OfaChainTypes;
use crate::base::one_for_all::traits::relay::OfaBaseRelay;
use crate::base::one_for_all::traits::relay::OfaRelayPreset;
use crate::base::relay::traits::packet_relayers::receive_packet::{
    CanRelayReceivePacket, ReceivePacketRelayer,
};
use crate::common::one_for_all::types::relay::OfaRelayWrapper;
use crate::std_prelude::*;

#[async_trait]
impl<Relay, Preset> CanRelayReceivePacket for OfaRelayWrapper<Relay>
where
    Relay: OfaBaseRelay<Preset = Preset>,
    Preset: OfaRelayPreset<Relay>,
{
    async fn relay_receive_packet(
        &self,
        source_height: &<Relay::SrcChain as OfaChainTypes>::Height,
        packet: &Relay::Packet,
    ) -> Result<Option<<Relay::DstChain as OfaChainTypes>::WriteAcknowledgementEvent>, Relay::Error>
    {
        Preset::ReceivePacketRelayer::relay_receive_packet(self, source_height, packet).await
    }
}